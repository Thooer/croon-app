//! Tree Bundle 的领域校验。文件系统操作在 `execution`：那是提供者的实现，不属于形状。
//! 参考：docs/Directory/README.md（Tree）。

use croon_eval::host::HostError;
use croon_eval::store::Bundle;
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet};

mod blocks;

pub fn schema(name: &str) -> Json {
    blocks::json(name)
}
pub fn error(e: impl std::fmt::Display) -> HostError {
    HostError(e.to_string())
}
pub fn required<T>(v: Option<T>, id: &str) -> Result<T, HostError> {
    v.ok_or_else(|| error(format!("unreadable content: {id}")))
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && !matches!(name, "." | "..") && !name.contains(['/', '\0'])
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    File { bundle: String, executable: bool },
    Directory { bundle: String },
    Link { target: String },
}
pub type Tree = BTreeMap<String, Entry>;

/// 用调用者已有的受限读取结果校验；不扩大读取范围。
pub fn validate(bundle: &Bundle, value: &Json) -> Result<Tree, HostError> {
    croon_eval::schema::validate(&schema("directory.tree"), value)
        .map_err(|e| error(format!("{e:?}")))?;
    let mut tree = Tree::new();
    let mut dependencies = BTreeSet::new();
    for (name, entry) in value
        .as_object()
        .ok_or_else(|| error("Tree must be object"))?
    {
        if !valid_name(name) {
            return Err(error(format!("invalid Tree name {name:?}")));
        }
        let entry = if let Some(id) = entry.get("file").and_then(Json::as_str) {
            dependencies.insert(id.to_owned());
            Entry::File {
                bundle: id.into(),
                executable: entry["x"].as_bool().unwrap(),
            }
        } else if let Some(id) = entry.get("dir").and_then(Json::as_str) {
            dependencies.insert(id.to_owned());
            Entry::Directory { bundle: id.into() }
        } else {
            let target = entry["link"].as_str().unwrap();
            if target.contains('\0') {
                return Err(error("NUL in symlink target"));
            }
            Entry::Link {
                target: target.into(),
            }
        };
        tree.insert(name.clone(), entry);
    }
    if dependencies
        .iter()
        .any(|id| croon_eval::value::ValueId::parse(id).is_none())
    {
        return Err(error("invalid entry Bundle ID"));
    }
    if dependencies.into_iter().collect::<Vec<_>>() != bundle.dependencies {
        return Err(error("Tree entries and direct Bundle dependencies differ"));
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dependency_set_is_exact_and_names_cannot_traverse() {
        let id = croon_eval::value::ValueId::of_bytes(b"file").to_string();
        let bundle = Bundle {
            content: String::new(),
            dependencies: vec![id.clone()],
        };
        assert!(validate(&bundle, &json!({"a":{"file":id,"x":false}})).is_ok());
        assert!(validate(&bundle, &json!({"a":{"link":id}})).is_err());
        assert!(validate(&bundle, &json!({"../escape":{"file":id,"x":false}})).is_err());
        assert!(validate(&bundle, &json!({"a":{"file":id,"x":false,"extra":0}})).is_err());
    }
}
