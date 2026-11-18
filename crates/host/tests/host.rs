//! 宿主装配的端到端：回放的模型、真的 Docker，跑完一次 W0 任务，结果物化到新目录；
//! 同一个 Kernel 目录再跑一次、模型换成空录像，仍得到同样的结果：结果来自 Journal，不再请求模型。
//! 参考：docs/Host/README.md（装配、客户端）；docs/Model/README.md（录制与回放）。

mod support;

use serde_json::{Value as Json, json};
use std::fs;
use support::{Temp, llm};

#[test]
#[ignore = "requires local Docker with croon-work:2 and python:3.12-slim"]
fn w0_runs_with_a_replayed_model_and_real_docker_and_recovers_from_the_journal() {
    let temp = Temp::new();
    let system = temp.system("w0");
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "broken\n").unwrap();

    // W0 自己写文件，不交办：请求里没有 git diff 的真实输出，录像能事先写定。
    let scenario: Json = host::scenario("library.scenario.delegate");
    let first_request = &scenario["values"]["sha256:lw0send0"];
    let task = "把 a.py 改成 fixed";
    let call =
        json!({"id": "c1", "name": "write_file", "input": {"path": "a.py", "content": "fixed\n"}});
    let user = json!({"role": "user", "text": task});
    let r0 = json!({"system": first_request["system"], "messages": [user], "tools": first_request["tools"]});
    let r1 = json!({"system": first_request["system"], "messages": [
        user,
        {"role": "assistant", "text": "", "reasoning": null, "calls": [call]},
        {"role": "tool", "results": [{"id": "c1", "name": "write_file", "output": "已写入 a.py", "is_error": false}]}
    ], "tools": first_request["tools"]});
    let cassette = temp.root.join("cassette.json");
    fs::write(
        &cassette,
        serde_json::to_vec(&json!({"entries": [
            {"request": r0, "outcome": llm("", json!([call]))},
            {"request": r1, "outcome": llm("改好了 a.py", json!([]))}
        ]}))
        .unwrap(),
    )
    .unwrap();

    let settings = host::Settings::load(&temp.config(&system, &cassette)).unwrap();
    let out1 = temp.root.join("out1");
    let first = host::run(&settings, host::Assembly::default(), &repo, task, &out1).unwrap();
    assert_eq!(first.value["summary"], "改好了 a.py");
    assert_eq!(fs::read_to_string(out1.join("a.py")).unwrap(), "fixed\n");
    assert_eq!(
        fs::read_to_string(repo.join("a.py")).unwrap(),
        "broken\n",
        "用户仓库不动"
    );

    // 同一个 Kernel 目录再跑：空录像下任何一次模型调用都会失败，所以结果只能来自 Journal。
    let empty = temp.root.join("empty.json");
    fs::write(&empty, "{\"entries\":[]}").unwrap();
    let settings = host::Settings::load(&temp.config(&system, &empty)).unwrap();
    let out2 = temp.root.join("out2");
    let second = host::resume(&settings, host::Assembly::default(), &out2).unwrap();
    assert_eq!(second.snapshot, first.snapshot);
    assert_eq!(second.value, first.value);
    assert_eq!(fs::read_to_string(out2.join("a.py")).unwrap(), "fixed\n");
}
