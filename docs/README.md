# 文档

本文是 croon-app 的文档入口：这里说明应用由哪些部分构成、依赖什么，以及想问什么去哪里读。Kernel 与 lang0 的语义分别由 croon 与 croon-lang0 的文档负责。

## 构成

| 部分 | 承担 | 依赖 |
|---|---|---|
| Directory | 文件与目录的领域格式、导入、物化与文件执行 | croon 的 Value、Bundle 与交付契约 |
| Model | 模型服务的 Port 形状与宿主适配边界 | croon 的 Value、Reference |
| Library | 预置 Definition、认知模式、工具目录、组织做法与 Application | croon、croon-lang0、Directory、Model |
| Application | 按场景装好的 Definition、Charter、接线与提示词 | Library、Directory、Model |

依赖方向为：croon 是 Kernel 基座，croon-lang0 提供第一种 Language，croon-app 组合 Kernel、Language 与模型和文件执行服务。三个仓库并排放置，path 依赖方向为 croon ← croon-lang0 ← croon-app。

## 地图

想问什么，去哪里读。每一行的第一列是问题，不是文件名。

| 想问什么 | 读哪里 |
|---|---|
| croon-app 是什么、当前做到哪了 | 仓库根的 README.md；进度与下一步见 Plan.md |
| 应用由哪些部分构成、依赖什么 | 本文的「构成」；应用侧的计划见 Plan.md |
| Directory 怎样导入、物化与执行文件 | Directory/README.md |
| Tree 的目录内容形状是什么 | croon:docs/Tree.md「Tree」 |
| 模型服务的 Port 形状与调用边界 | Model/README.md |
| Library 怎样运行 ReAct、工具与交办 | Library/Runtime.md |
| 怎样组织多个 Cognizer、交接与历史 | Library/Organization.md |
| 预置 Charter 与 Application 收什么 | Library/Presets.md |
| 应用侧的证据、先例与研究线索 | Research/；内容按待核验读 |
| 用户还没成形的想法 | Draft.md；不是设计，AI 只在用户让读时读 |
| Kernel 的保证、Reference、Journal 与 Definition | croon 的对应文档 |
| Lang0 的写法、求值与模块 | croon-lang0 的对应文档 |

## 名词表

| English | 中文 | 正文 | 用法 |
|---|---|---|---|
| Tool | 工具 | 中 | |
| Middleware | 中间层 | 英 | 同一 body 内的函数包装，见 Library/Runtime.md |
| Petname | 本地名字 | 英 | 只对自己有意义的短名字 |
| Custodian | 保管者 | 中 | 见 croon:docs/Kernel/Reference.md「持有意味着什么」 |
| Registrar | 登记者 | 中 | |
| Application | 应用 | 英 | |
| W0 | 根的第一个 Agent | 英 | |
| Directory | 目录 | 英 | 文件执行服务 |
| Model | 模型 | 英 | 宿主模型服务的 Port 形状 |

参考：Library/README.md；Directory/README.md；Model/README.md；Plan.md。
