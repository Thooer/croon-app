//! `definition.infer`：宿主的纯计算 Port，从 Definition 草稿的 body 推出 Interface。
//! 它不建立构造事实：construct 会把推出的 Interface 与 Definition 里写的重新核对，这里推错也过不了。
//!
//! 调用方要知道的：
//!
//! - 草稿的 `body.modules` 要在本次交付的 dependencies 里，否则读不到模块源码，以 use 失败交回。
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

fn success(interface: Json) -> Json {
    json!({"ok": true, "value": {"interface": interface}, "dependencies": []})
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
        let draft = json!({"language": call.value["language"], "body": call.value["body"]});
        let outcome = match lang0::interface(&draft, &Code(&*call.content)) {
            Ok(interface) => success(interface),
            Err(CheckError::Rejected(message)) => rejected(message),
            Err(CheckError::Host(error)) => return Err(error),
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
