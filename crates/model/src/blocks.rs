//! Model 的 Port 与配置 block，编进本 crate。
//! 参考：docs/Model/README.md（模型服务、宿主适配）。

pub use croon_eval::blocks::{Block, json_in};

pub static BLOCKS: &[Block] = &[
    Block {
        name: "model.generate",
        lang: "json",
        text: include_str!("../schema/model.generate.json"),
        file: "crates/model/schema/model.generate.json",
        line: 0,
    },
    Block {
        name: "model.provider",
        lang: "json",
        text: include_str!("../schema/model.provider.json"),
        file: "crates/model/schema/model.provider.json",
        line: 0,
    },
];

pub fn json(name: &str) -> serde_json::Value {
    json_in(BLOCKS, name)
}
