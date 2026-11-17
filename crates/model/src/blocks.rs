//! Model 的 block：Port、录像与 Provider 配置的形状。大的在 schema/ 下各成一个文件，
//! 只有几行的直接写在这里；名字就是形状的出处，`$block` 在取用时展开。
//! 参考：docs/Model/README.md（模型服务、宿主适配、录制与回放）。

use croon_eval::blocks::{Block, chain, expanded, find};

pub static BLOCKS: &[Block] =
    croon_eval::block_files!["model.generate", "model.cassette", "model.provider",];

/// 本 crate 的 block，找不到的沿依赖往下找。
pub fn block(name: &str) -> Option<&'static Block> {
    chain(find(BLOCKS, name), || croon_port::blocks::block(name))
}

/// 一份 block，`$block` 已展开。
pub fn json(name: &str) -> serde_json::Value {
    expanded(name, &block)
}
