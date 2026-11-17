//! Library 的生成模块：把 Kernel、Model 与 Directory 的形状写成 Lang0 模块。
//! 参考：docs/Library/README.md（生成模块）。

use croon_port::blocks;
use lang0::library::{Decl, module};
use serde_json::Value as Json;
use std::fs;
use std::path::Path;

fn port_name(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// Library 的手写模块，编进二进制：宿主交出的 Library 只由这次构建决定。
pub const SOURCES: &[(&str, &str)] = &[
    ("agents.py", include_str!("../../../library/agents.py")),
    ("delegation.py", include_str!("../../../library/delegation.py")),
    ("files.py", include_str!("../../../library/files.py")),
    ("react.py", include_str!("../../../library/react.py")),
    ("s.py", include_str!("../../../library/s.py")),
    ("w0.py", include_str!("../../../library/w0.py")),
    (
        "workspace.py",
        include_str!("../../../library/workspace.py"),
    ),
];

/// 默认的工具清单，源码在仓库根的 home/tools.json，编进二进制。
pub const HOME_TOOLS: &str = include_str!("../../../home/tools.json");

/// 把默认工具清单写成家目录里的 tools.json：规范文本，W0 用 `croon.read_json` 读它。
pub fn install_home_tools(output: impl AsRef<Path>) {
    let tools: Json = serde_json::from_str(HOME_TOOLS).unwrap();
    croon_eval::schema::validate(&crate::schema("home.tools"), &tools).unwrap();
    fs::write(
        output.as_ref().join("tools.json"),
        croon_eval::value::canonical(&tools).unwrap(),
    )
    .unwrap();
}

/// 一个手写模块的源码。
pub fn source(name: &str) -> &'static str {
    SOURCES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .unwrap_or_else(|| panic!("Library 里没有 {name}"))
}

/// 函数工具包：库里有 `tools(supply, ws)` 的手写模块，名字就是模块名。清单按这个名字载入。
pub const TOOL_PACKAGES: &[&str] = &["files"];

/// 把手写模块与生成模块一起写进 `output`，得到完整的 library 目录。
pub fn install(output: impl AsRef<Path>) {
    let output = output.as_ref();
    fs::create_dir_all(output).unwrap();
    for (name, text) in SOURCES {
        fs::write(output.join(name), text).unwrap();
    }
    generate(output);
}

/// 把 kernel.py、model.py、directory.py、infer.py 写进 `output`。
pub fn generate(output: impl AsRef<Path>) {
    let output = output.as_ref();
    fs::create_dir_all(output).unwrap();
    let reference = blocks::json("reference.kernel");
    let names: Vec<_> = reference
        .as_object()
        .unwrap()
        .keys()
        .map(|name| port_name(name))
        .collect();
    let kernel_decls: Vec<_> = reference
        .as_object()
        .unwrap()
        .iter()
        .zip(names.iter())
        .map(|((_, spec), name)| Decl {
            name,
            spec,
            schema: false,
        })
        .collect();
    let kernel = module("from croon import Port, Schema\n\n", &kernel_decls);
    let outcome = blocks::json("definition.outcome");
    let outcome_decl = [Decl {
        name: "Outcome",
        spec: &outcome,
        schema: true,
    }];
    let kernel = kernel + &module("", &outcome_decl);
    fs::write(output.join("kernel.py"), kernel).unwrap();

    let model = port_module("Generate", model::schema("model.generate"));
    fs::write(output.join("model.py"), model).unwrap();
    let directory = port_module("Exec", directory::schema("directory.exec"));
    fs::write(output.join("directory.py"), directory).unwrap();
    let infer = port_module("Infer", crate::schema("definition.infer"));
    fs::write(output.join("infer.py"), infer).unwrap();
    fs::write(output.join("tools.py"), tools_module()).unwrap();
}

/// library.tools：诞生时可载入的函数工具包。Lang0 不能在运行中按名字取函数，所以这张清单由宿主生成、
/// 随目录 Bundle 固定；要新增一个包，加手写模块并列进 [`TOOL_PACKAGES`]。
fn tools_module() -> String {
    let mut out = String::from(
        "\"\"\"可载入的函数工具包：名字到它的 tools(supply, ws)。由宿主生成，随目录 Bundle 固定。\"\"\"\nfrom typing import Any, Callable\n",
    );
    for name in TOOL_PACKAGES {
        out.push_str(&format!("from library import {name} as _{name}\n"));
    }
    out.push_str("\n\ndef load(package: str, supply: dict[str, Any], ws: Any) -> list[Callable]:\n    \"\"\"载入一个函数工具包。\"\"\"\n");
    for name in TOOL_PACKAGES {
        out.push_str(&format!(
            "    if package == \"{name}\":\n        return _{name}.tools(supply, ws)\n"
        ));
    }
    out.push_str("    raise KeyError(f\"没有名为 {package} 的函数工具包\")\n");
    out
}

fn port_module(name: &'static str, spec: Json) -> String {
    let decl = Decl {
        name,
        spec: &spec,
        schema: false,
    };
    module("from croon import Port\n\n", &[decl])
}
