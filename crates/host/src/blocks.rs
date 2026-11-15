//! 宿主的配置与 root Definition block，编进本 crate。
//! 参考：docs/Host/README.md（配置、root entry）。

pub use croon_eval::blocks::{Block, json_in};

pub static BLOCKS: &[Block] = &[
    Block {
        name: "host.config",
        lang: "json",
        text: include_str!("../schema/host.config.json"),
        file: "crates/host/schema/host.config.json",
        line: 0,
    },
    Block {
        name: "host.root",
        lang: "json",
        text: include_str!("../schema/host.root.json"),
        file: "crates/host/schema/host.root.json",
        line: 0,
    },
];

pub fn json(name: &str) -> serde_json::Value {
    json_in(BLOCKS, name)
}
