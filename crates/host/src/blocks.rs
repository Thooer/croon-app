//! 宿主的 block：配置、root Definition 与 `definition.infer`。大的在 schema/ 下各成一个文件，
//! 只有几行的直接写在这里；名字就是形状的出处。`$block` 在取用时展开，可以指向 Model、
//! Directory 与 Kernel 的 block。
//! 参考：docs/Host/README.md（配置、root entry、构造的推导）。

use croon_eval::blocks::{Block, chain, expanded, find};

pub static BLOCKS: &[Block] = croon_eval::block_files![
    "host.config",
    "host.root",
    "library.scenario.delegate",
    "definition.infer",
];

/// 本 crate 的 block，找不到的沿依赖往下找。
pub fn block(name: &str) -> Option<&'static Block> {
    chain(find(BLOCKS, name), || {
        chain(model::block(name), || directory::block(name))
    })
}

/// 一份 block，`$block` 已展开。
pub fn json(name: &str) -> serde_json::Value {
    expanded(name, &block)
}
