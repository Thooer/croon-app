//! definition.infer：从草稿推出 Interface；写错时 Lang0 的诊断原样作 use 失败交回。不需要 Docker。

use croon_port::{Budgets, Call, Content, ContentStore, Port};
use host::infer::Infer;
use serde_json::{Value as Json, json};
use std::rc::Rc;

const BUDGETS: Budgets = Budgets {
    nodes: 1_000_000,
    read_bytes: 1 << 30,
    write_bytes: 1 << 30,
};

fn infer(source: &str) -> Json {
    let root = std::env::temp_dir().join(format!(
        "croon-infer-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Rc::new(ContentStore::open(root.clone()).unwrap());
    let content = Rc::new(Content::from_inputs(store, &[], BUDGETS).unwrap());
    let value = json!({"language": "lang0@1", "body": {"source": source}});
    let call = Call {
        instance: "i",
        generation: 0,
        key: "k",
        source: "s",
        attempt: "a",
        value: &value,
        dependencies: &[],
        content,
    };
    let port = Infer::new("t/infer");
    let mut run = port.start(&call).unwrap();
    let done = run.collect().unwrap().unwrap();
    let _ = std::fs::remove_dir_all(&root);
    done.outcome
}

#[test]
fn a_valid_main_gives_its_interface() {
    let out = infer("def main(x: int) -> int:\n    \"\"\"加一。\"\"\"\n    return x + 1\n");
    assert_eq!(out["ok"], true, "{out}");
    let interface = &out["value"]["interface"];
    assert_eq!(interface["description"], "加一。");
    assert_eq!(interface["signature"]["in"]["required"], json!(["x"]));
}

#[test]
fn a_missing_annotation_comes_back_as_a_use_failure_with_lang0_diagnostic() {
    let out = infer("def main(x) -> int:\n    return x\n");
    assert_eq!(out["ok"], false, "{out}");
    assert_eq!(out["error"]["kind"], "use");
    let message = out["error"]["message"].as_str().unwrap();
    assert!(message.contains("main 的参数须标类型"), "{message}");
}

#[test]
fn a_source_without_main_is_a_use_failure() {
    let out = infer("def helper() -> int:\n    return 1\n");
    assert_eq!(out["ok"], false, "{out}");
    assert_eq!(out["error"]["kind"], "use");
    let message = out["error"]["message"].as_str().unwrap();
    assert!(message.contains("def main"), "{message}");
}

#[test]
fn modules_in_the_delivery_are_readable_and_the_w0_source_infers() {
    let root = std::env::temp_dir().join(format!(
        "croon-infer-modules-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Rc::new(ContentStore::open(root.join("content")).unwrap());
    let importer = Content::from_inputs(store.clone(), &[], BUDGETS).unwrap();
    host::library::install(root.join("src").join("library"));
    let limits = directory::Limits {
        nodes: 100_000,
        bytes: 1 << 26,
    };
    let modules = directory::import(&importer, &root.join("src"), limits).unwrap();
    let content = Rc::new(Content::from_inputs(store, &[modules.clone()], BUDGETS).unwrap());
    let value = json!({"language": "lang0@1", "body": {"source": host::library::source("w0.py"), "modules": modules}});
    let dependencies = [modules.clone()];
    let call = Call {
        instance: "i",
        generation: 0,
        key: "k",
        source: "s",
        attempt: "a",
        value: &value,
        dependencies: &dependencies,
        content,
    };
    let port = Infer::new("t/infer");
    let mut run = port.start(&call).unwrap();
    let out = run.collect().unwrap().unwrap().outcome;
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(out["ok"], true, "{out}");
    let requires = &out["value"]["interface"]["requires"];
    for port in ["engine", "exec", "instantiate", "back"] {
        assert!(
            !requires[port].is_null(),
            "W0 的 Interface 缺 {port}：{requires}"
        );
    }
}
