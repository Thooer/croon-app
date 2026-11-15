//! 交办的端到端：脚本化模型、真的 Docker，W0 交办给 S，审过再并入。
//! 快路径（W0 没动过就采用）与目录级三方合并（W0 自己也改过）各跑一遍，每遍三次：
//! 脚本化模型加录制；另一个系统只回放录像，结果相同；原 Kernel 目录配空录像，结果来自 Journal。
//! 录像只在临时目录里，不提交，所以不会随镜像或工具说明过期。
//! 参考：docs/Library/Runtime.md（工作区、交办）；docs/Host/README.md（运行参数）。

mod support;

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::fs;
use std::rc::Rc;
use support::{Temp, llm};

/// 按请求回答的模型：看 system 分辨 W0 与 S，看消息条数决定这一轮做什么。
struct Scripted {
    identity: String,
    three_way: bool,
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

fn delegation(three_way: bool) {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "broken\n").unwrap();
    let task = "修好 a.py";

    // 第一次：脚本化模型，边跑边录。配置里的回放来源不会被用到。
    let first = temp.system("record");
    let cassette = temp.root.join("cassette.json");
    let unused = temp.root.join("unused.json");
    let settings = host::Settings::load(&temp.config(&first, &unused)).unwrap();
    let scripted = Rc::new(Scripted {
        identity: format!("{first}/model"),
        three_way,
    });
    let recorder: Rc<dyn Port> = Rc::new(model::Recorder::new(scripted, &cassette).unwrap());
    let assembly = host::Assembly {
        model: Some(recorder),
        fault: None,
    };
    let out1 = temp.root.join("out1");
    let recorded = host::run(&settings, assembly, &repo, task, &out1).unwrap();
    assert_eq!(recorded.summary, "已修好 a.py");
    assert_eq!(fs::read_to_string(out1.join("a.py")).unwrap(), "fixed\n");
    if three_way {
        assert_eq!(fs::read_to_string(out1.join("o.txt")).unwrap(), "ours\n");
    } else {
        assert!(!out1.join("o.txt").exists());
    }
    assert_eq!(
        fs::read_to_string(repo.join("a.py")).unwrap(),
        "broken\n",
        "用户仓库不动"
    );

    // 第二次：另一个系统、新的 Kernel 目录，只回放。真 git 输出与产物快照都确定，结果逐字相同。
    let second = temp.system("replay");
    let settings = host::Settings::load(&temp.config(&second, &cassette)).unwrap();
    let out2 = temp.root.join("out2");
    let replayed = host::run(&settings, host::Assembly::default(), &repo, task, &out2).unwrap();
    assert_eq!(replayed.snapshot, recorded.snapshot);
    assert_eq!(replayed.summary, recorded.summary);

    // 第三次：第一个系统的 Kernel 目录配空录像，任何一次模型调用都会失败，结果只能来自 Journal。
    let empty = temp.root.join("empty.json");
    fs::write(&empty, "{\"entries\":[]}").unwrap();
    let settings = host::Settings::load(&temp.config(&first, &empty)).unwrap();
    let out3 = temp.root.join("out3");
    let again = host::run(&settings, host::Assembly::default(), &repo, task, &out3).unwrap();
    assert_eq!(again.snapshot, recorded.snapshot);
}

#[test]
#[ignore = "requires local Docker with croon-work:1 and python:3.12-slim"]
fn delegation_takes_the_candidate_when_w0_has_not_moved() {
    delegation(false);
}

#[test]
#[ignore = "requires local Docker with croon-work:1 and python:3.12-slim"]
fn delegation_merges_by_directory_when_w0_has_moved() {
    delegation(true);
}
