//! 没有配置模型时的占位 `model.generate`：`exec` 不需要模型也能运行；被运行的源码若真问了模型，
//! 那一次调用以 use 失败结束，说明这次运行没有配置模型。它不是宿主故障：配置不会因为重试而变。
//! 参考：docs/Host/README.md（配置与会话）。

use croon_port::{Call, Channel, Execution, HostDone, HostError, Port};
use serde_json::json;

/// 恢复身份与别的模型实现一样是 `<名字>/model`，所以同一个会话之后换成真模型也对得上。
pub struct NoModel {
    identity: String,
}

impl NoModel {
    pub fn new(identity: impl Into<String>) -> Self {
        Self {
            identity: identity.into(),
        }
    }
}

impl Port for NoModel {
    fn id(&self) -> &str {
        &self.identity
    }
    fn channel(&self) -> Channel {
        Channel::Discardable
    }
    fn start(&self, _call: &Call) -> Result<Box<dyn Execution>, HostError> {
        Ok(Box::new(Refused))
    }
    fn retire(&self, _attempt: &str) -> Result<(), HostError> {
        Ok(())
    }
}

struct Refused;

impl Execution for Refused {
    fn collect(&mut self) -> Result<Option<HostDone>, HostError> {
        Ok(Some(HostDone {
            outcome: json!({
                "ok": false,
                "error": {
                    "kind": "use",
                    "message": "这次运行没有配置模型：在配置文件里写 model.provider（见 config/agent-provider.example.json）",
                    "data": {"status": null, "type": "no_model"}
                },
                "dependencies": []
            }),
        }))
    }
}
