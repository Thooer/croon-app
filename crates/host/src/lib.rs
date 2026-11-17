//! 宿主：根在系统外的装配。按 host.config 打开 Kernel，装上 Lang0、模型与文件执行，
//! 创建 root entry，作为它的客户端交任务、取结果，再把结果快照物化到新目录。
//!
//! 调用方要知道的：
//!
//! - 同一个系统名必须始终配同一个 Kernel 目录与 state 目录；恢复身份由系统名派生，写进 Journal。
//! - 同一个 Kernel 目录再跑一次就是恢复：已有 root entry 时不再创建，按原键重报同样的几步；
//!   任务或仓库变了是重放分歧。
//! - 结果只写进一个新的或空的目录，不碰用户的仓库。续做用 [`resume`]：任务与导入的目录取自 Journal，不读磁盘。
//! - [`Assembly`] 里给的模型 Port，恢复身份必须是 `<系统名>/model`；故障点只用于测试。
//! - 有 Instance 被宿主停下时，[`run`] 带着停止原因返回错误，不会一直等；同一个 Kernel 目录
//!   再跑一次时先重开它们。业务失败是 Journal 里的事实，重跑只重放出同样的失败。
//!
//! 参考：docs/Host/README.md（装配、配置、root entry、客户端）。

mod blocks;
pub mod infer;
pub mod library;

use croon_port::store::{Budgets, Content};
use croon_port::{Bundle, CheckError, HostError, Port, Read};
use kernel::host::{Config as KernelConfig, Language};
pub use kernel::host::{Fault, FaultHook};
use kernel::kernel::{Conn, Kernel, KernelError};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn schema(name: &str) -> Json {
    blocks::json(name)
}

/// 宿主一侧的故障：配置、装配或运行中的任何一步停下。
#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

macro_rules! from_display {
    ($($t:ty),*) => {$(
        impl From<$t> for Error {
            fn from(e: $t) -> Self {
                Error(e.to_string())
            }
        }
    )*};
}
from_display!(
    KernelError,
    HostError,
    CheckError,
    std::io::Error,
    serde_json::Error
);

pub type Result<T> = std::result::Result<T, Error>;

const BUDGETS: Budgets = Budgets {
    nodes: 1_000_000,
    read_bytes: 1 << 30,
    write_bytes: 1 << 30,
};

/// 模型从哪里来：真 Provider、边调用边录制，或只回放录像。
#[derive(Clone)]
pub enum ModelSource {
    Provider(PathBuf),
    Record { provider: PathBuf, record: PathBuf },
    Replay(PathBuf),
}

/// 校验过的宿主配置。
#[derive(Clone)]
pub struct Settings {
    pub system: String,
    pub kernel: PathBuf,
    pub model: ModelSource,
    pub directory: directory::Config,
}

fn text(v: &Json, key: &str) -> Result<String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error(format!("宿主配置缺少 {key}")))
}

fn absolute(v: &Json, key: &str) -> Result<PathBuf> {
    let path = PathBuf::from(text(v, key)?);
    if !path.is_absolute() {
        return Err(Error(format!("宿主配置的 {key} 要写绝对路径")));
    }
    Ok(path)
}

fn number<T: TryFrom<u64>>(v: &Json, key: &str) -> Result<T> {
    v[key]
        .as_u64()
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| Error(format!("宿主配置的 {key} 超出范围")))
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        let value: Json = serde_json::from_slice(&std::fs::read(path)?)?;
        croon_eval::schema::validate(&schema("host.config"), &value)
            .map_err(|e| Error(format!("host.config: {e:?}")))?;
        let system = text(&value, "system")?;
        let m = &value["model"];
        let model = if m.get("replay").is_some() {
            ModelSource::Replay(absolute(m, "replay")?)
        } else if m.get("record").is_some() {
            ModelSource::Record {
                provider: absolute(m, "provider")?,
                record: absolute(m, "record")?,
            }
        } else {
            ModelSource::Provider(absolute(m, "provider")?)
        };
        let d = &value["directory"];
        let directory = directory::Config {
            identity: format!("{system}/directory"),
            state: absolute(d, "state")?,
            docker: absolute(d, "docker")?,
            python: absolute(d, "python")?,
            image: text(d, "image")?,
            collector_image: text(d, "collector_image")?,
            platform: text(d, "platform")?,
            cpus: text(d, "cpus")?,
            memory_bytes: number(d, "memory_bytes")?,
            work_bytes: number(d, "work_bytes")?,
            temporary_bytes: number(d, "temporary_bytes")?,
            pids: number(d, "pids")?,
            output_bytes: number(d, "output_bytes")?,
            nodes: number(d, "nodes")?,
            archive_bytes: number(d, "archive_bytes")?,
            cleanup_timeout: Duration::from_secs(number(d, "cleanup_timeout_seconds")?),
        };
        Ok(Self {
            kernel: absolute(&value, "kernel")?,
            system,
            model,
            directory,
        })
    }

    /// 配置里的模型来源；`resuming` 时录制是接着已有的录像追加。
    fn model_port(&self, resuming: bool) -> Result<Rc<dyn Port>> {
        let identity = format!("{}/model", self.system);
        Ok(match &self.model {
            ModelSource::Provider(provider) => Rc::new(model::OpenAiCompatible::from_config_file(
                provider, identity,
            )?),
            ModelSource::Record { provider, record } => {
                let inner = Rc::new(model::OpenAiCompatible::from_config_file(
                    provider, identity,
                )?);
                if resuming && record.exists() {
                    Rc::new(model::Recorder::resume(inner, record.clone())?)
                } else {
                    Rc::new(model::Recorder::new(inner, record.clone())?)
                }
            }
            ModelSource::Replay(cassette) => Rc::new(model::Replayer::open(cassette, identity)?),
        })
    }

    fn limits(&self) -> directory::Limits {
        directory::Limits {
            nodes: self.directory.nodes,
            bytes: self.directory.work_bytes,
        }
    }
}

/// 配置文件之外的运行参数，都可以不给。见 docs/Host/README.md「运行参数」。
#[derive(Default)]
pub struct Assembly {
    /// 代替配置里的模型来源；恢复身份必须是 `<系统名>/model`。
    pub model: Option<Rc<dyn Port>>,
    /// 原样交给 Kernel 的故障点，只用于测试。
    pub fault: Option<FaultHook>,
    /// 代替默认的工具清单：实验要测制造时不载入预置的子代理。要过 [`library::check_tools`]。
    pub tools: Option<Json>,
}

/// 一次运行的结果：W0 的总结与它最终采用的目录 Bundle。
pub struct Finished {
    pub summary: String,
    pub snapshot: String,
}

/// 代码读取按宿主自己的内容视图；宿主只读它导入的东西。
struct Code<'a>(&'a Content);
impl Read for Code<'_> {
    fn code_bytes(&self, id: &str) -> std::result::Result<Option<Vec<u8>>, HostError> {
        self.0.bytes(id)
    }
    fn code_json(&self, id: &str) -> std::result::Result<Option<Json>, HostError> {
        self.0.json(id)
    }
    fn code_bundle(&self, id: &str) -> std::result::Result<Option<Bundle>, HostError> {
        self.0.bundle(id)
    }
    fn bytes(&self, id: &str, _: &[String]) -> std::result::Result<Option<Vec<u8>>, HostError> {
        self.code_bytes(id)
    }
    fn json(&self, id: &str, _: &[String]) -> std::result::Result<Option<Json>, HostError> {
        self.code_json(id)
    }
    fn bundle(&self, id: &str, _: &[String]) -> std::result::Result<Option<Bundle>, HostError> {
        self.code_bundle(id)
    }
}

/// 把家目录装进临时目录，再导入成目录 Bundle：顶层是 library/ 与 tools.json，见 docs/Library/Packages.md。
fn home_bundle(content: &Content, limits: directory::Limits, tools: Option<&Json>) -> Result<String> {
    let root = std::env::temp_dir().join(format!(
        "croon-host-library-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    library::install(root.join("library"));
    match tools {
        Some(tools) => library::write_home_tools(&root, tools).map_err(Error)?,
        None => library::install_home_tools(&root),
    }
    let imported = directory::import(content, &root, limits);
    let _ = std::fs::remove_dir_all(&root);
    Ok(imported?)
}

/// 用 Library 里的一个模块作 body，推出它的 Interface，存成完整 Definition Bundle。
fn definition(content: &Content, source: &str, modules: &str) -> Result<String> {
    let mut def = json!({"language": "lang0@1", "body": {"source": source, "modules": modules}});
    def["interface"] = lang0::interface(&def, &Code(content))?;
    let raw = content.put_json(&def)?;
    Ok(content.pack(&raw, &[modules.to_owned()])?)
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids.dedup();
    ids
}

/// root entry 的客户端：每一步用固定的键报出，在途就推进别的 Instance 再用原键重报。
struct Client<'a> {
    kernel: &'a mut Kernel,
    conn: Conn,
}

impl Client<'_> {
    fn submit(&mut self, atom: &Json) -> Result<Json> {
        loop {
            if let Some(result) = self.kernel.submit(&self.conn, atom)? {
                return Ok(result);
            }
            let progress = self.kernel.run()?;
            // 被宿主停下的 Instance 要修好宿主、重开之后才会再走：这次运行跑不完，不再等。
            let stopped = self.kernel.stopped();
            if !stopped.is_empty() {
                let reasons: Vec<String> = stopped
                    .iter()
                    .map(|(instance, reason)| format!("{instance}：{}", reason.trim()))
                    .collect();
                return Err(Error(format!(
                    "有 Instance 被宿主停下，这次运行跑不完：{}",
                    reasons.join("；")
                )));
            }
            if !progress {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    fn send(
        &mut self,
        key: &str,
        to: Json,
        value: Json,
        dependencies: Vec<String>,
        attach: Json,
    ) -> Result<Json> {
        let atom = json!({"key": key, "send": {
            "to": to,
            "delivery": {"value": value, "dependencies": sorted(dependencies)},
            "attach": attach
        }});
        let result = self.submit(&atom)?;
        if result["outcome"]["ok"] != true {
            return Err(Error(format!("{key} 失败：{}", result["outcome"]["error"])));
        }
        Ok(result)
    }
}

/// 一次运行要做的事：运行哪一份 Definition，输入是什么。恢复时整份取自 Journal。
struct Job {
    /// 被运行的 Definition 的 Bundle ID。
    definition: String,
    /// 交给它的输入，目录字段已经填成导入后的 Bundle ID。
    input: Json,
    /// 输入用到的 Bundle，不含 Definition。
    dependencies: Vec<String>,
    /// 结束值里哪个字段是结果目录，空串是没有。
    directory: String,
}

/// 开始一次运行：W0 收一个任务，改调用方给的仓库，结果目录物化到新的或空的 `out`。这个 Kernel 目录里已经有
/// root entry 时报错：续做用 [`resume`]。
pub fn run(
    settings: &Settings,
    assembly: Assembly,
    repo: &Path,
    task: &str,
    out: &Path,
) -> Result<Finished> {
    let start = Start::W0 {
        repo: repo.to_owned(),
        task: task.to_owned(),
    };
    drive(settings, assembly, Some(start), out)
}

/// 构造并运行一份 Lang0 源码，不经 W0：输入照它的 Signature，`input` 是 JSON 对象；Signature 的 in 里有
/// `snapshot` 时，`repo` 是交给它的目录。结束值里有 `snapshot` 时物化到 `out`；没有就不建目录。
pub fn exec(
    settings: &Settings,
    assembly: Assembly,
    source: &str,
    input: Json,
    repo: Option<&Path>,
    out: &Path,
) -> Result<Finished> {
    let start = Start::Source {
        source: source.to_owned(),
        input,
        repo: repo.map(Path::to_owned),
    };
    drive(settings, assembly, Some(start), out)
}

/// 继续一次被中断的运行：运行什么、输入是什么、导入的目录与工具清单都取自 Journal，不读命令行与磁盘。
/// 已经结束的运行得到同样的结果。
pub fn resume(settings: &Settings, assembly: Assembly, out: &Path) -> Result<Finished> {
    drive(settings, assembly, None, out)
}

enum Start {
    W0 { repo: PathBuf, task: String },
    Source { source: String, input: Json, repo: Option<PathBuf> },
}

fn drive(
    settings: &Settings,
    assembly: Assembly,
    start: Option<Start>,
    out: &Path,
) -> Result<Finished> {
    let resuming = start.is_none();
    let model = match assembly.model {
        Some(model) => {
            let identity = format!("{}/model", settings.system);
            if model.id() != identity {
                return Err(Error(format!(
                    "运行参数里的模型恢复身份是 {}，要是 {identity}",
                    model.id()
                )));
            }
            model
        }
        None => settings.model_port(resuming)?,
    };
    let exec: Rc<dyn Port> = Rc::new(directory::Docker::new(settings.directory.clone())?);
    let inferrer: Rc<dyn Port> = Rc::new(infer::Infer::new(format!("{}/infer", settings.system)));
    let mut kernel = Kernel::open(
        &settings.kernel,
        KernelConfig {
            languages: vec![Box::new(lang0::Lang0) as Box<dyn Language>],
            budgets: BUDGETS,
            ports: BTreeMap::new(),
            clock: Box::new(now),
            fault: assembly.fault,
        },
    )?;

    let existing = kernel.index().root().map(ToString::to_string);
    let (root, job) = match (existing, start) {
        (Some(_), Some(_)) => {
            return Err(Error(
                "这个 Kernel 目录里已经有一次运行：续做用 resume，要重新做就换一个 Kernel 目录".into(),
            ));
        }
        (None, None) => return Err(Error("这个 Kernel 目录里没有运行可续做：先 run".into())),
        (Some(root), None) => {
            let job = recorded(&kernel, &root)?;
            (root, job)
        }
        (None, Some(start)) => {
            let content = Content::from_inputs(kernel.store().clone(), &[], BUDGETS)?;
            let home = home_bundle(&content, settings.limits(), assembly.tools.as_ref())?;
            let job = match start {
                Start::W0 { repo, task } => {
                    let definition = definition(&content, library::source("w0.py"), &home)?;
                    let snapshot = directory::import(&content, &repo, settings.limits())?;
                    Job {
                        definition,
                        input: json!({"task": task, "home": home, "snapshot": snapshot}),
                        dependencies: sorted(vec![home, snapshot]),
                        directory: "snapshot".into(),
                    }
                }
                Start::Source { source, input, repo } => {
                    let definition = definition(&content, &source, &home)?;
                    let interface = croon_interface(&content, &definition)?;
                    let mut input = input;
                    let mut dependencies = vec![home.clone()];
                    let takes = interface["signature"]["in"]["properties"].get("snapshot").is_some();
                    if let Some(repo) = repo.filter(|_| takes) {
                        let snapshot = directory::import(&content, &repo, settings.limits())?;
                        input["snapshot"] = json!(snapshot);
                        dependencies.push(snapshot);
                    } else if takes {
                        return Err(Error("这份源码要 snapshot：用 --snapshot 给一个目录".into()));
                    }
                    let gives = interface["signature"]["out"]["properties"].get("snapshot").is_some();
                    Job {
                        definition,
                        input,
                        dependencies: sorted(dependencies),
                        directory: if gives { "snapshot".into() } else { String::new() },
                    }
                }
            };
            let root_definition = schema("host.root");
            let raw = content.put_json(&root_definition)?;
            let bundle = content.pack(&raw, &[])?;
            let requires = &root_definition["interface"]["requires"];
            let mut sources = serde_json::Map::new();
            for port in ["pack", "construct", "instantiate"] {
                sources.insert(port.into(), json!({"kernel": port, "spec": requires[port]}));
            }
            sources.insert(
                "engine".into(),
                json!({"host": model.id(), "spec": requires["engine"]}),
            );
            sources.insert(
                "exec".into(),
                json!({"host": exec.id(), "spec": requires["exec"]}),
            );
            sources.insert(
                "infer".into(),
                json!({"host": inferrer.id(), "spec": requires["infer"]}),
            );
            let birth_input = json!({
                "value": {"definition": job.definition, "input": job.input, "dependencies": job.dependencies, "directory": job.directory},
                "dependencies": sorted([vec![job.definition.clone()], job.dependencies.clone()].concat()),
            });
            let root = kernel.create_root(
                &bundle,
                &birth_input,
                &json!({"back": "port/done"}),
                &json!({"sources": sources}),
            )?;
            (root, job)
        }
    };
    let definition = job.definition.clone();
    for (port, provider) in [("engine", model), ("exec", exec), ("infer", inferrer)] {
        let handle = kernel.index().journals[root.as_str()].birth()["wire"][port]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error(format!("root entry 没有接 {port}")))?;
        kernel.bind_host(&handle, provider)?;
    }

    // 再跑一次就是恢复：重开被宿主停下的 Instance，从 Journal 接着走。
    let stopped: Vec<String> = kernel.stopped().keys().cloned().collect();
    for instance in stopped {
        kernel.reopen(&instance)?;
    }

    let conn = kernel.connect(&root)?;
    let mut client = Client {
        kernel: &mut kernel,
        conn,
    };
    client.send(
        "construct-w0",
        json!({"port": "construct"}),
        json!({"definition": definition}),
        vec![definition.clone()],
        json!({}),
    )?;
    // 接线按被运行的 Definition 自己声明的 Required port 取，root entry 手里没有的当场报错：
    // 沟通由 Definition 的 Interface 决定，宿主不替它多给，也不少给。
    let interface = croon_interface(&Content::from_inputs(client.kernel.store().clone(), &[definition.clone()], BUDGETS)?, &definition)?;
    let mut attach = serde_json::Map::new();
    let mut own = serde_json::Map::new();
    for port in interface["requires"].as_object().into_iter().flatten().map(|(k, _)| k) {
        if port == "back" {
            own.insert("back".into(), json!("port/done"));
        } else if ["engine", "exec", "infer", "instantiate", "pack", "construct"].contains(&port.as_str()) {
            attach.insert(port.clone(), json!({"port": port}));
        } else {
            return Err(Error(format!("要运行的 Definition 要 {port}，宿主给不了")));
        }
    }
    let spawned = client.send(
        "spawn",
        json!({"port": "instantiate"}),
        json!({
            "definition": definition,
            "input": {"value": job.input, "dependencies": job.dependencies},
            "seats": {},
            "self": own
        }),
        sorted([vec![definition.clone()], job.dependencies.clone()].concat()),
        Json::Object(attach),
    )?;
    let notify = spawned["attach"]["notify"]["handle"]
        .as_str()
        .ok_or_else(|| Error("实例化没有交回 notify".into()))?
        .to_owned();
    let waited = client.send(
        "wait",
        json!({"handle": notify}),
        json!({}),
        vec![],
        json!({"to": {"port": "back"}}),
    )?;
    let done = waited["derived"]["to"]["handle"]
        .as_str()
        .ok_or_else(|| Error("登记没有交回 to 的 Handle".into()))?
        .to_owned();
    let taken =
        client.submit(&json!({"key": "take", "receive": {"ports": ["done"], "via": [done]}}))?;
    let outcome = &taken["arrival"]["delivery"]["value"];
    if outcome["ok"] != true {
        return Err(Error(format!("运行失败：{}", outcome["error"])));
    }
    let value = &outcome["value"];
    let finished = Finished {
        summary: match value.get("summary") {
            Some(summary) => text(value, "summary").map_err(|_| Error(format!("summary 要是字符串：{summary}")))?,
            None => value.to_string(),
        },
        snapshot: if job.directory.is_empty() {
            String::new()
        } else {
            text(value, &job.directory)?
        },
    };
    if finished.snapshot.is_empty() {
        return Ok(finished);
    }

    if out.exists() {
        // 恢复时上次可能已经建好了结果目录：空的可以接着用，有东西的不覆盖。
        if std::fs::read_dir(out)?.next().is_some() {
            return Err(Error(format!("结果目录 {} 不是空的", out.display())));
        }
    } else {
        std::fs::create_dir(out)?;
    }
    let view = Content::from_inputs(
        kernel.store().clone(),
        &[finished.snapshot.clone()],
        BUDGETS,
    )?;
    directory::materialize(&view, &finished.snapshot, out, settings.limits())?;
    Ok(finished)
}

/// 取回 root entry 诞生时记下的这次运行。
fn recorded(kernel: &Kernel, root: &str) -> Result<Job> {
    let birth = kernel.index().journals[root].birth();
    let id = birth["input"]["value"]
        .as_str()
        .ok_or_else(|| Error("root entry 的诞生里没有输入".into()))?;
    let value = kernel
        .store()
        .get_json(&croon_eval::value::ValueId::parse(id).ok_or_else(|| Error(format!("root 的输入 ID 不对：{id}")))?)
        .map_err(|e| Error(e.to_string()))?
        .ok_or_else(|| Error(format!("内容库里没有 root 的输入 {id}")))?;
    Ok(Job {
        definition: text(&value, "definition")?,
        input: value["input"].clone(),
        dependencies: value["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        directory: value["directory"].as_str().unwrap_or_default().to_owned(),
    })
}

/// 一份 Definition Bundle 的 Interface。
fn croon_interface(content: &Content, definition: &str) -> Result<Json> {
    let bundle = content
        .bundle(definition)?
        .ok_or_else(|| Error(format!("读不到 Definition {definition}")))?;
    let body = content
        .json(&bundle.content)?
        .ok_or_else(|| Error(format!("读不到 Definition 的内容 {}", bundle.content)))?;
    Ok(body["interface"].clone())
}

/// 记录里的 `at`：UTC 的 RFC 3339。
fn now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}
