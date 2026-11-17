//! 模型调用的重试：偶发的失败（429、5xx、超时）由 Library 的 `generate` 重试，每次是一个新键的 Atom；
//! 请求内容被拒这类不重试；重试用尽原失败照常向外。不需要 Docker：只用脚本化的模型。
//! 参考：docs/Library/Runtime.md「模型调用」；docs/Model/README.md「失败」。

mod support;

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use support::{Temp, llm};

/// 一个按次数回答的模型：`plan` 里每一项是这一次的回答，用完之后一直成功。
struct Flaky {
    identity: String,
    plan: Vec<Json>,
    asked: Rc<RefCell<usize>>,
}

fn fail(status: Json) -> Json {
    json!({"ok": false, "error": {"kind": "use", "message": "fixture", "data": {"status": status, "type": "t"}}, "dependencies": []})
}

impl Port for Flaky {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, _call: &Call) -> Result<Box<dyn Execution>, HostError> {
        let n = {
            let mut asked = self.asked.borrow_mut();
            *asked += 1;
            *asked - 1
        };
        let outcome = self.plan.get(n).cloned().unwrap_or_else(|| llm("好了", json!([])));
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

fn run_with(plan: Vec<Json>) -> (Result<host::Finished, host::Error>, usize) {
    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "x\n").unwrap();
    let system = temp.system("retry");
    let settings =
        host::Settings::load(&temp.config(&system, &temp.root.join("unused.json"))).unwrap();
    let asked = Rc::new(RefCell::new(0));
    let assembly = host::Assembly {
        model: Some(Rc::new(Flaky {
            identity: format!("{system}/model"),
            plan,
            asked: asked.clone(),
        })),
        fault: None,
        tools: None,
    };
    let done = host::run(&settings, assembly, &repo, "说一句话", &temp.root.join("out"));
    let n = *asked.borrow();
    (done, n)
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn a_transient_failure_is_retried_and_each_try_is_its_own_action() {
    // 503、429、超时（status 为空）各一次，再成功：共 4 次模型调用。
    let (done, asked) = run_with(vec![fail(json!(503)), fail(json!(429)), fail(Json::Null)]);
    let done = done.unwrap_or_else(|e| panic!("偶发失败应当被重试掉：{e}"));
    assert_eq!(done.summary, "好了");
    assert_eq!(asked, 4);
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn a_rejected_request_is_not_retried() {
    // 400 是请求内容被拒，重做不会更好：只问一次，失败照常向外。
    let (done, asked) = run_with(vec![fail(json!(400))]);
    let error = done.err().expect("请求被拒要失败");
    assert!(error.0.contains("fixture"), "{error}");
    assert_eq!(asked, 1);
}

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn retries_are_bounded_and_the_original_failure_comes_out() {
    // 一直 503：第一次加四次重试，共五次，然后向外。
    let (done, asked) = run_with(vec![fail(json!(503)); 20]);
    let error = done.err().expect("一直失败要向外");
    assert!(error.0.contains("503") || error.0.contains("fixture"), "{error}");
    assert_eq!(asked, 5);
}
