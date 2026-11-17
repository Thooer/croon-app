//! 打印 croon 的一份场景 block，`$block` 已展开：`cargo run -p host --example block_shared -- scenario.reconnect`。
//! CPython 核对借它的共享 Value；场景在 croon 的 scenarios crate，只在测试里用。

fn main() {
    let name = std::env::args().nth(1).expect("usage: block_shared NAME");
    println!("{}", serde_json::to_string(&scenarios::blocks::json(&name)).unwrap());
}
