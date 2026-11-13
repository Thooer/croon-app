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

具体 HTTP 适配、推理签名、缓存控制、部分结果和 provider 错误仍需实际接入时定。当前形状供确定的服务装配验收，不能把假的结果当作已完成真实模型接入。

参考：../Library/Runtime.md（ReAct）。
