//! Directory 的 block：文件执行 Port 的形状在 schema/ 下，名字就是出处。
//! 参考：docs/Directory/README.md（文件执行）。

use croon_eval::blocks::{Block, expanded};

pub static BLOCKS: &[Block] = &[Block {
    name: "directory.exec",
    lang: "json",
    text: include_str!("../schema/directory.exec.json"),
    file: "crates/directory/schema/directory.exec.json",
    line: 0,
}];

pub fn json(name: &str) -> serde_json::Value {
    expanded(&[BLOCKS], name)
}
