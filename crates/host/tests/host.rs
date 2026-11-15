//! 宿主装配的端到端：回放的模型、真的 Docker，跑完一次 W0 任务，结果物化到新目录；
//! 同一个 Kernel 目录再跑一次、模型换成空录像，仍得到同样的结果：结果来自 Journal，不再请求模型。
//! 参考：docs/Host/README.md（装配、客户端）；docs/Model/README.md（录制与回放）。

use serde_json::{Value as Json, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SCENARIO: &str = include_str!("../schema/library.scenario.delegate.json");

struct Temp {
    root: PathBuf,
    system: String,
}

impl Temp {
    fn new() -> Self {
        let unique = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(format!("croon-host-test-{unique}"));
        fs::create_dir(&root).unwrap();
        Self {
            root,
            system: format!("host-test-{unique}"),
        }
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // 只清这个系统的容器：前缀由恢复身份派生，与 directory 的命名一致。
        let identity = format!("{}/directory", self.system);
        let prefix = format!(
            "croon-exec-{}",
            croon_eval::value::ValueId::of_bytes(identity.as_bytes()).hex()
        );
        if let Ok(out) = Command::new("/usr/bin/docker")
            .args([
                "ps",
                "-aq",
                "--filter",
                &format!("label=croon.provider={prefix}"),
            ])
            .output()
        {
            for id in String::from_utf8_lossy(&out.stdout).lines() {
                let _ = Command::new("/usr/bin/docker")
                    .args(["rm", "-f", id])
                    .output();
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn llm(text: &str, calls: Json) -> Json {
    let finish = if calls.as_array().is_some_and(|c| c.is_empty()) {
        "stop"
    } else {
        "tool_calls"
    };
    json!({"ok": true, "value": {"text": text, "reasoning": null, "calls": calls, "finish": finish,
        "usage": {"input": 1, "output": 1}}, "dependencies": []})
}

fn config(temp: &Temp, cassette: &Path) -> PathBuf {
    let path = temp.root.join(format!(
        "config-{}.json",
        cassette.file_stem().unwrap().to_string_lossy()
    ));
    let value = json!({
        "system": temp.system,
        "kernel": temp.root.join("kernel"),
        "model": {"replay": cassette},
        "directory": {
            "state": temp.root.join("state"),
            "docker": "/usr/bin/docker",
            "python": "/usr/bin/python3",
            "image": "croon-work:1",
            "collector_image": "python:3.12-slim",
            "platform": "linux/amd64",
            "cpus": "0.5",
            "memory_bytes": 268435456,
            "work_bytes": 16777216,
            "temporary_bytes": 4194304,
            "pids": 64,
            "output_bytes": 65536,
            "nodes": 10000,
            "archive_bytes": 33554432,
            "cleanup_timeout_seconds": 30
        }
    });
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    path
}

#[test]
#[ignore = "requires local Docker with croon-work:1 and python:3.12-slim"]
fn w0_runs_with_a_replayed_model_and_real_docker_and_recovers_from_the_journal() {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "broken\n").unwrap();

    // W0 自己写文件，不交办：请求里没有 git diff 的真实输出，录像能事先写定。
    let scenario: Json = serde_json::from_str(SCENARIO).unwrap();
    let first_request = &scenario["values"]["sha256:lw0send0"];
    let task = "把 a.py 改成 fixed";
    let call =
        json!({"id": "c1", "name": "write_file", "input": {"path": "a.py", "content": "fixed\n"}});
    let user = json!({"role": "user", "text": task});
    let r0 = json!({"system": first_request["system"], "messages": [user], "tools": first_request["tools"]});
    let r1 = json!({"system": first_request["system"], "messages": [
        user,
        {"role": "assistant", "text": "", "reasoning": null, "calls": [call]},
        {"role": "tool", "results": [{"id": "c1", "name": "write_file", "output": "已写入 a.py", "is_error": false}]}
    ], "tools": first_request["tools"]});
    let cassette = temp.root.join("cassette.json");
    fs::write(
        &cassette,
        serde_json::to_vec(&json!({"entries": [
            {"request": r0, "outcome": llm("", json!([call]))},
            {"request": r1, "outcome": llm("改好了 a.py", json!([]))}
        ]}))
        .unwrap(),
    )
    .unwrap();

    let settings = host::Settings::load(&config(&temp, &cassette)).unwrap();
    let out1 = temp.root.join("out1");
    let first = host::run(&settings, &repo, task, &out1).unwrap();
    assert_eq!(first.summary, "改好了 a.py");
    assert_eq!(fs::read_to_string(out1.join("a.py")).unwrap(), "fixed\n");
    assert_eq!(
        fs::read_to_string(repo.join("a.py")).unwrap(),
        "broken\n",
        "用户仓库不动"
    );

    // 同一个 Kernel 目录再跑：空录像下任何一次模型调用都会失败，所以结果只能来自 Journal。
    let empty = temp.root.join("empty.json");
    fs::write(&empty, "{\"entries\":[]}").unwrap();
    let settings = host::Settings::load(&config(&temp, &empty)).unwrap();
    let out2 = temp.root.join("out2");
    let second = host::run(&settings, &repo, task, &out2).unwrap();
    assert_eq!(second.snapshot, first.snapshot);
    assert_eq!(second.summary, first.summary);
    assert_eq!(fs::read_to_string(out2.join("a.py")).unwrap(), "fixed\n");
}
