//! 真 Provider 的冒烟：record 模式跑完一次 W0 任务。只断言任务完成、结果目录存在、录像里没有凭据。
//! 会调用真模型并产生费用，所以 ignore；录像只在临时目录里，不提交。
//! provider 配置取自仓库的 config/agent-provider.json，它由 .gitignore 排除：从 config/agent-provider.example.json 复制并填好。
//! 没有这份文件时测试 panic 并说明缺什么，不会静默通过。
//! 参考：docs/Host/README.md（配置）；docs/Model/README.md（录制与回放）。

mod support;

use serde_json::{Value as Json, json};
use std::fs;
use std::path::Path;
use support::Temp;

#[test]
#[ignore = "calls the real model provider in config/agent-provider.json and needs local Docker"]
fn a_real_provider_completes_a_task_and_the_cassette_holds_no_credentials() {
    let provider = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/agent-provider.json");
    let provider = provider
        .canonicalize()
        .unwrap_or_else(|_| panic!("冒烟测试要 {}，按 model.provider 写好", provider.display()));
    let secrets: Json = serde_json::from_slice(&fs::read(&provider).unwrap()).unwrap();

    let temp = Temp::new();
    let repo = temp.root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("a.py"), "def add(a, b):\n    return a - b\n").unwrap();

    let system = temp.system("smoke");
    let cassette = temp.root.join("cassette.json");
    let config = temp.config(&system, &temp.root.join("unused.json"));
    let mut value: Json = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    value["model"] = json!({"provider": provider, "record": cassette});
    fs::write(&config, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    let settings = host::Settings::load(&config).unwrap();
    let out = temp.root.join("out");
    let finished = host::run(
        &settings,
        host::Assembly::default(),
        &repo,
        "a.py 里的 add 应当返回两数之和，修好它",
        &out,
    )
    .unwrap_or_else(|e| panic!("任务没有完成：{e}"));
    assert!(out.is_dir(), "结果目录存在");
    assert!(!finished.snapshot.is_empty());

    let tape = fs::read_to_string(&cassette).unwrap();
    for key in ["api_key", "base_url"] {
        let secret = secrets[key].as_str().unwrap();
        assert!(!tape.contains(secret), "录像里出现了 {key} 的字面值");
    }
}
