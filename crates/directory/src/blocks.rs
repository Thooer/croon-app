//! Directory 的 Port block，编进本 crate。
//! 参考：docs/Directory/README.md（文件执行）。

pub use croon_eval::blocks::{Block, json_in};

pub static BLOCKS: &[Block] = &[Block {
    name: "directory.exec",
    lang: "json",
    text: include_str!("../schema/directory.exec.json"),
    file: "crates/directory/schema/directory.exec.json",
    line: 0,
}];

pub fn json(name: &str) -> serde_json::Value {
    json_in(BLOCKS, name)
}
