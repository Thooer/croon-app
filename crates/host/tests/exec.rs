//! 不经 W0 直接运行一份源码：输入与结果照它自己的 Signature，目录字段按需要给与取。
//! 实验的独立验收用它：在检验方自己的输入上，把模型造出来的源码再跑一遍。
//! 参考：docs/Host/README.md（运行哪一个 Definition）。

mod support;

use serde_json::json;
use std::fs;
use support::Temp;

const PURE: &str = "def main(numbers: list[int]) -> int:\n    return sum([n for n in numbers if n % 2 == 0])\n";
/// 要一个目录、交回一个目录：和 S 同一个形状，但不问模型，直接写一个文件。
const FILES: &str = r#"from typing import TypedDict
import croon
from library.directory import Exec
from library.workspace import advance, workspace

Out = TypedDict("Out", {"summary": str, "snapshot": str})


def main(snapshot: str, exec: Exec) -> Out:
    """往目录里写一个文件。"""
    ws = workspace(snapshot)
    advance(ws, exec, ["sh", "-c", "echo hello > made.txt"])
    return croon.deliver({"summary": "写好了", "snapshot": ws["current"]}, [ws["current"]])
"#;

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn a_source_runs_on_its_own_signature_with_or_without_a_directory() {
    let temp = Temp::new();
    let unused = temp.root.join("unused.json");
    fs::write(&unused, r#"{"entries":[]}"#).unwrap();

    // 纯计算：没有目录，也不起 exec。
    let system = temp.system("exec-pure");
    let settings = host::Settings::load(&temp.config(&system, &unused)).unwrap();
    let out = temp.root.join("out-pure");
    let done = host::exec(
        &settings,
        host::Assembly::default(),
        PURE,
        json!({"numbers": [1, 2, 3, 4, 10]}),
        None,
        &out,
    )
    .unwrap();
    assert_eq!(done.summary, "16");
    assert!(done.snapshot.is_empty());
    assert!(!out.exists(), "没有目录产物就不建结果目录");

    // 同一个 Kernel 目录续做，得到同样的结果：任务与输入取自 Journal。
    let again = host::resume(&settings, host::Assembly::default(), &out).unwrap();
    assert_eq!(again.summary, "16");

    // 要目录、交回目录：由调用方指定的目录填进输入，结果物化出来。
    let system = temp.system("exec-files");
    let settings = host::Settings::load(&temp.config(&system, &unused)).unwrap();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.txt"), "a\n").unwrap();
    let out = temp.root.join("out-files");
    let done = host::exec(&settings, host::Assembly::default(), FILES, json!({}), Some(&repo), &out).unwrap();
    assert_eq!(done.summary, "写好了");
    assert_eq!(fs::read_to_string(out.join("made.txt")).unwrap(), "hello\n");
    assert_eq!(fs::read_to_string(out.join("a.txt")).unwrap(), "a\n");
    assert!(!repo.join("made.txt").exists(), "调用方的目录不动");

    // 要目录却没给：当场说清。
    let system = temp.system("exec-missing");
    let settings = host::Settings::load(&temp.config(&system, &unused)).unwrap();
    let error = host::exec(&settings, host::Assembly::default(), FILES, json!({}), None, &temp.root.join("out-x"))
        .err()
        .expect("没给目录要报错");
    assert!(error.0.contains("snapshot"), "{error}");
}

const ASKING: &str = r#"from library.model import Generate
from library.model_call import generate


def main(topic: str, engine: Generate) -> str:
    r = generate(engine, {"system": "s", "messages": [{"role": "user", "text": topic}], "tools": []})
    return r["text"]
"#;

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn exec_works_without_any_model_and_a_source_that_asks_one_fails_with_a_clear_reason() {
    let temp = Temp::new();
    let settings = host::Settings::from_json(&json!({})).unwrap();
    let sessions = temp.root.join("sessions");
    let assembly = |name: &str| host::Assembly {
        session: Some(settings.session(Some(name), &sessions).unwrap()),
        quiet: true,
        ..Default::default()
    };

    // 纯计算：没有配置、没有模型，照样跑。
    let done = host::exec(&settings, assembly("pure"), PURE, json!({"numbers": [1, 2, 3, 4]}), None, &temp.root.join("o1")).unwrap();
    assert_eq!(done.summary, "6");

    // 要问模型的：那一次调用以 use 失败结束，说明没有配置模型，而不是一个看不懂的宿主故障。
    let error = host::exec(&settings, assembly("asks"), ASKING, json!({"topic": "咖啡"}), None, &temp.root.join("o2"))
        .err()
        .expect("没有模型时问模型要失败");
    assert!(error.0.contains("没有配置模型"), "{error}");
    assert!(error.0.contains("model.provider"), "要说怎么配：{error}");
}

#[test]
fn run_without_a_model_is_refused_before_anything_is_created() {
    let temp = Temp::new();
    let settings = host::Settings::from_json(&json!({})).unwrap();
    let sessions = temp.root.join("sessions");
    let error = host::run(
        &settings,
        host::Assembly {
            session: Some(settings.session(Some("nomodel"), &sessions).unwrap()),
            quiet: true,
            ..Default::default()
        },
        &temp.root,
        "x",
        &temp.root.join("o"),
    )
    .err()
    .expect("run 要模型");
    assert!(error.0.contains("provider") && error.0.contains("README"), "要说怎么配：{error}");
    assert!(!sessions.exists(), "拒绝时不留下会话目录");
}
