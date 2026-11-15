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
    ("files.py", include_str!("../../../library/files.py")),
    ("react.py", include_str!("../../../library/react.py")),
    ("s.py", include_str!("../../../library/s.py")),
    ("w0.py", include_str!("../../../library/w0.py")),
    (
        "workspace.py",
        include_str!("../../../library/workspace.py"),
    ),
];

/// 一个手写模块的源码。
pub fn source(name: &str) -> &'static str {
    SOURCES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .unwrap_or_else(|| panic!("Library 里没有 {name}"))
}

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
}

fn port_module(name: &'static str, spec: Json) -> String {
    let decl = Decl {
        name,
        spec: &spec,
        schema: false,
    };
    module("from croon import Port\n\n", &[decl])
}
