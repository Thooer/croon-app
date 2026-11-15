//! 宿主：根在系统外的装配。按 host.config 打开 Kernel，装上 Lang0、模型与文件执行，
//! 创建 root entry，作为它的客户端交任务、取结果，再把结果快照物化到新目录。
//!
//! 调用方要知道的：
//!
//! - 同一个系统名必须始终配同一个 Kernel 目录与 state 目录；恢复身份由系统名派生，写进 Journal。
//! - 同一个 Kernel 目录再跑一次就是恢复：已有 root entry 时不再创建，按原键重报同样的几步；
//!   任务或仓库变了是重放分歧。
//! - 结果只写进一个新的空目录，不碰用户的仓库。
//! - [`Assembly`] 里给的模型 Port，恢复身份必须是 `<系统名>/model`；故障点只用于测试。
//! - 有 Instance 被宿主停下时，[`run`] 带着停止原因返回错误，不会一直等；同一个 Kernel 目录
//!   再跑一次时先重开它们。业务失败是 Journal 里的事实，重跑只重放出同样的失败。
//!
//! 参考：docs/Host/README.md（装配、配置、root entry、客户端）。

mod blocks;
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

    fn model_port(&self) -> Result<Rc<dyn Port>> {
        let identity = format!("{}/model", self.system);
        Ok(match &self.model {
            ModelSource::Provider(provider) => Rc::new(model::OpenAiCompatible::from_config_file(
                provider, identity,
            )?),
            ModelSource::Record { provider, record } => Rc::new(model::Recorder::new(
                Rc::new(model::OpenAiCompatible::from_config_file(
                    provider, identity,
                )?),
                record.clone(),
            )?),
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

/// 把 Library 装进临时目录，再导入成模块目录 Bundle：顶层是 library/。
fn library_bundle(content: &Content, limits: directory::Limits) -> Result<String> {
    let root = std::env::temp_dir().join(format!(
        "croon-host-library-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    library::install(root.join("library"));
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

/// 按装配的五步跑完一个任务，结果物化到新的空目录 `out`。
pub fn run(
    settings: &Settings,
    assembly: Assembly,
    repo: &Path,
    task: &str,
    out: &Path,
) -> Result<Finished> {
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
        None => settings.model_port()?,
    };
    let exec: Rc<dyn Port> = Rc::new(directory::Docker::new(settings.directory.clone())?);
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

    let content = Content::from_inputs(kernel.store().clone(), &[], BUDGETS)?;
    let modules = library_bundle(&content, settings.limits())?;
    let s = definition(&content, library::source("s.py"), &modules)?;
    let w0 = definition(&content, library::source("w0.py"), &modules)?;
    let snapshot = directory::import(&content, repo, settings.limits())?;

    let root = match kernel.index().root().map(ToString::to_string) {
        Some(root) => root,
        None => {
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
            let input = json!({"value": null, "dependencies": sorted(vec![w0.clone(), s.clone(), snapshot.clone()])});
            kernel.create_root(
                &bundle,
                &input,
                &json!({"back": "port/done"}),
                &json!({"sources": sources}),
            )?
        }
    };
    for (port, provider) in [("engine", model), ("exec", exec)] {
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
        "construct-s",
        json!({"port": "construct"}),
        json!({"definition": s}),
        vec![s.clone()],
        json!({}),
    )?;
    client.send(
        "construct-w0",
        json!({"port": "construct"}),
        json!({"definition": w0}),
        vec![w0.clone()],
        json!({}),
    )?;
    let spawned = client.send(
        "spawn",
        json!({"port": "instantiate"}),
        json!({
            "definition": w0,
            "input": {"value": {"task": task, "worker": s, "snapshot": snapshot}, "dependencies": sorted(vec![s.clone(), snapshot.clone()])},
            "seats": {},
            "self": {"back": "port/done"}
        }),
        vec![w0.clone(), s.clone(), snapshot.clone()],
        json!({"engine": {"port": "engine"}, "exec": {"port": "exec"}, "instantiate": {"port": "instantiate"}}),
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
        return Err(Error(format!("W0 失败：{}", outcome["error"])));
    }
    let finished = Finished {
        summary: text(&outcome["value"], "summary")?,
        snapshot: text(&outcome["value"], "snapshot")?,
    };

    std::fs::create_dir(out)?;
    let view = Content::from_inputs(
        kernel.store().clone(),
        &[finished.snapshot.clone()],
        BUDGETS,
    )?;
    directory::materialize(&view, &finished.snapshot, out, settings.limits())?;
    Ok(finished)
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
