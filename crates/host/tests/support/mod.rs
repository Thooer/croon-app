//! 宿主端到端测试共用的临时目录、按系统生成的配置与模型结果。
//! 每个系统各有自己的 Kernel 目录与 state 目录；结束时只清这些系统的容器。
#![allow(dead_code)]

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

    /// 这个系统的宿主配置，模型来源是回放 `cassette`；Kernel 与 state 目录按系统名分开。
    pub fn config(&self, system: &str, cassette: &Path) -> PathBuf {
        let path = self.root.join(format!(
            "config-{system}-{}.json",
            cassette.file_stem().unwrap().to_string_lossy()
        ));
        let value = json!({
            "system": system,
            "kernel": self.root.join(format!("kernel-{system}")),
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

impl Drop for Temp {
    fn drop(&mut self) {
        for system in self.systems.borrow().iter() {
            // 前缀由恢复身份派生，与 directory 的命名一致。
            let identity = format!("{system}/directory");
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
        }
        let _ = fs::remove_dir_all(&self.root);
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
