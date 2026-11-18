//! `definition.infer`：宿主的纯计算 Port，从 Definition 草稿的 body 推出 Interface。
//! 它不建立构造事实：construct 会把推出的 Interface 与 Definition 里写的重新核对，这里推错也过不了。
//!
//! 同一个 Port 也答模块目录的问题：`{modules, module?}` 不给 module 时列出目录里的模块与各自 docstring 的第一行，
//! 给了就推那个模块的公开面（block `lang0.surface`）。它们是 Library「看」的材料，见 docs/Library/Packages.md。
//!
//! 调用方要知道的：
//!
//! - 草稿的 `body.modules` 要在本次交付的 dependencies 里，否则读不到模块源码，以 use 失败交回。
//! - 提供 `bound_inputs` 时，结果同时含 `tool_input`，供调用方注册工具。`interface` 始终是完整接口。
//! - 模块目录的 Tree 要在本次交付的 dependencies 里；列出模块时某个模块写错了，它的 description 写诊断的第一行，不拦下整张表。
//! - Lang0 的诊断（`路径:行:列: 说明`）原样作 use 失败交回；装载与资源故障是宿主故障，停下调用者。
//! - 通道是 Discardable：没有世界效果，重做得到同样的结果。

use crate::Code;
use croon_port::{Call, Channel, CheckError, Execution, HostDone, HostError, Port};
use serde_json::{Value as Json, json};

/// `definition.infer` 的宿主实现；恢复身份由装配给出。
pub struct Infer {
    identity: String,
}

impl Infer {
    pub fn new(identity: impl Into<String>) -> Self {
        Self {
            identity: identity.into(),
        }
    }
}

/// 目录里的模块与各自 docstring 的第一行；写错的模块不拦下整张表。
fn listing(modules: &str, read: &dyn croon_port::Read) -> Result<Vec<Json>, CheckError> {
    let mut out = vec![];
    for name in lang0::modules(modules, read)? {
        let description = match lang0::surface(modules, &name, read) {
            Ok(surface) => surface["description"]
                .as_str()
                .and_then(|d| d.lines().next())
                .unwrap_or("")
                .to_owned(),
            Err(CheckError::Rejected(message)) => message.lines().next().unwrap_or("").to_owned(),
            Err(error) => return Err(error),
        };
        out.push(json!({"module": name, "description": description}));
    }
    Ok(out)
}

/// 给模型看的参数。绑定只作用于顶层入参，保留其余 Schema 内容，包括参数描述与嵌套约束。
pub(crate) fn tool_input<'a>(interface: &Json, bound: impl IntoIterator<Item = &'a str>) -> Json {
    let mut input = interface["signature"]["in"].clone();
    for name in bound {
        if let Some(fields) = input.get_mut("properties").and_then(Json::as_object_mut) {
            fields.remove(name);
        }
        if let Some(required) = input.get_mut("required").and_then(Json::as_array_mut) {
            required.retain(|field| field != name);
        }
    }
    input
}

fn success(interface: Json, bound: Option<&Vec<Json>>) -> Json {
    let mut value = json!({});
    if let Some(names) = bound {
        value["tool_input"] = tool_input(&interface, names.iter().filter_map(Json::as_str));
    }
    value["interface"] = interface;
    json!({"ok": true, "value": value, "dependencies": []})
}

fn rejected(message: String) -> Json {
    json!({"ok": false, "error": {"kind": "use", "message": message, "data": null}, "dependencies": []})
}

impl Port for Infer {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, call: &Call) -> Result<Box<dyn Execution>, HostError> {
        let read = Code(&*call.content);
        let outcome = if let Some(modules) = call.value["modules"].as_str() {
            match call.value["module"].as_str() {
                Some(name) => match lang0::surface(modules, name, &read) {
                    Ok(surface) => json!({"ok": true, "value": {"surface": surface}, "dependencies": []}),
                    Err(CheckError::Rejected(message)) => rejected(message),
                    Err(CheckError::Host(error)) => return Err(error),
                },
                None => match listing(modules, &read) {
                    Ok(list) => json!({"ok": true, "value": {"modules": list}, "dependencies": []}),
                    Err(CheckError::Rejected(message)) => rejected(message),
                    Err(CheckError::Host(error)) => return Err(error),
                },
            }
        } else {
            let draft = json!({"language": call.value["language"], "body": call.value["body"]});
            match lang0::interface(&draft, &read) {
                Ok(interface) => success(interface, call.value["bound_inputs"].as_array()),
                Err(CheckError::Rejected(message)) => rejected(message),
                Err(CheckError::Host(error)) => return Err(error),
            }
        };
        Ok(Box::new(Ready(HostDone { outcome })))
    }
    fn retire(&self, _attempt: &str) -> Result<(), HostError> {
        Ok(())
    }
}

struct Ready(HostDone);

impl Execution for Ready {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        Ok(Some(self.0.clone()))
    }
}
