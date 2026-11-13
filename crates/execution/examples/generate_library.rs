//! Generate the Lang0 modules shipped by the app Library.

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

    let model = crate_module("Generate", execution::schema("model.generate"));
    fs::write(output.join("model.py"), model).unwrap();
    let directory = crate_module("Exec", execution::schema("directory.exec"));
    fs::write(output.join("directory.py"), directory).unwrap();
}

fn crate_module(name: &'static str, spec: Json) -> String {
    let decl = Decl {
        name,
        spec: &spec,
        schema: false,
    };
    module("from croon import Port\n\n", &[decl])
}

#[cfg(not(test))]
fn main() {
    let output = std::env::args_os()
        .nth(1)
        .expect("usage: generate_library OUTPUT");
    generate(output);
}
