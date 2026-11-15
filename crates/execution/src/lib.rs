//! Docker CLI 文件执行，attempt 的持久封印与命名环境共同兑现 retire。
//! 参考：docs/Directory/README.md（文件执行、隔离与恢复）。

mod collect;
mod fd;
mod filesystem;
mod launch;
pub mod model;
mod process;
mod state;
#[cfg(test)]
mod tests;
pub use filesystem::{Limits, import, materialize, read};

mod blocks;
pub fn schema(name: &str) -> serde_json::Value {
    blocks::json(name)
}
use process::{bounded, drain};

use croon_port::host::{Call, Channel, Execution, HostDone, HostError, Port};
use croon_port::store::Content;
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

fn host(e: impl std::fmt::Display) -> HostError {
    HostError(e.to_string())
}
#[derive(Clone)]
pub struct Config {
    pub identity: String,
    pub state: PathBuf,
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
#[derive(Clone)]
pub struct Docker {
    config: Rc<Config>,
    prefix: String,
    runs: Rc<RefCell<BTreeMap<String, Rc<RefCell<Running>>>>>,
}
struct Lock(File);
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
struct Request {
    snapshot: String,
    mounts: Vec<(String, String)>,
    argv: Vec<String>,
    stdin: String,
    timeout: Duration,
}
impl Request {
    fn parse(value: &Json) -> Result<Self, HostError> {
        croon_eval::schema::validate(&crate::schema("directory.exec")["in"], value)
            .map_err(|e| host(format!("{e:?}")))?;
        let argv: Vec<String> = value["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect();
        let timeout = value["timeout"]
            .as_u64()
            .filter(|v| (1..=600).contains(v))
            .ok_or_else(|| host("timeout must be 1..600 seconds"))?;
        if argv.is_empty() || argv.iter().any(|s| s.contains('\0')) {
            return Err(host("argv must be nonempty and contain no NUL"));
        }
        let mounts: Vec<_> = value["mounts"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        if mounts.iter().any(|(name, _)| !croon_tree::valid_name(name)) {
            return Err(host("invalid mount name"));
        }
        Ok(Self {
            snapshot: value["snapshot"].as_str().unwrap().into(),
            mounts,
            argv,
            stdin: value["stdin"].as_str().unwrap().into(),
            timeout: Duration::from_secs(timeout),
        })
    }
}
struct Running {
    provider: Docker,
    attempt: String,
    container: String,
    content: Rc<Content>,
    child: Child,
    stdout: Option<std::thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>>,
    stderr: Option<std::thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>>,
    deadline: Instant,
    watchdog_cancel: std::sync::mpsc::Sender<()>,
    watchdog_status: std::sync::mpsc::Receiver<Result<Vec<u8>, HostError>>,
    exit: Option<i32>,
    timed_out: bool,
    output: Option<(Vec<u8>, Vec<u8>, bool)>,
    removed: bool,
    done: Option<HostDone>,
}
impl Port for Docker {
    fn id(&self) -> &str {
        &self.config.identity
    }
    fn channel(&self) -> Channel {
        Channel::Isolated
    }
    fn start(&self, call: &Call) -> Result<Box<dyn Execution>, HostError> {
        let request = Request::parse(call.value)?;
        let _lock = self.lock(call.attempt)?;
        let path = self.path(call.attempt);
        if path.join("retired").exists() {
            return Err(host("attempt is permanently retired"));
        }
        if let Some(run) = self.runs.borrow().get(call.attempt) {
            return Ok(Box::new(Shared(run.clone())));
        }
        if path.exists() {
            return Err(host(
                "attempt already prepared; retire before recovery with a new attempt",
            ));
        }
        fs::create_dir(&path).map_err(host)?;
        File::open(&self.config.state)
            .map_err(host)?
            .sync_all()
            .map_err(host)?;
        // The durable directory precedes every Docker request, including uncertain create responses.
        let launched = self.launch(call, request);
        let running = match launched {
            Ok(running) => Rc::new(RefCell::new(running)),
            Err(error) => {
                let cleanup = self.retire_locked(call.attempt);
                return Err(host(format!(
                    "{error}; failed-start retirement: {cleanup:?}"
                )));
            }
        };
        self.runs
            .borrow_mut()
            .insert(call.attempt.into(), running.clone());
        Ok(Box::new(Shared(running)))
    }
    fn retire(&self, attempt: &str) -> Result<(), HostError> {
        let _lock = self.lock(attempt)?;
        self.retire_locked(attempt)
    }
}
impl Docker {
    fn retire_locked(&self, attempt: &str) -> Result<(), HostError> {
        let path = self.path(attempt);
        fs::create_dir_all(&path).map_err(host)?;
        self.seal(&path)?;
        let name = self.name(attempt);
        let collector = format!("{name}-collector");
        self.retire_container(&collector)?;
        self.retire_container(&name)?;
        if let Some(run) = self.runs.borrow_mut().remove(attempt) {
            let mut run = run.borrow_mut();
            let _ = run.watchdog_cancel.send(());
            let _ = run.child.kill();
            let _ = run.child.wait();
        }
        // Keep the tombstone forever: a late start with this attempt cannot resurrect writers.
        for name in ["input", "artifacts"] {
            let p = path.join(name);
            if p.exists() {
                fs::remove_dir_all(p).map_err(host)?;
            }
        }
        for name in ["stdin", "archive.tmp", "work.tar"] {
            let file = path.join(name);
            match fs::remove_file(file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(host(error)),
            }
        }
        for entry in fs::read_dir(&path).map_err(host)? {
            let entry = entry.map_err(host)?;
            if entry.file_name().to_string_lossy().starts_with("mount-") {
                fs::remove_dir_all(entry.path()).map_err(host)?;
            }
        }
        Ok(())
    }
}
struct Shared(Rc<RefCell<Running>>);
impl Execution for Shared {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        self.0.borrow_mut().collect()
    }
}
