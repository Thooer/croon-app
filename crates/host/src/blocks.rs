//! 宿主的 block：配置、root Definition、工具清单与 `definition.infer`，每份在 schema/ 下一个文件，
//! 文件名就是 block 名，也是形状的出处。`$block` 在取用时展开，可以指向 Model、Directory、Lang0 与 Kernel 的 block。
//! 测试用的场景预期不在这里：它不该编进 croon-app 的二进制，见 [`scenario`]。
//! 参考：docs/Host/README.md（配置、root entry、构造的推导）。

use croon_eval::blocks::{Block, chain, expanded, find};

pub static BLOCKS: &[Block] = croon_eval::block_files![
    "host.config",
    "host.directory.binding",
    "host.directory.result",
    "host.root",
    "home.tools",
    "definition.infer",
];

/// 本 crate 的 block，找不到的沿依赖往下找。
pub fn block(name: &str) -> Option<&'static Block> {
    chain(find(BLOCKS, name), || {
        chain(model::block(name), || {
            chain(directory::block(name), || lang0::block(name))
        })
    })
}

/// 一份 block，`$block` 已展开。
pub fn json(name: &str) -> serde_json::Value {
    expanded(name, &block)
}

/// Delegate 场景的预期，只给测试与 CPython 核对用，不进 [`BLOCKS`]：69KB 的夹具不该随二进制发出去。
/// 它的形状与权威见 docs/Library/Scenarios/Delegate.md。
pub fn scenario(name: &str) -> serde_json::Value {
    static SCENARIOS: &[Block] = croon_eval::block_files!["library.scenario.delegate"];
    let block = find(SCENARIOS, name).unwrap_or_else(|| panic!("没有场景 {name}"));
    expanded(name, &|n| if n == name { Some(block) } else { block_of(n) })
}

fn block_of(name: &str) -> Option<&'static Block> {
    block(name)
}
