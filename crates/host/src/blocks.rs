//! 宿主的 block：配置、root Definition 与 `definition.infer`。大的在 schema/ 下各成一个文件，
//! 只有几行的直接写在这里；名字就是形状的出处。`$block` 在取用时展开，可以指向 Model、
//! Directory 与 Kernel 的 block。
//! 参考：docs/Host/README.md（配置、root entry、构造的推导）。

use croon_eval::blocks::{Block, expanded};

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
    Block {
        name: "definition.infer",
        lang: "json",
        text: r#"{
  "description": "从 Definition 草稿的 body 推出 Interface；纯计算，不建立构造事实",
  "in": {
    "type": "object",
    "properties": {
      "language": {"enum": ["lang0@1"]},
      "body": {
        "type": "object",
        "properties": {
          "source": {"type": "string"},
          "modules": {"type": "string", "description": "模块目录的 Tree ID，要在本次交付的依赖里"}
        },
        "required": ["source"],
        "additionalProperties": false
      }
    },
    "required": ["language", "body"],
    "additionalProperties": false
  },
  "attach": {"additionalProperties": false},
  "out": {
    "value": {
      "type": "object",
      "properties": {"interface": {"type": "object"}},
      "required": ["interface"],
      "additionalProperties": false
    },
    "attach": {"additionalProperties": false}
  }
}"#,
        file: "crates/host/src/blocks.rs",
        line: 0,
    },
];

pub fn json(name: &str) -> serde_json::Value {
    expanded(
        &[
            BLOCKS,
            model::BLOCKS,
            directory::BLOCKS,
            croon_port::blocks::BLOCKS,
        ],
        name,
    )
}
