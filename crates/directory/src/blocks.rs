//! Directory 的 block：文件执行 Port 的形状在 schema/ 下，名字就是出处。
//! 参考：docs/Directory/README.md（文件执行）。

use croon_eval::blocks::{Block, chain, expanded, find};

pub static BLOCKS: &[Block] = croon_eval::block_files!["directory.exec",];

/// 本 crate 的 block，找不到的沿依赖往下找。
pub fn block(name: &str) -> Option<&'static Block> {
    chain(find(BLOCKS, name), || croon_port::blocks::block(name))
}

/// 一份 block，`$block` 已展开。
pub fn json(name: &str) -> serde_json::Value {
    expanded(name, &block)
}
