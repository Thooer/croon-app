# 文档

本文是 croon-app 的文档入口：这里说明应用由哪些部分构成、依赖什么，以及想问什么去哪里读。Kernel 与 lang0 的语义分别由 croon 与 croon-lang0 的文档负责。

## 构成

| 部分 | 承担 | 依赖 |
|---|---|---|
| Directory | 导入、物化与文件执行；Tree 的领域格式由 croon 定义 | croon 的 Value、Bundle 与交付契约；Tree |
| Model | 模型服务的 Port 形状与宿主适配边界 | croon 的 Value、Reference |
| Library | 包与管理器、认知模式、工具、组织做法与 Application | croon、croon-lang0、Directory、Model |
| Application | 按场景装好的 Definition、Charter、接线与提示词 | Library、Directory、Model |
| Host | 根在系统外的装配：Kernel、Language、宿主 Port、root entry 与客户端，以及 `definition.infer` | croon 的 Kernel 运行时、croon-lang0、Directory、Model、Library |

依赖方向为：croon 是 Kernel 基座，croon-lang0 提供第一种 Language，croon-app 组合 Kernel、Language 与模型和文件执行服务。三个仓库并排放置，path 依赖方向为 croon ← croon-lang0 ← croon-app。

本仓库有三个 crate：`crates/directory` 承担 Directory，`crates/model` 承担 Model 的宿主适配，`crates/host` 承担宿主装配与 Library 生成模块。两个提供者 crate 只依赖 croon 的公开契约，互不依赖；只有 host 依赖 Kernel 运行时。

## 地图

想问什么，去哪里读。每一行的第一列是问题，不是文件名。

| 想问什么 | 读哪里 |
|---|---|
| 怎样测真实模型的表现、任务与检查器、记录什么 | Experiments.md |
| croon-app 是什么、当前做到哪了 | 仓库根的 README.md；进度与下一步见 Plan.md |
| 应用由哪些部分构成、依赖什么 | 本文的「构成」；应用侧的计划见 Plan.md |
| Directory 怎样导入、物化与执行文件 | Directory/README.md |
| Tree 的目录内容形状是什么 | croon:docs/Tree.md「Tree」 |
| 模型服务的 Port 形状、宿主适配与配置 | Model/README.md |
| Library 是什么、包装在哪、怎样看、装、载 | Library/Packages.md |
| Library 怎样运行 ReAct、工具与交办 | Library/Runtime.md |
| 怎样组织多个主体、交接与历史；做法什么时候走动态、什么时候走 Handoff | Library/Organization.md |
| 预置 Charter 与 Application 收什么 | Library/Presets.md |
| 根怎样装起一个系统、运行一个 Definition、取回与采用结果 | Host/README.md |
| 运行中怎样把源码变成一份构造得了的 Definition | Host/README.md 的「构造的推导」；Library/Runtime.md 的「工具」里的「构造并入载入」 |
| 工作区是什么、谁是它的主人、怎样并入 | Library/Runtime.md 的「工作区」 |
| Library 的某个场景应当跑出什么 Journal | Library/Scenarios/ |
| 应用侧的证据、先例与研究线索 | Research/；内容按待核验读 |
| 还没有进入设计的方向与想法 | Roadmap.md；不是设计，不是承诺 |
| Kernel 的保证、Reference、Journal 与 Definition | croon 的对应文档 |
| Lang0 的写法、求值与模块 | croon-lang0 的对应文档 |

## 名词表

| English | 中文 | 正文 | 用法 |
|---|---|---|---|
| Tool | 工具 | 中 | |
| Middleware | 中间层 | 英 | 同一 body 内的函数包装，见 Library/Runtime.md |
| Petname | 本地名字 | 英 | 只对自己有意义的短名字；名字空间里的名字，见 Library/Runtime.md |
| Namespace | 名字空间 | 中 | 主体持有的 Reference 的本地名字表，见 Library/Runtime.md |
| Package | 包 | 中 | 一份带 README 与索引的目录 Bundle，见 Library/Packages.md |
| Library 目录 | 包目录 | 中 | 磁盘上装着所有包与工具清单的目录，在系统里是一个工作区，改动经采用留存，见 Library/Packages.md |
| Skill | 技能 | 中 | 一段教模型怎样组合已有工具的说明，是工具表里的一种 |
| Custodian | 保管者 | 中 | 见 croon:docs/Kernel/Reference.md「持有意味着什么」 |
| Registrar | 登记者 | 中 | |
| Application | 应用 | 英 | |
| W0 | 用户的手，根的第一个 Agent | 英 | App 唯一预置的 Definition，见 Library/Presets.md |
| Directory | 目录 | 英 | 文件执行服务 |
| Model | 模型 | 英 | 宿主模型服务的 Port 形状 |
| Host | 宿主 | 中 | 根在系统外的装配，见 Host/README.md |
| Workspace | 工作区 | 中 | 一个主体当前采用哪份目录 Bundle 的可变位置，见 Library/Runtime.md |

参考：Library/README.md；Library/Packages.md；Directory/README.md；Model/README.md；Host/README.md；Plan.md。
