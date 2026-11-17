//! 不同形状的主体都能交办：纯计算、只用模型，与接线出错的两种情形，都在一次 W0 运行里走完。
//! 接线只按目标声明的名字取：缺的在 Library 里报，规格不同的由 Kernel 在实例化时拒绝，错误都以可读的形式回到模型。
//! 参考：docs/Library/Runtime.md（交办）。

mod support;

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use support::{Temp, llm};

const PURE: &str = "def main(x: int) -> int:\n    return x * 2\n";
/// 只用模型：问它一句，原样交回文本。
const ASKING: &str = r#"import croon
from library.model import Generate


def main(question: str, engine: Generate) -> str:
    """问模型一句。"""
    r = croon.send(engine, {"system": "答得简短", "messages": [{"role": "user", "text": question}],
                            "tools": []}).value
    return r["text"]
"#;
/// 把 exec 声明成另一种规格：名字对得上，规格对不上，Kernel 在实例化时拒绝。
const MISMATCH: &str = r#"from croon import Port


Weird = Port(str, doc="不是 directory.exec")


def main(exec: Weird) -> str:
    return "不会运行"
"#;

struct Shapes {
    identity: String,
    seen: Rc<RefCell<Vec<Json>>>,
}

fn call(id: &str, name: &str, input: Json) -> Json {
    llm("", json!([{"id": id, "name": name, "input": input}]))
}

impl Port for Shapes {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, call_: &Call) -> Result<Box<dyn Execution>, HostError> {
        let request = call_.value;
        let outcome = if request["system"].as_str().unwrap().contains("W0") {
            self.seen.borrow_mut().push(request.clone());
            match request["messages"].as_array().unwrap().len() / 2 {
                0 => call("c1", "make", json!({"name": "double", "source": PURE})),
                1 => call("c2", "double", json!({"x": 21})),
                2 => call("c3", "make", json!({"name": "ask", "source": ASKING})),
                3 => call("c4", "ask", json!({"question": "二加二"})),
                4 => call("c5", "double", json!({"x": 4})),
                5 => call("c6", "make", json!({"name": "weird", "source": MISMATCH})),
                6 => call("c7", "weird", json!({})),
                7 => call("c8", "nobody", json!({})),
                _ => llm("都试过了", json!([])),
            }
        } else {
            llm("四", json!([]))
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
fn different_shapes_run_and_a_wiring_mistake_comes_back_readable() {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "x\n").unwrap();
    let system = temp.system("shapes");
    let settings =
        host::Settings::load(&temp.config(&system, &temp.root.join("unused.json"))).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let assembly = host::Assembly {
        model: Some(Rc::new(Shapes {
            identity: format!("{system}/model"),
            seen: seen.clone(),
        })),
        fault: None,
        tools: None,
    };
    let out = temp.root.join("out");
    let done = host::run(&settings, assembly, &repo, "试几种主体", &out).unwrap();
    assert_eq!(done.summary, "都试过了");
    assert_eq!(fs::read_to_string(out.join("a.py")).unwrap(), "x\n");

    let seen = seen.borrow();
    let last = seen.last().unwrap()["messages"].as_array().unwrap().clone();
    let result = |n: usize| last[2 * n + 2]["results"][0].clone();
    assert_eq!(result(0)["output"]["tool"], "double");
    assert_eq!(result(1)["output"], json!(42));
    assert_eq!(result(3)["output"], json!("四"));
    assert_eq!(result(4)["output"], json!(8), "再调用不用再给源码");
    let weird = result(6);
    assert_eq!(weird["is_error"], true, "{weird}");
    assert_eq!(weird["output"]["error"], "type", "{weird}");
    let nobody = result(7);
    assert_eq!(nobody["is_error"], true);
    assert!(
        nobody["output"]["message"].as_str().unwrap().contains("nobody"),
        "{nobody}"
    );
}
