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

mod absent;
mod blocks;
pub mod infer;
pub mod library;
mod progress;

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

/// 场景预期（`library.scenario.*`），只给测试与核对脚本用。
pub fn scenario(name: &str) -> Json {
    blocks::scenario(name)
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

const NO_MODEL_FOR_RUN: &str = "这条命令要调用模型，配置里却没有 model：在配置文件里写 {\"model\": {\"provider\": \"/绝对路径/agent-provider.json\"}}，见 README「上手」";

/// 模型从哪里来：真 Provider、边调用边录制，或只回放录像。
#[derive(Clone)]
pub enum ModelSource {
    Provider(PathBuf),
    /// `record` 为空时录在会话目录里。
    Record {
        provider: PathBuf,
        record: Option<PathBuf>,
    },
    Replay(PathBuf),
    /// 配置里没有 `model`：只有 `exec` 可以这样运行。
    None,
}

/// 校验过的宿主配置。一次运行的身份（system、kernel、state）可以没有，由会话补上。
#[derive(Clone)]
pub struct Settings {
    pub system: Option<String>,
    pub kernel: Option<PathBuf>,
    pub model: ModelSource,
    pub directory: DirectorySettings,
}

/// 文件执行的装配：没写的字段用默认值，见 docs/Host/README.md「配置与会话」。`state` 为空时在会话目录里。
#[derive(Clone)]
pub struct DirectorySettings {
    pub state: Option<PathBuf>,
    pub docker: PathBuf,
    pub python: PathBuf,
    pub image: String,
    pub collector_image: String,
    pub platform: String,
    pub cpus: String,
    pub memory_bytes: u64,
    pub work_bytes: usize,
    pub temporary_bytes: usize,
    pub pids: u32,
    pub output_bytes: usize,
    pub nodes: usize,
    pub archive_bytes: usize,
    pub cleanup_timeout: Duration,
}

impl DirectorySettings {
    /// 交给 Directory 的装配：恢复身份与 state 目录来自会话。
    fn config(&self, session: &Session) -> directory::Config {
        directory::Config {
            identity: format!("{}/directory", session.name),
            state: session.state.clone(),
            docker: self.docker.clone(),
            python: self.python.clone(),
            image: self.image.clone(),
            collector_image: self.collector_image.clone(),
            platform: self.platform.clone(),
            cpus: self.cpus.clone(),
            memory_bytes: self.memory_bytes,
            work_bytes: self.work_bytes,
            temporary_bytes: self.temporary_bytes,
            pids: self.pids,
            output_bytes: self.output_bytes,
            nodes: self.nodes,
            archive_bytes: self.archive_bytes,
            cleanup_timeout: self.cleanup_timeout,
        }
    }
}

/// 一次运行落在磁盘上的位置与身份：会话给出，或配置里写死。
#[derive(Clone)]
pub struct Session {
    pub name: String,
    pub kernel: PathBuf,
    pub state: PathBuf,
    pub cassette: PathBuf,
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

/// 写了就按它解析（写错是错），没写就用默认值。
fn or<T>(v: &Json, key: &str, default: T, parse: impl Fn(&Json, &str) -> Result<T>) -> Result<T> {
    if v.get(key).is_some() {
        parse(v, key)
    } else {
        Ok(default)
    }
}

/// PATH 里一个可执行文件的绝对路径。
fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(|dir| Path::new(dir).join(name))
        .find(|p| p.is_file())
}

/// 用户的状态目录：`$XDG_STATE_HOME/croon`，没有就 `~/.local/state/croon`。
pub fn state_home() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_STATE_HOME").filter(|d| Path::new(d).is_absolute()) {
        return Ok(Path::new(&dir).join("croon"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error("找不到家目录：设 HOME 或 XDG_STATE_HOME".into()))?;
    Ok(Path::new(&home).join(".local/state/croon"))
}

/// 默认的配置文件：`$XDG_CONFIG_HOME/croon/config.json`，没有就 `~/.config/croon/config.json`；文件不存在就是没有。
pub fn default_config() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|d| Path::new(d).is_absolute()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(&std::env::var_os("HOME")?).join(".config"),
    };
    let path = base.join("croon/config.json");
    path.is_file().then_some(path)
}

/// `sessions` 目录下有哪些会话，按最近修改在前。目录不存在就是没有，不会因为看一眼而建出来。
pub fn list_sessions(sessions: &Path) -> Vec<String> {
    let mut found: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(sessions)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("kernel/journal.db").is_file())
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.file_name().to_string_lossy().into_owned()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, name)| name).collect()
}

/// 会话名要出现在容器名与路径里：只允许字母、数字、`-`、`_`、`.`，不以 `.` 开头。
pub fn valid_session_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        let value: Json = serde_json::from_slice(&std::fs::read(path)?)?;
        Self::from_json(&value)
    }

    pub fn from_json(value: &Json) -> Result<Self> {
        croon_eval::schema::validate(&schema("host.config"), value)
            .map_err(|e| Error(format!("host.config: {e:?}")))?;
        let m = &value["model"];
        let model = if value.get("model").is_none() {
            ModelSource::None
        } else if m.get("replay").is_some() {
            ModelSource::Replay(absolute(m, "replay")?)
        } else if let Some(record) = m.get("record") {
            ModelSource::Record {
                provider: absolute(m, "provider")?,
                // true：录在会话目录里；写了路径：录到那里。
                record: if record == &json!(true) { None } else { Some(absolute(m, "record")?) },
            }
        } else {
            ModelSource::Provider(absolute(m, "provider")?)
        };
        let none = json!({});
        let d = value.get("directory").unwrap_or(&none);
        let docker = match d.get("docker") {
            Some(_) => absolute(d, "docker")?,
            None => on_path("docker").ok_or_else(|| {
                Error("PATH 里没有 docker：装好它，或在配置的 directory.docker 里写路径".into())
            })?,
        };
        let python = match d.get("python") {
            Some(_) => absolute(d, "python")?,
            None => on_path("python3").ok_or_else(|| {
                Error("PATH 里没有 python3：装好它，或在配置的 directory.python 里写路径".into())
            })?,
        };
        let directory = DirectorySettings {
            state: d.get("state").map(|_| absolute(d, "state")).transpose()?,
            docker,
            python,
            image: or(d, "image", "croon-work:2".into(), text)?,
            collector_image: or(d, "collector_image", "python:3.12-slim".into(), text)?,
            platform: or(d, "platform", "linux/amd64".into(), text)?,
            cpus: or(d, "cpus", "1".into(), text)?,
            memory_bytes: or(d, "memory_bytes", 512 << 20, number)?,
            work_bytes: or(d, "work_bytes", 64 << 20, number)?,
            temporary_bytes: or(d, "temporary_bytes", 64 << 20, number)?,
            pids: or(d, "pids", 128, number)?,
            output_bytes: or(d, "output_bytes", 64 << 10, number)?,
            nodes: or(d, "nodes", 20_000, number)?,
            archive_bytes: or(d, "archive_bytes", 128 << 20, number)?,
            cleanup_timeout: Duration::from_secs(or(d, "cleanup_timeout_seconds", 60, number)?),
        };
        Ok(Self {
            system: value.get("system").map(|_| text(value, "system")).transpose()?,
            kernel: value.get("kernel").map(|_| absolute(value, "kernel")).transpose()?,
            model,
            directory,
        })
    }

    /// 这次运行的身份与目录。配置里写死了的按配置；其余由会话名派生，落在 `sessions` 下。
    /// `name` 为空时用配置里的 `system`；两者都没有就报错。
    pub fn session(&self, name: Option<&str>, sessions: &Path) -> Result<Session> {
        let name = match (name, &self.system) {
            (Some(n), _) => n.to_owned(),
            (None, Some(s)) => s.clone(),
            (None, None) => {
                return Err(Error(
                    "没有会话名：run 会自己取一个，resume 要 --session 名字".into(),
                ));
            }
        };
        if !valid_session_name(&name) {
            return Err(Error(format!(
                "会话名 {name:?} 不合法：只允许字母、数字、-、_、.，且不以 . 开头"
            )));
        }
        let dir = sessions.join(&name);
        Ok(Session {
            kernel: self.kernel.clone().unwrap_or_else(|| dir.join("kernel")),
            state: self
                .directory
                .state
                .clone()
                .unwrap_or_else(|| dir.join("state")),
            cassette: match &self.model {
                ModelSource::Record {
                    record: Some(r), ..
                } => r.clone(),
                _ => dir.join("cassette.json"),
            },
            name,
        })
    }

    /// 配置里的模型来源；写了 `record` 才录，`resuming` 时接着已有的录像追加。录像路径不写时放在会话目录里。
    fn model_port(&self, session: &Session, resuming: bool, needs_model: bool) -> Result<Rc<dyn Port>> {
        let identity = format!("{}/model", session.name);
        Ok(match &self.model {
            ModelSource::Provider(provider) => Rc::new(model::OpenAiCompatible::from_config_file(
                provider, identity,
            )?),
            ModelSource::Record { provider, .. } => {
                let inner = Rc::new(model::OpenAiCompatible::from_config_file(
                    provider, identity,
                )?);
                if resuming && session.cassette.exists() {
                    Rc::new(model::Recorder::resume(inner, session.cassette.clone())?)
                } else {
                    Rc::new(model::Recorder::new(inner, session.cassette.clone())?)
                }
            }
            ModelSource::Replay(cassette) => Rc::new(model::Replayer::open(cassette, identity)?),
            ModelSource::None if needs_model => {
                return Err(Error(NO_MODEL_FOR_RUN.into()));
            }
            ModelSource::None => Rc::new(absent::NoModel::new(identity)),
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
    /// 一次运行落在磁盘上的位置与身份；不给就用配置里写死的 `system`、`kernel`、`state`，它们都要有。
    pub session: Option<Session>,
    /// 不在标准错误上打印进度。
    pub quiet: bool,
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
    progress: progress::Progress,
}

impl Client<'_> {
    fn submit(&mut self, atom: &Json) -> Result<Json> {
        loop {
            if let Some(result) = self.kernel.submit(&self.conn, atom)? {
                return Ok(result);
            }
            let progress = self.kernel.run()?;
            let store = self.kernel.store().clone();
            self.progress.report(
                self.kernel
                    .index()
                    .journals
                    .iter()
                    .map(|(id, journal)| (id.as_str(), journal.records.as_slice())),
                &|id| store.get_json(&croon_eval::value::ValueId::parse(id)?).ok().flatten(),
            );
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
    // run 要模型；exec 不要；resume 续的是哪一种看不出来，也不拦：真要问模型时那一次调用以失败结束并说清楚。
    let needs_model = matches!(start, Some(Start::W0 { .. }));
    // 一次运行的身份：运行参数给了就用，没有就看配置里写死的 system；都没有是调用方的错。
    let session = match assembly.session.clone() {
        Some(session) => session,
        None => settings.session(None, &state_home()?.join("sessions"))?,
    };
    // 先看这个会话是什么状态，再决定做什么：判断没有任何副作用，所以拼错名字不会留下东西，
    // 重复的名字也不会先去碰录像。有 journal.db 就是开始过。
    let started = session.kernel.join("journal.db").is_file();
    match (started, resuming) {
        (true, false) => {
            return Err(Error(format!(
                "会话 {0} 里已经有一次运行：续做用 resume --session {0}，要做新的就换一个会话名（不给 --session 会自己取一个）",
                session.name
            )));
        }
        (false, true) => {
            let others = list_sessions(session.kernel.parent().and_then(Path::parent).unwrap_or(Path::new("")));
            let hint = if others.is_empty() {
                "这台机器上还没有别的会话".to_owned()
            } else {
                format!("已有的会话：{}", others.iter().take(8).cloned().collect::<Vec<_>>().join("、"))
            };
            return Err(Error(format!(
                "找不到会话 {}：它没有开始过。要开始一次运行用 run；{hint}",
                session.name
            )));
        }
        _ => {}
    }
    // 配置缺东西是调用方的错：在建任何目录之前说，不留下空会话。
    if needs_model && assembly.model.is_none() && matches!(settings.model, ModelSource::None) {
        return Err(Error(NO_MODEL_FOR_RUN.into()));
    }
    std::fs::create_dir_all(&session.kernel)?;
    std::fs::create_dir_all(&session.state)?;
    let model = match assembly.model {
        Some(model) => {
            let identity = format!("{}/model", session.name);
            if model.id() != identity {
                return Err(Error(format!(
                    "运行参数里的模型恢复身份是 {}，要是 {identity}",
                    model.id()
                )));
            }
            model
        }
        None => settings.model_port(&session, resuming, needs_model)?,
    };
    let exec: Rc<dyn Port> = Rc::new(directory::Docker::new(settings.directory.config(&session))?);
    let inferrer: Rc<dyn Port> = Rc::new(infer::Infer::new(format!("{}/infer", session.name)));
    let mut kernel = Kernel::open(
        &session.kernel,
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
        // 有 journal.db 却没有 root：创建 root 之前就中断了。没有东西丢失，重新 run 即可。
        (None, None) => {
            return Err(Error(format!(
                "会话 {} 在创建之前就中断了，没有留下运行：用 run --session {} 重新开始",
                session.name, session.name
            )));
        }
        (Some(_), Some(_)) => unreachable!("上面已经拒绝了重复的会话"),
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
        progress: progress::Progress::new(assembly.quiet, resuming),
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
