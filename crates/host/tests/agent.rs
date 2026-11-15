//! 造出来能交办的端到端：W0 构造一个 Agent，按它的 Interface 实例化、等结束、审过 diff 再并入。
//! 目标要的 Port 不在交办一方手里的报错，这一条单独在 W0 之外验，见 `run_agent` 的实现。
//! 参考：docs/Library/Runtime.md「工具」；docs/Plan.md（制造闭环）。

mod support;

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::fs;
use std::rc::Rc;
use support::{Temp, llm};

/// 造出来的 Agent：只改文件，交回总结与产物。它自己不交办，所以只要 engine 与 exec。
const WRITER: &str = r#"from typing import TypedDict
import croon
from library.model import Generate
from library.directory import Exec
from library.files import file_tools
from library.react import react
from library.workspace import workspace

Out = TypedDict("Out", {"summary": str, "snapshot": str})


def main(task: str, snapshot: str, engine: Generate, exec: Exec) -> Out:
    """改一处代码，交回总结与改过的 Bundle。

    Args:
        task: 要做的事。
    """
    ws = workspace(snapshot)
    summary = react(engine, file_tools(exec, ws), [{"role": "user", "text": task}],
                    "你在 /work 里改代码，改完用一句话说清。")
    return croon.deliver({"summary": summary, "snapshot": ws["current"]}, [ws["current"]])
"#;

/// 脚本化的模型：W0 与造出来的 Agent 共用同一个 engine，按 system 分辨。
struct Scripted {
    identity: String,
}

fn w0(step: usize) -> Json {
    match step {
        0 => llm(
            "",
            json!([{"id": "c1", "name": "construct_agent",
                    "input": {"name": "writer", "source": WRITER}}]),
        ),
        1 => llm(
            "",
            json!([{"id": "c2", "name": "delegate_to",
                    "input": {"name": "writer", "value": {"task": "修好 a.py"}}}]),
        ),
        2 => llm(
            "",
            json!([{"id": "c3", "name": "merge", "input": {"name": "writer"}}]),
        ),
        _ => llm("已造好 writer 并修好 a.py", json!([])),
    }
}

fn made(step: usize) -> Json {
    match step {
        0 => llm(
            "",
            json!([{"id": "d1", "name": "write_file",
                    "input": {"path": "a.py", "content": "fixed\n"}}]),
        ),
        _ => llm("改好了 a.py", json!([])),
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
        let step = call.value["messages"].as_array().unwrap().len() / 2;
        let system = call.value["system"].as_str().unwrap_or_default();
        let outcome = if system.contains("W0") {
            w0(step)
        } else {
            made(step)
        };
        Ok(Box::new(Ready(HostDone { outcome })))
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

#[test]
#[ignore = "requires local Docker with croon-work:1 and python:3.12-slim"]
fn w0_constructs_an_agent_delegates_to_it_and_merges_the_result() {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "broken\n").unwrap();
    let task = "造一个改代码的 Agent，用它修好 a.py";

    let system = temp.system("agent");
    let cassette = temp.root.join("cassette.json");
    let unused = temp.root.join("unused.json");
    let settings = host::Settings::load(&temp.config(&system, &unused)).unwrap();
    let scripted = Rc::new(Scripted {
        identity: format!("{system}/model"),
    });
    let recorder: Rc<dyn Port> = Rc::new(model::Recorder::new(scripted, &cassette).unwrap());
    let assembly = host::Assembly {
        model: Some(recorder),
        fault: None,
    };
    let out1 = temp.root.join("out1");
    let recorded = host::run(&settings, assembly, &repo, task, &out1).unwrap_or_else(|e| {
        panic!("任务没有完成：{e}");
    });
    assert_eq!(recorded.summary, "已造好 writer 并修好 a.py");
    assert_eq!(fs::read_to_string(out1.join("a.py")).unwrap(), "fixed\n");
    assert_eq!(
        fs::read_to_string(repo.join("a.py")).unwrap(),
        "broken\n",
        "用户仓库不动"
    );

    // 另一个系统只回放录像：造出来的 Agent 的请求也在录像里，结果逐字相同。
    let replay = temp.system("agent-replay");
    let settings = host::Settings::load(&temp.config(&replay, &cassette)).unwrap();
    let out2 = temp.root.join("out2");
    let played = host::run(&settings, host::Assembly::default(), &repo, task, &out2).unwrap();
    assert_eq!(played.snapshot, recorded.snapshot);
    assert_eq!(played.summary, recorded.summary);
}
