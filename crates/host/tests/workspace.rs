//! 工作区的派生与采用规则，用真的 Lang0 求值，不经 Kernel：只调这些函数的 main 不报 Atom，一次 resume 就结束。
//! 参考：docs/Library/Runtime.md（工作区）。

use croon_port::store::{Budgets, Content, ContentStore};
use croon_port::{Bundle, HostError, Language, Read};
use lang0::Lang0;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 代码读取与业务读取都按同一个内容库；这里只有模块目录要读。
struct Code<'a>(&'a Content);
impl Read for Code<'_> {
    fn code_bytes(&self, id: &str) -> Result<Option<Vec<u8>>, HostError> {
        Ok(self.0.bytes(id)?)
    }
    fn code_json(&self, id: &str) -> Result<Option<Json>, HostError> {
        Ok(self.0.json(id)?)
    }
    fn code_bundle(&self, id: &str) -> Result<Option<Bundle>, HostError> {
        Ok(self.0.bundle(id)?)
    }
    fn bytes(&self, id: &str, _: &[String]) -> Result<Option<Vec<u8>>, HostError> {
        self.code_bytes(id)
    }
    fn json(&self, id: &str, _: &[String]) -> Result<Option<Json>, HostError> {
        self.code_json(id)
    }
    fn bundle(&self, id: &str, _: &[String]) -> Result<Option<Bundle>, HostError> {
        self.code_bundle(id)
    }
}

/// 把仓库的 library/ 与生成模块装成模块目录，求值 `main`，返回它的结束值。
fn run(source: &str) -> Json {
    let temp = Temp(std::env::temp_dir().join(format!(
        "croon-workspace-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    let library = temp.0.join("root/library");
    fs::create_dir_all(&library).unwrap();
    let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
    for entry in fs::read_dir(&source_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "py") {
            fs::copy(&path, library.join(path.file_name().unwrap())).unwrap();
        }
    }
    host::library::generate(&library);
    let content = Content::from_inputs(
        Rc::new(ContentStore::open(temp.0.join("store")).unwrap()),
        &[],
        Budgets {
            nodes: 10000,
            read_bytes: 1 << 22,
            write_bytes: 1 << 22,
        },
    )
    .unwrap();
    let modules = directory::import(
        &content,
        &temp.0.join("root"),
        directory::Limits {
            nodes: 1000,
            bytes: 1 << 22,
        },
    )
    .unwrap();
    let definition = json!({
        "language": "lang0@1",
        "body": {"source": source, "modules": modules},
        "interface": {
            "description": "",
            "signature": {
                "in": {"type": "object", "properties": {}, "required": [], "additionalProperties": false},
                "out": {}
            },
            "provides": {},
            "requires": {}
        }
    });
    let birth = json!({"definition": definition, "input": {"value": {}, "dependencies": []}, "bundle": "definition"});
    let read = Code(&content);
    let mut session = Lang0.open("test", &birth, &BTreeMap::new(), &read).unwrap();
    let out = session.resume(&BTreeMap::new(), &read).unwrap();
    assert!(
        out.get("atoms").is_none(),
        "工作区的派生与快路径采用不报 Atom：{out}"
    );
    let done = out["done"].clone();
    assert_eq!(done["ok"], true, "{done}");
    done["value"].clone()
}

#[test]
fn a_repeated_name_gets_the_next_free_suffix_and_never_overwrites() {
    // 旧写法按条目数加序号：已有 fix 与 fix3 时，第二个 fix 会成为 fix3，覆盖已有的那一位。
    let value = run("from typing import Any
from library.workspace import workspace, fork

def main() -> Any:
    ws = workspace(\"sha256:a\")
    names = [fork(ws, \"fix\"), fork(ws, \"fix3\"), fork(ws, \"fix\"), fork(ws, \"fix\")]
    return [names, sorted(ws[\"forks\"].keys())]
");
    assert_eq!(
        value,
        json!([
            ["fix", "fix3", "fix2", "fix4"],
            ["fix", "fix2", "fix3", "fix4"]
        ])
    );
}

#[test]
fn adoption_needs_a_candidate_and_takes_it_when_the_current_position_has_not_moved() {
    let value = run("from typing import Any
import croon
from library.workspace import workspace, fork, offer, adopt

def main() -> Any:
    ws = workspace(\"sha256:a\")
    name = fork(ws, \"a\")
    early = \"\"
    try:
        adopt(ws, None, name)
    except RuntimeError as e:
        early = str(e)
    offer(ws, name, \"sha256:b\")
    adopt(ws, None, name)
    missing = \"\"
    try:
        adopt(ws, None, \"none\")
    except KeyError as e:
        missing = croon.kind(e)
    return [early, ws[\"current\"], missing]
");
    assert_eq!(value, json!(["a 还没有交回", "sha256:b", "KeyError"]));
}
