//! 宿主层的崩溃恢复，exec 在途：起 croon-app 子进程，在文件执行还在跑时 SIGKILL 它，
//! 用同一个 Kernel 目录与 state 目录重跑。旧 attempt 被退役（封印与 tombstone），新 attempt 重做，
//! 结果与不崩时相同，没有存活的容器。进程内的故障点测不了这一种：执行表在内存里，真崩溃时会丢。
//! 参考：docs/Host/README.md（客户端）；docs/Directory/README.md（隔离与恢复）。

mod support;

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::fs;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};
use support::{Temp, llm, prefix};

const COMMAND: &str = "sleep 10; echo slow > s.txt";

/// W0 调一次慢命令，然后结束。
struct Slow {
    identity: String,
}

impl Slow {
    fn answer(&self, request: &Json) -> Json {
        if request["messages"].as_array().unwrap().len() == 1 {
            llm(
                "",
                json!([{"id": "c1", "name": "run", "input": {"command": COMMAND}}]),
            )
        } else {
            llm("写好了 s.txt", json!([]))
        }
    }
}

impl Port for Slow {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, call: &Call) -> Result<Box<dyn Execution>, HostError> {
        Ok(Box::new(Ready(HostDone {
            outcome: self.answer(call.value),
        })))
    }
    fn retire(&self, _attempt: &str) -> Result<(), HostError> {
        Ok(())
    }
}

struct Ready(HostDone);

impl Execution for Ready {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        Ok(Some(self.0.clone()))
    }
}

fn docker(args: &[&str]) -> String {
    let out = Command::new("/usr/bin/docker").args(args).output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn croon_app(
    config: &std::path::Path,
    repo: &std::path::Path,
    out: &std::path::Path,
    task: &str,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_croon-app"));
    command
        .arg("run")
        .arg("--config")
        .arg(config)
        .arg("--repo")
        .arg(repo)
        .arg("--out")
        .arg(out)
        .arg(task);
    command
}

#[test]
#[ignore = "requires local Docker with croon-work:1 and python:3.12-slim"]
fn killing_the_host_during_an_exec_retires_the_attempt_and_redoes_it() {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "x\n").unwrap();
    let task = "慢慢写 s.txt";

    // 基线：进程内用慢模型边跑边录。
    let first = temp.system("record");
    let cassette = temp.root.join("cassette.json");
    let settings =
        host::Settings::load(&temp.config(&first, &temp.root.join("unused.json"))).unwrap();
    let recorder: Rc<dyn Port> = Rc::new(
        model::Recorder::new(
            Rc::new(Slow {
                identity: format!("{first}/model"),
            }),
            &cassette,
        )
        .unwrap(),
    );
    let assembly = host::Assembly {
        model: Some(recorder),
        fault: None,
    };
    let baseline = host::run(
        &settings,
        assembly,
        &repo,
        task,
        &temp.root.join("baseline"),
    )
    .unwrap();

    // 子进程回放这盘录像；任务容器跑起来、命令开始之后 SIGKILL。
    let system = temp.system("kill");
    let config = temp.config(&system, &cassette);
    let mut child = croon_app(&config, &repo, &temp.root.join("killed"), task)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let label = format!("label=croon.provider={}", prefix(&system));
    let deadline = Instant::now() + Duration::from_secs(60);
    while docker(&["ps", "-q", "--filter", &label, "--filter", "status=running"])
        .trim()
        .is_empty()
    {
        assert!(Instant::now() < deadline, "任务容器没有跑起来");
        assert!(
            child.try_wait().unwrap().is_none(),
            "croon-app 在 exec 之前就退出了"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(2));
    assert!(child.try_wait().unwrap().is_none(), "exec 应当还在途");
    child.kill().unwrap();
    child.wait().unwrap();

    // 同一个 Kernel 目录与 state 目录重跑：旧 attempt 退役，新 attempt 重做。
    let out = temp.root.join("out");
    let rerun = croon_app(&config, &repo, &out, task).output().unwrap();
    let stdout = String::from_utf8_lossy(&rerun.stdout).into_owned();
    assert!(
        rerun.status.success(),
        "重跑失败：{stdout}{}",
        String::from_utf8_lossy(&rerun.stderr)
    );
    assert!(
        stdout.contains(&baseline.snapshot),
        "结果快照与不崩时相同：{stdout}"
    );
    assert_eq!(fs::read_to_string(out.join("s.txt")).unwrap(), "slow\n");

    let state = temp.root.join(format!("state-{system}"));
    let sealed = fs::read_dir(&state)
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.path().join("retired").exists());
    assert!(sealed, "崩溃前的 attempt 应当留下封印");
    let tombstones = docker(&[
        "ps",
        "-aq",
        "--filter",
        &label,
        "--filter",
        "label=croon.retired",
    ]);
    assert!(
        !tombstones.trim().is_empty(),
        "崩溃前的 attempt 应当留下 tombstone"
    );
    let running = docker(&["ps", "-q", "--filter", &label, "--filter", "status=running"]);
    assert!(running.trim().is_empty(), "不应有存活的容器：{running}");
}
