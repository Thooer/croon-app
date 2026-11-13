//! block 表：schema/ 下的 JSON 文件直接编进来，不经过 build.rs。
//! 参考：docs/Kernel/Engineering.md（接口）。

pub use croon_eval::blocks::{Block, get_in, json_in};

pub static BLOCKS: &[Block] = &[
    Block { name: "directory.exec", lang: "json", text: include_str!("../schema/directory.exec.json"), file: "crates/directory/schema/directory.exec.json", line: 0 },
    Block { name: "directory.tree", lang: "json", text: include_str!("../schema/directory.tree.json"), file: "crates/directory/schema/directory.tree.json", line: 0 },
];

pub fn get(name: &str) -> &'static Block {
    get_in(BLOCKS, name)
}

pub fn json(name: &str) -> serde_json::Value {
    json_in(BLOCKS, name)
}
