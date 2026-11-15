# 模型服务

模型是宿主 Port：提供者和凭据由根装配，Instance 仅凭 Reference 请求一次生成。模型内容是业务数据，Kernel 只执行授权、交付、结果记录和通道规则。下面的规范化形状供 Lang0 与 Library 使用。

```json model.generate
{
  "description":"请求一次模型生成；提供者和模型由这份 Reference 决定",
  "in":{
    "type":"object",
    "properties":{
      "system":{"type":"string"},
      "messages":{"type":"array","items":{"$ref":"#/$defs/message"}},
      "tools":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"description":{"type":"string"},"input":{"type":"object"}},"required":["name","description","input"],"additionalProperties":false}},
      "max_output_tokens":{"type":"integer"},"timeout":{"type":"integer"}
    },
    "required":["system","messages","tools"],"additionalProperties":false,
    "$defs":{
      "call":{"type":"object","properties":{"id":{"type":"string"},"name":{"type":"string"},"input":{}},"required":["id","name","input"],"additionalProperties":false},
      "message":{
        "anyOf":[
          {"type":"object","properties":{"role":{"enum":["user"]},"text":{"type":"string"}},"required":["role","text"],"additionalProperties":false},
          {"type":"object","properties":{"role":{"enum":["assistant"]},"text":{"type":"string"},"reasoning":{"anyOf":[{"type":"string"},{"type":"null"}]},"calls":{"type":"array","items":{"$ref":"#/$defs/call"}}},"required":["role","text","reasoning","calls"],"additionalProperties":false},
          {"type":"object","properties":{"role":{"enum":["tool"]},"results":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"name":{"type":"string"},"output":{},"is_error":{"type":"boolean"}},"required":["id","name","output","is_error"],"additionalProperties":false}}},"required":["role","results"],"additionalProperties":false}
        ]
      }
    }
  },
  "attach":{"additionalProperties":false},
  "out":{
    "value":{
      "type":"object",
      "properties":{
        "text":{"type":"string"},"reasoning":{"anyOf":[{"type":"string"},{"type":"null"}]},
        "calls":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"name":{"type":"string"},"input":{}},"required":["id","name","input"],"additionalProperties":false}},
        "finish":{"enum":["stop","length","tool_calls","content_filter","other"]},
        "usage":{"type":"object","properties":{"input":{"anyOf":[{"type":"integer"},{"type":"null"}]},"output":{"anyOf":[{"type":"integer"},{"type":"null"}]}},"required":["input","output"],"additionalProperties":false}
      },
      "required":["text","reasoning","calls","finish","usage"],"additionalProperties":false
    },
    "attach":{"additionalProperties":false}
  }
}
```

模型调用属于无世界效果通道，暂停时可丢弃；计费仍可能发生，不能据未记录推断没有费用。Library 决定预算、重试、上下文组织和停止；每次真正重试是新键的行动。

首版模型服务接收文本，不隐式读取交付依赖。需要向模型提供文件时，程序先读取并明确组织为输入；提供者不能借调用者的整个读取范围找材料。

## 宿主适配

`model.generate` 是 Library 与 Kernel 之间稳定使用的 Port 形状；网络协议和凭据属于宿主适配器。`model` crate 提供 OpenAI Chat Completions 协议适配器，读取符合 `model.provider` 的配置文件后构造宿主 Port，再按 root binding 的恢复身份装入 Kernel。Definition 只声明 `model.generate`，不包含 endpoint、模型账户或 API key。其他协议应实现同一 Port，不改变 Library 的 Definition。

```json model.provider
{
  "type": "object",
  "properties": {
    "protocol": { "enum": ["openai_chat_completions"] },
    "base_url": { "type": "string" },
    "api_key": { "type": "string" },
    "model": { "type": "string" },
    "max_output_tokens": { "type": "integer" },
    "timeout_seconds": { "type": "integer" }
  },
  "required": ["protocol", "base_url", "api_key", "model", "max_output_tokens", "timeout_seconds"],
  "additionalProperties": false
}
```

配置文件由宿主启动入口显式指定；适配器不从环境变量或当前目录隐式寻找配置。开发机上的本地配置可放在 `config/agent-provider.json`，该文件由仓库的 `.gitignore` 排除。真实 Provider 错误作为宿主故障返回，不能合成为正常模型结果。

HTTP 适配、推理签名、缓存控制、部分结果和其他协议仍需各自的实际场景确定。当前适配器映射 `model.generate` 的文本消息、函数工具调用和完成状态，不把它声明成所有 Provider 的共同协议。

## 录制与回放

录制器与回放器是 `model.generate` 的另外两种宿主实现，把一次真模型运行变成能反复运行的测试。录制器包住任意一个 `model.generate` 的 Port，把每次的请求与结果原样记进录像；回放器只读录像，不连任何 Provider。三者装在同一个恢复身份下，选哪一个是宿主配置的事，Definition 不变。

回放按请求的规范文本严格匹配，条目按逻辑行动领用：一个逻辑行动（Instance 与键）第一次请求某段文本时，领走这段文本下一条未用的条目；同一行动重做时，仍拿它领过的那一条。所以崩溃后的重做不会错位；录制时因重做多录下的条目在回放中用不到，不算错。录像不记 Instance 与键：它们随系统的密钥变化。查不到或用尽，以宿主故障停下，不改去调真模型。宽松匹配会让改过的提示词悄悄拿到旧结果，测试证明的东西就少于以为的；对不上就重录。

录像只含请求与结果，不含凭据与 endpoint。录制器只录完成的结果：Provider 报错是宿主故障，不进录像。

```json model.cassette
{
  "description": "录下的 model.generate：每条一个请求与它的结果，按录制的先后",
  "type": "object",
  "properties": {
    "entries": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "request": {"$block": "model.generate#/in"},
          "outcome": {"$block": "definition.outcome"}
        },
        "required": ["request", "outcome"],
        "additionalProperties": false
      }
    }
  },
  "required": ["entries"],
  "additionalProperties": false
}
```

参考：../Directory/README.md（文件执行）；../Library/Runtime.md（ReAct）；../Host/README.md（装配）。
