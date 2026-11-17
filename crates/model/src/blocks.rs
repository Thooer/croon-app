//! Model 的 block：Port、录像与 Provider 配置的形状。大的在 schema/ 下各成一个文件，
//! 只有几行的直接写在这里；名字就是形状的出处，`$block` 在取用时展开。
//! 参考：docs/Model/README.md（模型服务、宿主适配、录制与回放）。

use croon_eval::blocks::{Block, expanded, find};

pub static BLOCKS: &[Block] = &[
    Block {
        name: "model.generate",
        lang: "json",
        text: include_str!("../schema/model.generate.json"),
        file: "crates/model/schema/model.generate.json",
        line: 0,
    },
    Block {
        name: "model.cassette",
        lang: "json",
        text: include_str!("../schema/model.cassette.json"),
        file: "crates/model/schema/model.cassette.json",
        line: 0,
    },
    Block {
        name: "model.provider",
        lang: "json",
        text: r#"{
  "type": "object",
  "properties": {
    "protocol": { "enum": ["openai_chat_completions"] },
    "base_url": { "type": "string" },
    "api_key": { "type": "string" },
    "model": { "type": "string" },
    "max_output_tokens": { "type": "integer" },
    "timeout_seconds": { "type": "integer" }
  },
  "required": ["protocol", "base_url", "api_key", "model", "max_output_tokens", "timeout_seconds"],
  "additionalProperties": false
}"#,
        file: "crates/model/src/blocks.rs",
        line: 0,
    },
];

/// 本 crate 的 block，找不到的沿依赖往下找。
pub fn block(name: &str) -> Option<&'static Block> {
    find(BLOCKS, name).or_else(|| croon_port::blocks::block(name))
}

/// 一份 block，`$block` 已展开。
pub fn json(name: &str) -> serde_json::Value {
    expanded(name, &block)
}
