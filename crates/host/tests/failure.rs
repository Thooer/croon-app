//! 模型调用失败的两类处置，见 docs/Model/README.md「失败」与 docs/Host/README.md「客户端」：
//! 宿主故障让 Kernel 停下 W0，客户端带着原因退出、不盲等；修好之后同一个 Kernel 目录重跑，重开 W0 接着走。
//! use 失败交还 body，W0 以失败结束，客户端照常收到结束状态并报出。

mod support;

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::fs;
use std::rc::Rc;
use std::time::{Duration, Instant};
use support::{Scripted, Temp};

/// 一开始就报宿主故障的模型，如模型名配错了。
struct Refusing {
    identity: String,
}

impl Port for Refusing {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, _call: &Call) -> Result<Box<dyn Execution>, HostError> {
        Err(HostError(
            "model HTTP request failed (HTTP 404): model_not_found".into(),
        ))
    }
    fn retire(&self, _attempt: &str) -> Result<(), HostError> {
        Ok(())
    }
}

/// 每次都给出 use 失败的模型，如请求被 Provider 以 400 拒绝。
struct Rejecting {
    identity: String,
}

impl Port for Rejecting {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, _call: &Call) -> Result<Box<dyn Execution>, HostError> {
        Ok(Box::new(Ready(HostDone {
            outcome: rejected(),
        })))
    }
    fn retire(&self, _attempt: &str) -> Result<(), HostError> {
        Ok(())
    }
}

fn rejected() -> Json {
    json!({"ok": false, "error": {"kind": "use", "message": "context too long",
        "data": {"status": 400, "type": "invalid_request_error"}}, "dependencies": []})
}

struct Ready(HostDone);

impl Execution for Ready {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        Ok(Some(self.0.clone()))
    }
}

fn repo(temp: &Temp) -> std::path::PathBuf {
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "broken\n").unwrap();
    repo
}

#[test]
#[ignore = "requires local Docker with croon-work:1 and python:3.12-slim"]
fn a_host_fault_ends_the_run_and_a_rerun_reopens_and_finishes() {
    let temp = Temp::new();
    let repo = repo(&temp);
    let system = temp.system("refused");
    let settings =
        host::Settings::load(&temp.config(&system, &temp.root.join("unused.json"))).unwrap();
    let task = "修好 a.py";

    let start = Instant::now();
    let error = host::run(
        &settings,
        host::Assembly {
            model: Some(Rc::new(Refusing {
                identity: format!("{system}/model"),
            })),
            fault: None,
            tools: None,
        },
        &repo,
        task,
        &temp.root.join("out1"),
    )
    .err()
    .expect("宿主故障要报错");
    assert!(start.elapsed() < Duration::from_secs(60), "不能盲等");
    assert!(error.0.contains("model_not_found"), "{error}");
    assert!(!temp.root.join("out1").exists(), "失败时不写结果目录");

    // 修好模型，同一个 Kernel 目录重跑：重开被停下的 W0，从 Journal 接着走。
    let out2 = temp.root.join("out2");
    let done = host::run(
        &settings,
        host::Assembly {
            model: Some(Rc::new(Scripted {
                identity: format!("{system}/model"),
                three_way: false,
            })),
            fault: None,
            tools: None,
        },
        &repo,
        task,
        &out2,
    )
    .unwrap_or_else(|e| panic!("重开后应当跑完：{e}"));
    assert_eq!(done.summary, "已修好 a.py");
    assert_eq!(fs::read_to_string(out2.join("a.py")).unwrap(), "fixed\n");
}

#[test]
#[ignore = "requires local Docker with croon-work:1 and python:3.12-slim"]
fn a_use_failure_ends_w0_with_a_failure_that_the_root_receives() {
    let temp = Temp::new();
    let repo = repo(&temp);
    let system = temp.system("rejected");
    let settings =
        host::Settings::load(&temp.config(&system, &temp.root.join("unused.json"))).unwrap();
    let start = Instant::now();
    let error = host::run(
        &settings,
        host::Assembly {
            model: Some(Rc::new(Rejecting {
                identity: format!("{system}/model"),
            })),
            fault: None,
            tools: None,
        },
        &repo,
        "修好 a.py",
        &temp.root.join("out"),
    )
    .err()
    .expect("W0 失败要报错");
    assert!(start.elapsed() < Duration::from_secs(60), "不能盲等");
    assert!(error.0.contains("W0 失败"), "{error}");
    assert!(error.0.contains("context too long"), "{error}");
}
