//! construct_agent 的端到端：脚本化模型先交一份写错的源码，再交改对的，真的 definition.infer、pack、construct 检查它们。
//! 写错的以 use 失败带着 Lang0 的诊断回到模型，改对的构造成功；同一个 Kernel 目录配空录像重跑，结果来自 Journal。
//! 参考：docs/Plan.md（制造闭环）；docs/Library/Runtime.md（工具）。

mod support;

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use support::{Temp, llm};

const BAD: &str = "def main(x) -> int:\n    return x\n";
const GOOD: &str = "def main(x: int) -> int:\n    return x + 1\n";
/// 要的 Port 交办方给不了：构造通过，但返回值当场说出接不上，模型不必等到交办才发现。
const UNSUPPLIED: &str = r#"from typing import Any
from library.kernel import Pack


def main(pack: Pack) -> Any:
    """要 pack，交办方给不了。"""
    return None
"#;

/// 脚本化的 W0 模型：先交写错的源码，看到失败后交改对的，最后收尾；记下每次收到的请求。
struct Constructing {
    identity: String,
    seen: Rc<RefCell<Vec<Json>>>,
}

fn construct(id: &str, source: &str) -> Json {
    llm(
        "",
        json!([{"id": id, "name": "construct_agent", "input": {"name": "helper", "source": source}}]),
    )
}

impl Port for Constructing {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, call: &Call) -> Result<Box<dyn Execution>, HostError> {
        self.seen.borrow_mut().push(call.value.clone());
        let turn = call.value["messages"].as_array().unwrap().len() / 2;
        let outcome = match turn {
            0 => construct("c1", BAD),
            1 => construct("c2", GOOD),
            2 => construct("c3", UNSUPPLIED),
            _ => llm("已造好 helper", json!([])),
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
fn a_wrong_source_comes_back_as_a_diagnostic_and_a_fixed_one_is_constructed() {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "broken\n").unwrap();
    let task = "造一个加一的 Agent";

    let system = temp.system("construct");
    let unused = temp.root.join("unused.json");
    let settings = host::Settings::load(&temp.config(&system, &unused)).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let scripted: Rc<dyn Port> = Rc::new(Constructing {
        identity: format!("{system}/model"),
        seen: seen.clone(),
    });
    let assembly = host::Assembly {
        model: Some(scripted),
        fault: None,
    };
    let out1 = temp.root.join("out1");
    let first = host::run(&settings, assembly, &repo, task, &out1).unwrap();
    assert_eq!(first.summary, "已造好 helper");

    let seen = seen.borrow();
    assert_eq!(seen.len(), 4);
    let messages = seen[2]["messages"].as_array().unwrap();
    let failed = &messages[2]["results"][0];
    assert_eq!(failed["is_error"], true);
    assert_eq!(failed["output"]["error"], "use");
    let message = failed["output"]["message"].as_str().unwrap();
    assert!(message.contains("main 的参数须标类型"), "{message}");
    let built = &messages[4]["results"][0];
    assert_eq!(built["is_error"], false);
    assert_eq!(built["output"], "已构造 helper");
    let later = seen[3]["messages"].as_array().unwrap();
    let hint = &later[6]["results"][0];
    assert_eq!(hint["is_error"], false);
    let text = hint["output"].as_str().unwrap();
    assert!(text.contains("要 pack"), "{text}");

    // 同一个 Kernel 目录配空录像：任何一次模型调用都会失败，结果只能来自 Journal。
    let empty = temp.root.join("empty.json");
    fs::write(&empty, r#"{"entries":[]}"#).unwrap();
    let settings = host::Settings::load(&temp.config(&system, &empty)).unwrap();
    let out2 = temp.root.join("out2");
    let again = host::run(&settings, host::Assembly::default(), &repo, task, &out2).unwrap();
    assert_eq!(again.snapshot, first.snapshot);
    assert_eq!(again.summary, first.summary);
}
