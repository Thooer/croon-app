//! 宿主端到端测试共用的临时目录、按系统生成的配置与模型结果。
//! 每个系统各有自己的 Kernel 目录与 state 目录；结束时只清这些系统的容器。
#![allow(dead_code)]

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Temp {
    pub root: PathBuf,
    unique: String,
    systems: RefCell<Vec<String>>,
}

impl Temp {
    pub fn new() -> Self {
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
            unique,
            systems: RefCell::new(Vec::new()),
        }
    }

    /// 一个新的系统名；它的容器在结束时清掉。
    pub fn system(&self, label: &str) -> String {
        let system = format!("host-test-{}-{label}", self.unique);
        self.systems.borrow_mut().push(system.clone());
        system
    }

    /// 这个系统的 Kernel 目录。
    pub fn kernel(&self, system: &str) -> PathBuf {
        self.root.join(format!("kernel-{system}"))
    }

    /// 这个系统的宿主配置，模型来源是回放 `cassette`；Kernel 与 state 目录按系统名分开。
    pub fn config(&self, system: &str, cassette: &Path) -> PathBuf {
        let path = self.root.join(format!(
            "config-{system}-{}.json",
            cassette.file_stem().unwrap().to_string_lossy()
        ));
        let value = json!({
            "system": system,
            "kernel": self.kernel(system),
            "model": {"replay": cassette},
            "directory": {
                "state": self.root.join(format!("state-{system}")),
                "docker": "/usr/bin/docker",
                "python": "/usr/bin/python3",
                "image": "croon-work:1",
                "collector_image": "python:3.12-slim",
                "platform": "linux/amd64",
                "cpus": "0.5",
                "memory_bytes": 268435456,
                "work_bytes": 16777216,
                "temporary_bytes": 16777216,
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
}

/// 一个系统的文件执行容器都带 `croon.provider=<前缀>` 标签；前缀由恢复身份派生，与 directory 的命名一致。
pub fn prefix(system: &str) -> String {
    let identity = format!("{system}/directory");
    format!(
        "croon-exec-{}",
        croon_eval::value::ValueId::of_bytes(identity.as_bytes()).hex()
    )
}

impl Drop for Temp {
    fn drop(&mut self) {
        for system in self.systems.borrow().iter() {
            let prefix = prefix(system);
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
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// 按请求回答的模型：看 system 分辨 W0 与 S，看消息条数决定这一轮做什么。
/// W0 交办一处修改；`three_way` 时它在等结果之后自己也写一个文件，再并入，走目录级三方合并。
pub struct Scripted {
    pub identity: String,
    pub three_way: bool,
}

impl Scripted {
    fn answer(&self, request: &Json) -> Json {
        let messages = request["messages"].as_array().unwrap();
        if let Some(last) = messages.last()
            && last["role"] == "tool"
            && let Some(bad) = last["results"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["is_error"] == true)
        {
            // 工具失败就结束，把原因放进总结，测试在断言处看得到。
            return llm(&format!("工具失败：{}", bad["output"]), json!([]));
        }
        let w0 = request["system"].as_str().unwrap().contains("W0");
        let mut plan = Vec::new();
        if w0 {
            plan.push(("delegate", json!({"name": "fix", "task": "修好 a.py"})));
            if self.three_way {
                plan.push(("write_file", json!({"path": "o.txt", "content": "ours\n"})));
            }
            plan.push(("merge", json!({"name": "fix"})));
        } else {
            // S 只写文件、不提交：并入不看提交状态。
            plan.push(("write_file", json!({"path": "a.py", "content": "fixed\n"})));
        }
        let turn = messages.len() / 2;
        match plan.get(turn) {
            Some((name, input)) => llm(
                "",
                json!([{"id": format!("c{}", turn + 1), "name": name, "input": input}]),
            ),
            None => llm(
                if w0 {
                    "已修好 a.py"
                } else {
                    "改好了 a.py"
                },
                json!([]),
            ),
        }
    }
}

impl Port for Scripted {
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

/// 一次模型生成的结果：没有调用就是 stop。
pub fn llm(text: &str, calls: Json) -> Json {
    let finish = if calls.as_array().is_some_and(|c| c.is_empty()) {
        "stop"
    } else {
        "tool_calls"
    };
    json!({"ok": true, "value": {"text": text, "reasoning": null, "calls": calls, "finish": finish,
        "usage": {"input": 1, "output": 1}}, "dependencies": []})
}
