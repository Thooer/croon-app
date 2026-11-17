//! 进度：在等结束的间隙读 Journal 里新增的记录，每步打印一行到标准错误。
//! 只读已提交的事实，不为它在 Library 里加东西；`--quiet` 时不打。
//! 续做时先静默读完已有的记录，只打印之后新发生的，开头一行说明已经完成了多少。
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
    /// 续做时为真：第一次 `report` 只数不打，打一行总结。
    replaying: bool,
}

impl Progress {
    pub fn new(quiet: bool, resuming: bool) -> Self {
        Self {
            seen: BTreeMap::new(),
            started: Instant::now(),
            model_calls: 0,
            tokens: (0, 0),
            quiet,
            replaying: resuming,
        }
    }

    /// 读各 Instance 里新增的记录，打印它们。`records` 是 (实例, 它的全部记录)；
    /// `content` 按 ID 取一份内容（命令要从那里读），取不到就不显示。
    pub fn report<'a>(
        &mut self,
        records: impl Iterator<Item = (&'a str, &'a [Json])>,
        content: &dyn Fn(&str) -> Option<Json>,
    ) {
        let quiet = self.quiet;
        if self.replaying {
            // 已有的历史：算进累计的数，但不一条条打。
            self.quiet = true;
        }
        let mut steps = 0;
        for (instance, all) in records {
            let from = *self.seen.get(instance).unwrap_or(&0);
            for record in &all[from.min(all.len())..] {
                steps += usize::from(self.line(record, content));
            }
            self.seen.insert(instance.to_owned(), all.len());
        }
        if self.replaying {
            self.replaying = false;
            self.quiet = quiet;
            if steps > 0 {
                self.print(format!(
                    "接着上次做：已经有 {} 次模型调用（累计输入 {}、输出 {} token）、{steps} 步，下面是新发生的",
                    self.model_calls, self.tokens.0, self.tokens.1
                ));
            }
            // 计时从这里开始，不把历史的耗时算进去。
            self.started = Instant::now();
        }
    }

    /// 打印一条记录，返回它是不是值得算一步。
    fn line(&mut self, record: &Json, content: &dyn Fn(&str) -> Option<Json>) -> bool {
        let Some(send) = record["what"].get("send") else {
            if record["what"].get("end").is_some() {
                self.print("一个主体结束了".into());
            }
            return false;
        };
        let value = &send["result"]["outcome"];
        match send["to"]["port"].as_str() {
            Some("engine") => {
                self.model_calls += 1;
                let usage = &value["value"]["usage"];
                self.tokens.0 += usage["input"].as_u64().unwrap_or(0);
                self.tokens.1 += usage["output"].as_u64().unwrap_or(0);
                if value["ok"] == false {
                    let message = value["error"]["message"].as_str().unwrap_or("");
                    self.print(format!("模型调用失败：{}", short(message, 80)));
                } else {
                    let calls = value["value"]["calls"].as_array().cloned().unwrap_or_default();
                    let what = if calls.is_empty() {
                        "给出回答".to_owned()
                    } else {
                        let names: Vec<_> = calls.iter().filter_map(|c| c["name"].as_str()).collect();
                        format!("要调用 {}", names.join("、"))
                    };
                    self.print(format!(
                        "模型第 {} 次：{what}（累计输入 {}、输出 {} token）",
                        self.model_calls, self.tokens.0, self.tokens.1
                    ));
                }
            }
            Some("exec") => {
                let command = send["delivery"]["value"]
                    .as_str()
                    .and_then(content)
                    .and_then(|v| v["argv"].as_array().cloned())
                    .map(|argv| describe(&argv))
                    .unwrap_or_else(|| "一条命令".into());
                let verdict = match value["value"]["exit"].as_i64() {
                    Some(0) => "成功".to_owned(),
                    Some(code) => format!("退出码 {code}"),
                    None => "失败".to_owned(),
                };
                self.print(format!("容器里运行：{command} → {verdict}"));
            }
            Some("infer") => self.print("检查一份源码".into()),
            Some("construct") => self.print("构造一个 Agent".into()),
            Some("instantiate") => self.print("启动一个 Agent".into()),
            _ => return false,
        }
        true
    }

    fn print(&self, text: String) {
        if !self.quiet {
            eprintln!("[{:>4}s] {text}", self.started.elapsed().as_secs());
        }
    }
}

/// 一条命令的一句话：`sh -c "..."` 取脚本本身，其余把参数连起来；只留第一行，截短。
fn describe(argv: &[Json]) -> String {
    let words: Vec<&str> = argv.iter().filter_map(Json::as_str).collect();
    let text = match words.as_slice() {
        // 文件工具写成 `sh -c 'cat > "$1"' sh 路径`：脚本里的 $1 是后面的参数，一起写出来才看得出写的哪个文件。
        ["sh", "-c", script, _, rest @ ..] if script.contains("$1") && !rest.is_empty() => {
            format!("{} （$1 = {}）", script, rest.join(" "))
        }
        ["sh", "-c", script, ..] => (*script).to_owned(),
        _ => words.join(" "),
    };
    short(text.lines().next().unwrap_or(""), 100)
}

fn short(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_shell_script_shows_itself_and_other_commands_show_their_words() {
        assert_eq!(describe(&[json!("cat"), json!("--"), json!("a.py")]), "cat -- a.py");
        assert_eq!(
            describe(&[json!("sh"), json!("-c"), json!("pwd; rg --files\nsecond line")]),
            "pwd; rg --files"
        );
        assert_eq!(
            describe(&[json!("sh"), json!("-c"), json!("cat > \"$1\""), json!("sh"), json!("a.py")]),
            "cat > \"$1\" （$1 = a.py）"
        );
        let long = "x".repeat(150);
        assert_eq!(describe(&[json!("sh"), json!("-c"), json!(long)]).chars().count(), 101);
    }
}
