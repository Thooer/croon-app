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

/// 把 kernel.py、model.py、directory.py 写进 `output`。
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
}

fn port_module(name: &'static str, spec: Json) -> String {
    let decl = Decl {
        name,
        spec: &spec,
        schema: false,
    };
    module("from croon import Port\n\n", &[decl])
}
