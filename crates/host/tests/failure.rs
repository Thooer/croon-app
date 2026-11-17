//! 模型调用失败的两类处置，见 docs/Model/README.md「失败」与 docs/Host/README.md「客户端」：
//! 宿主故障让 Kernel 停下 W0，客户端带着原因退出、不盲等；修好之后同一个 Kernel 目录 resume，重开 W0 接着走。
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
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
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
            session: None,
            quiet: true,
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
    let done = host::resume(
        &settings,
        host::Assembly {
            model: Some(Rc::new(Scripted {
                identity: format!("{system}/model"),
                three_way: false,
            })),
            fault: None,
            tools: None,
            session: None,
            quiet: true,
        },
        &out2,
    )
    .unwrap_or_else(|e| panic!("重开后应当跑完：{e}"));
    assert_eq!(done.summary, "已修好 a.py");
    assert_eq!(fs::read_to_string(out2.join("a.py")).unwrap(), "fixed\n");
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
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
            session: None,
            quiet: true,
        },
        &repo,
        "修好 a.py",
        &temp.root.join("out"),
    )
    .err()
    .expect("W0 失败要报错");
    assert!(start.elapsed() < Duration::from_secs(60), "不能盲等");
    assert!(error.0.contains("运行失败"), "{error}");
    assert!(error.0.contains("context too long"), "{error}");
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn run_refuses_a_directory_that_already_has_a_run_and_resume_needs_one() {
    let temp = Temp::new();
    let repo = repo(&temp);
    let system = temp.system("twice");
    let settings =
        host::Settings::load(&temp.config(&system, &temp.root.join("unused.json"))).unwrap();
    let scripted = |system: &str| host::Assembly {
        model: Some(Rc::new(Scripted {
            identity: format!("{system}/model"),
            three_way: false,
        })),
        fault: None,
        tools: None,
        session: None,
        quiet: true,
    };

    let none = host::resume(&settings, scripted(&system), &temp.root.join("out0"))
        .err()
        .expect("没有运行可续做");
    assert!(none.0.contains("用 run"), "{none}");

    host::run(&settings, scripted(&system), &repo, "修好 a.py", &temp.root.join("out1")).unwrap();
    let again = host::run(&settings, scripted(&system), &repo, "修好 a.py", &temp.root.join("out2"))
        .err()
        .expect("已有运行时 run 要报错");
    assert!(again.0.contains("resume"), "{again}");
    assert!(!temp.root.join("out2").exists());

    // 已经结束的运行再 resume，得到同样的结果；结果目录是空的也行。
    fs::create_dir(temp.root.join("out3")).unwrap();
    let same = host::resume(&settings, scripted(&system), &temp.root.join("out3")).unwrap();
    assert_eq!(same.summary, "已修好 a.py");
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn asking_for_a_session_that_does_not_exist_changes_nothing_and_says_so() {
    let temp = Temp::new();
    let repo = repo(&temp);
    let sessions = temp.root.join("sessions");
    let mut settings =
        host::Settings::load(&temp.config("unused", &temp.root.join("unused.json"))).unwrap();
    settings.system = None;
    settings.kernel = None;
    settings.directory.state = None;
    let session = settings.session(Some("typo"), &sessions).unwrap();

    // resume 一个没有的会话：说「找不到」，并且不建目录，之后它不会出现在会话列表里。
    let error = host::resume(
        &settings,
        host::Assembly {
            model: Some(Rc::new(Scripted {
                identity: "typo/model".into(),
                three_way: false,
            })),
            session: Some(session.clone()),
            quiet: true,
            ..Default::default()
        },
        &temp.root.join("out"),
    )
    .err()
    .expect("没有的会话不能 resume");
    assert!(error.0.contains("找不到会话 typo"), "{error}");
    assert!(error.0.contains("run"), "要告诉用户怎么开始：{error}");
    assert!(!sessions.exists(), "查询失败不能留下任何目录：{}", sessions.display());
    assert!(host::list_sessions(&sessions).is_empty());

    // 同一个名字 run 一次，它才是会话；再 run 同一个名字，说的是「已经有一次运行」，不是别的。
    host::run(
        &settings,
        host::Assembly {
            model: Some(Rc::new(Scripted {
                identity: "typo/model".into(),
                three_way: false,
            })),
            session: Some(session.clone()),
            quiet: true,
            ..Default::default()
        },
        &repo,
        "修好 a.py",
        &temp.root.join("out1"),
    )
    .unwrap();
    assert_eq!(host::list_sessions(&sessions), vec!["typo".to_owned()]);
    let again = host::run(
        &settings,
        host::Assembly {
            model: Some(Rc::new(Scripted {
                identity: "typo/model".into(),
                three_way: false,
            })),
            session: Some(session),
            quiet: true,
            ..Default::default()
        },
        &repo,
        "修好 a.py",
        &temp.root.join("out2"),
    )
    .err()
    .expect("重复的会话要拒绝");
    assert!(again.0.contains("已经有一次运行"), "{again}");
    assert!(again.0.contains("resume --session typo"), "{again}");
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn a_duplicate_session_is_refused_before_the_cassette_is_touched() {
    // record: true 时，旧的做法是先开录像再判断，于是重复的会话报「录像已存在」。
    let temp = Temp::new();
    let repo = repo(&temp);
    let sessions = temp.root.join("sessions");
    let provider = temp.root.join("provider.json");
    fs::write(&provider, r#"{"protocol":"openai_chat_completions","base_url":"http://127.0.0.1:1/v1","api_key":"x","model":"m","max_output_tokens":8,"timeout_seconds":1}"#).unwrap();
    let settings = host::Settings::from_json(&serde_json::json!({
        "model": {"provider": provider, "record": true}
    }))
    .unwrap();
    let session = settings.session(Some("dup"), &sessions).unwrap();
    fs::create_dir_all(&session.kernel).unwrap();
    fs::create_dir_all(session.cassette.parent().unwrap()).unwrap();
    fs::write(&session.cassette, r#"{"entries":[]}"#).unwrap();
    // 这个会话里已经有一次运行：用脚本化模型先跑出来。
    let ran = host::run(
        &host::Settings::load(&temp.config("dup", &temp.root.join("unused.json"))).unwrap(),
        host::Assembly {
            model: Some(Rc::new(Scripted {
                identity: "dup/model".into(),
                three_way: false,
            })),
            session: Some(host::Session {
                name: "dup".into(),
                kernel: session.kernel.clone(),
                state: session.state.clone(),
                cassette: session.cassette.clone(),
            }),
            quiet: true,
            ..Default::default()
        },
        &repo,
        "修好 a.py",
        &temp.root.join("out1"),
    );
    ran.unwrap();
    let before = fs::read(&session.cassette).unwrap();

    let error = host::run(
        &settings,
        host::Assembly {
            session: Some(session.clone()),
            quiet: true,
            ..Default::default()
        },
        &repo,
        "另一件事",
        &temp.root.join("out2"),
    )
    .err()
    .expect("重复的会话要拒绝");
    assert!(error.0.contains("已经有一次运行"), "不是「录像已存在」：{error}");
    assert!(error.0.contains("resume --session dup"), "{error}");
    assert_eq!(fs::read(&session.cassette).unwrap(), before, "录像没被碰");
}
