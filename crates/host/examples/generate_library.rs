//! 生成 Library 的 Lang0 模块：`cargo run -p host --example generate_library -- library`。

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .expect("usage: generate_library OUTPUT");
    host::library::generate(output);
}
