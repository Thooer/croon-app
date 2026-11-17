//! 进度：在等结束的间隙读 Journal 里新增的记录，每步打印一行到标准错误。
//! 只读已提交的事实，不为它在 Library 里加东西；`--quiet` 时不打。
//! 参考：docs/Host/README.md（配置与会话「进度」）。

use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::time::Instant;

/// 每个 Instance 已经读到哪一条记录。
pub struct Progress {
    seen: BTreeMap<String, usize>,
    started: Instant,
    model_calls: usize,
    tokens: (u64, u64),
    quiet: bool,
}

impl Progress {
    pub fn new(quiet: bool) -> Self {
        Self {
            seen: BTreeMap::new(),
            started: Instant::now(),
            model_calls: 0,
            tokens: (0, 0),
            quiet,
        }
    }

    /// 读各 Instance 里新增的记录，打印它们。`records` 是 (实例, 它的全部记录)。
    pub fn report<'a>(&mut self, records: impl Iterator<Item = (&'a str, &'a [Json])>) {
        for (instance, all) in records {
            let from = *self.seen.get(instance).unwrap_or(&0);
            for record in &all[from.min(all.len())..] {
                self.line(record);
            }
            self.seen.insert(instance.to_owned(), all.len());
        }
    }

    fn line(&mut self, record: &Json) {
        let Some(send) = record["what"].get("send") else {
            if record["what"].get("end").is_some() {
                self.print("一个主体结束了".into());
            }
            return;
        };
        let value = &send["result"]["outcome"];
        match send["to"]["port"].as_str() {
            Some("engine") => {
                self.model_calls += 1;
                let usage = &value["value"]["usage"];
                self.tokens.0 += usage["input"].as_u64().unwrap_or(0);
                self.tokens.1 += usage["output"].as_u64().unwrap_or(0);
                let calls = value["value"]["calls"].as_array().cloned().unwrap_or_default();
                let what = if calls.is_empty() {
                    "给出回答".to_owned()
                } else {
                    let names: Vec<_> = calls.iter().filter_map(|c| c["name"].as_str()).collect();
                    format!("要调用 {}", names.join("、"))
                };
                if value["ok"] == false {
                    let message = value["error"]["message"].as_str().unwrap_or("");
                    self.print(format!("模型调用失败：{}", short(message)));
                } else {
                    self.print(format!(
                        "模型第 {} 次：{what}（累计输入 {}、输出 {} token）",
                        self.model_calls, self.tokens.0, self.tokens.1
                    ));
                }
            }
            Some("exec") => {
                let exit = value["value"]["exit"].as_i64();
                self.print(match exit {
                    Some(0) => "在容器里运行了一条命令，成功".into(),
                    Some(code) => format!("在容器里运行了一条命令，退出码 {code}"),
                    None => "在容器里运行命令失败".into(),
                });
            }
            Some("infer") => self.print("检查一份源码".into()),
            Some("construct") => self.print("构造一个 Agent".into()),
            Some("instantiate") => self.print("启动一个 Agent".into()),
            _ => {}
        }
    }

    fn print(&self, text: String) {
        if !self.quiet {
            eprintln!("[{:>4}s] {text}", self.started.elapsed().as_secs());
        }
    }
}

fn short(text: &str) -> String {
    let mut out: String = text.chars().take(80).collect();
    if text.chars().count() > 80 {
        out.push('…');
    }
    out
}
