# 模型服务

模型是宿主 Port：提供者和凭据由根装配，Instance 仅凭 Reference 请求一次生成。模型内容是业务数据，Kernel 只执行授权、交付、结果记录和通道规则。规范化的形状是 `model.generate`，供 Lang0 与 Library 使用。

模型调用属于无世界效果通道，暂停时可丢弃；计费仍可能发生，不能据未记录推断没有费用。Library 决定预算、重试、上下文组织和停止；每次真正重试是新键的行动。

首版模型服务接收文本，不隐式读取交付依赖。需要向模型提供文件时，程序先读取并明确组织为输入；提供者不能借调用者的整个读取范围找材料。

## 宿主适配

`model.generate` 是 Library 与 Kernel 之间稳定使用的 Port 形状；网络协议和凭据属于宿主适配器。`model` crate 提供 OpenAI Chat Completions 协议适配器，读取符合 `model.provider` 的配置文件后构造宿主 Port，再按 root binding 的恢复身份装入 Kernel。Definition 只声明 `model.generate`，不包含 endpoint、模型账户或 API key。其他协议应实现同一 Port，不改变 Library 的 Definition。

配置的形状是 `model.provider`。

配置文件由宿主启动入口显式指定；适配器不从环境变量或当前目录隐式寻找配置。开发机上的本地配置可放在 `config/agent-provider.json`，该文件由仓库的 `.gitignore` 排除。Provider 的错误按「失败」一节分类，不能合成为正常模型结果。

HTTP 适配、推理签名、缓存控制、部分结果和其他协议仍需各自的实际场景确定。当前适配器映射 `model.generate` 的文本消息、函数工具调用和完成状态，不把它声明成所有 Provider 的共同协议。

## 失败

一次模型调用没有成功，按谁能修好它分成两类。判据是 croon:docs/Kernel/Instance.md「执行」里的那一条：能否在 Definition 不变的条件下修好原因。

| 情况 | 归类 |
|---|---|
| 请求内容被拒：HTTP 400、413、422，如上下文过长、工具的 Schema 不合法 | `use` 失败 |
| 偶发：HTTP 429、5xx，或请求自己的时限到了 | `use` 失败 |
| 回复不合协议：不是 JSON、没有 choices、工具参数不是 JSON | `use` 失败 |
| 宿主配置：HTTP 401、403，模型不存在的 404 等其余状态 | 宿主故障 |
| 网络：解析不了地址、连不上、代理拒绝 | 宿主故障 |
| 适配器本地：起不了 curl、写不了临时文件、回复超出适配器上限 | 宿主故障 |

`use` 失败作为结果记下，交还 body：同样的请求重做也不会更好，要不要重试、怎样改请求由 Library 决定，每次真正重试是新键的行动。它的 data 是 `{"status": HTTP 状态或 null, "type": Provider 给的错误类型或 null}`，message 带 Provider 的错误说明。

宿主故障不写业务结果，Kernel 停下这个 Instance，等宿主修好后重开，从固定请求重做；模型调用是无世界效果通道，重做不重复副作用。网络断了不能记成业务失败：那样一次断网就让主体永久失败，而修好网络本不需要换 Definition。

## 录制与回放

录制器与回放器是 `model.generate` 的另外两种宿主实现，把一次真模型运行变成能反复运行的测试。录制器包住任意一个 `model.generate` 的 Port，把每次的请求与结果原样记进录像；回放器只读录像，不连任何 Provider。三者装在同一个恢复身份下，选哪一个是宿主配置的事，Definition 不变。

回放按请求的规范文本严格匹配，条目按逻辑行动领用：一个逻辑行动（Instance 与键）第一次请求某段文本时，领走这段文本下一条未用的条目；同一行动重做时，仍拿它领过的那一条。所以崩溃后的重做不会错位；录制时因重做多录下的条目在回放中用不到，不算错。录像不记 Instance 与键：它们随系统的密钥变化。查不到或用尽，以宿主故障停下，不改去调真模型。宽松匹配会让改过的提示词悄悄拿到旧结果，测试证明的东西就少于以为的；对不上就重录。

录像只含请求与结果，不含凭据与 endpoint。录制器只录完成的结果：Provider 报错是宿主故障，不进录像。

录像的形状是 `model.cassette`。

参考：../Directory/README.md（文件执行）；../Library/Runtime.md（ReAct）；../Host/README.md（装配）。
