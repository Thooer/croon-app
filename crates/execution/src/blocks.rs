//! App-owned Port blocks compiled into the execution crate.
//! Reference: docs/Directory/README.md (file execution); docs/Model/README.md.

pub use croon_eval::blocks::{Block, json_in};

pub static BLOCKS: &[Block] = &[
    Block {
        name: "directory.exec",
        lang: "json",
        text: include_str!("../schema/directory.exec.json"),
        file: "crates/execution/schema/directory.exec.json",
        line: 0,
    },
    Block {
        name: "model.generate",
        lang: "json",
        text: include_str!("../schema/model.generate.json"),
        file: "crates/execution/schema/model.generate.json",
        line: 0,
    },
];

pub fn json(name: &str) -> serde_json::Value {
    json_in(BLOCKS, name)
}
