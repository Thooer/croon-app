//! 家目录的工具清单：装配时就检查，写错的名字与路径不等 W0 运行到才发现。不需要 Docker。
//! 参考：docs/Library/Packages.md（家目录）；docs/Library/Runtime.md（工具）。

use serde_json::json;

#[test]
fn the_default_manifest_is_valid() {
    let tools: serde_json::Value = serde_json::from_str(host::library::HOME_TOOLS).unwrap();
    host::library::check_tools(&tools).unwrap();
}

#[test]
fn a_function_package_the_library_does_not_list_is_refused() {
    let tools = json!({"tools": [{"kind": "functions", "package": "nope"}]});
    let error = host::library::check_tools(&tools).unwrap_err();
    assert!(error.contains("nope"), "{error}");
}

#[test]
fn an_agent_path_outside_the_library_is_refused() {
    let tools = json!({"tools": [{"kind": "agent", "name": "x", "path": "library/missing.py", "description": "d"}]});
    let error = host::library::check_tools(&tools).unwrap_err();
    assert!(error.contains("missing.py"), "{error}");
}

#[test]
fn a_manifest_with_the_wrong_shape_is_refused() {
    let tools = json!({"tools": [{"kind": "functions"}]});
    assert!(host::library::check_tools(&tools).is_err());
}
