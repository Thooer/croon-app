//! 构造并入实例化的端到端：脚本化模型交的源码，经真的 definition.infer、pack、construct 检查，再实例化、收回结果。
//! 写错的以 use 失败带着 Lang0 的诊断回到模型，改对的就跑起来；要的 Port 交办一方给不了的当场报错。
//! 这个主体没有目录也没有模型，所以这一条同时验证纯计算的主体不拿工作区、不多起一次 exec。
//! 同一个 Kernel 目录配空录像重跑，结果来自 Journal。
//! 参考：docs/Plan.md（制造闭环）；docs/Library/Runtime.md（工具、交办）。

mod support;

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use support::{Temp, llm};

const BAD: &str = "def main(x) -> int:\n    return x\n";
const GOOD: &str = "def main(x: int) -> int:\n    return x + 1\n";
/// 要的 Port 交办方给不了：构造前就说出来，模型不必等到运行才发现。
const UNSUPPLIED: &str = r#"from typing import Any
from library.kernel import Pack


def main(pack: Pack) -> Any:
    """要 pack，交办方给不了。"""
    return None
"#;

/// 脚本化的 W0 模型：先交写错的源码，看到失败后交改对的，再交一个交办方给不了 Port 的，最后收尾；记下每次收到的请求。
struct Constructing {
    identity: String,
    seen: Rc<RefCell<Vec<Json>>>,
}

fn make(id: &str, source: &str) -> Json {
    llm(
        "",
        json!([{"id": id, "name": "make", "input": {"name": "helper", "source": source}}]),
    )
}

fn invoke(id: &str, name: &str) -> Json {
    llm("", json!([{"id": id, "name": name, "input": {"x": 1}}]))
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
            0 => make("c1", BAD),
            1 => make("c2", GOOD),
            2 => invoke("c3", "helper"),
            3 => make("c4", UNSUPPLIED),
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
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn a_wrong_source_comes_back_as_a_diagnostic_and_a_fixed_one_runs() {
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
        tools: None,
    };
    let out1 = temp.root.join("out1");
    let first = host::run(&settings, assembly, &repo, task, &out1).unwrap();
    assert_eq!(first.summary, "已造好 helper");
    assert_eq!(
        fs::read_to_string(out1.join("a.py")).unwrap(),
        "broken\n",
        "纯计算的主体不动工作区"
    );

    let seen = seen.borrow();
    assert_eq!(seen.len(), 5);
    let messages = seen[1]["messages"].as_array().unwrap();
    let failed = &messages[2]["results"][0];
    assert_eq!(failed["is_error"], true);
    assert_eq!(failed["output"]["error"], "use");
    let message = failed["output"]["message"].as_str().unwrap();
    assert!(message.contains("main 的参数须标类型"), "{message}");
    let made = &seen[2]["messages"].as_array().unwrap()[4]["results"][0];
    assert_eq!(made["is_error"], false);
    assert_eq!(made["output"]["tool"], "helper");
    let tools = seen[3]["tools"].as_array().unwrap();
    assert!(
        tools.iter().any(|t| t["name"] == "helper"),
        "造好的主体下一轮就在工具表里"
    );
    let ran = &seen[3]["messages"].as_array().unwrap()[6]["results"][0];
    assert_eq!(ran["is_error"], false);
    assert_eq!(ran["output"], json!(2));
    let later = seen[4]["messages"].as_array().unwrap();
    let hint = &later[8]["results"][0];
    assert_eq!(hint["is_error"], true);
    let text = hint["output"]["message"].as_str().unwrap();
    assert!(text.contains("pack"), "{text}");

    // 同一个 Kernel 目录配空录像：任何一次模型调用都会失败，结果只能来自 Journal。
    let empty = temp.root.join("empty.json");
    fs::write(&empty, r#"{"entries":[]}"#).unwrap();
    let settings = host::Settings::load(&temp.config(&system, &empty)).unwrap();
    let out2 = temp.root.join("out2");
    let again = host::resume(&settings, host::Assembly::default(), &out2).unwrap();
    assert_eq!(again.snapshot, first.snapshot);
    assert_eq!(again.summary, first.summary);
}
