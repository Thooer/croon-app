//! 打印一份 block，`$block` 已展开：`cargo run -p host --example block -- 名字`。
//! CPython 核对（crates/host/cpython/run_delegate.py）用它读场景的预期。

fn main() {
    let name = std::env::args().nth(1).expect("usage: block NAME");
    let value = if name.starts_with("library.scenario.") {
        host::scenario(&name)
    } else {
        host::schema(&name)
    };
    println!("{}", serde_json::to_string(&value).unwrap());
}
