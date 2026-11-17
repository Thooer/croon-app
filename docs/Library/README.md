# Library

Library 是随框架预置的一批 Definition 与做法，用 Lang0 与 Kernel 的 Port 写成。它不是机制：Kernel 对它不做任何特殊处理，它与用户自己构造的 Definition 是同一种东西，可以整份替换，也可以只用一部分。它不产生权利：其中的 Definition 拿到的 Reference 仍只来自接线。谁来装由根决定，装它就是构造加实例化；Application 是它的成品形态。本文件夹是它的内容，分三份，见「地图」。代码是一份目录 Bundle，用它的 Cognition 把它放进 body.modules；模块装载的语义见 croon-lang0:docs/README.md「模块」。源码在仓库根的 library/；agents.py、files.py、react.py、s.py、w0.py、workspace.py 是手写源码，kernel.py、model.py、directory.py、infer.py 是由本仓库生成器产生的模块，不进 Git。

生成模块的职责固定为：library.kernel 只提供 KernelPort 的 Port spec 与 Outcome；library.model 的 Generate 来自 model.generate；library.directory 的 Exec 来自 directory.exec；library.infer 的 Infer 来自 definition.infer。生成源码与库文件一同固定进目录 Bundle，重放不随宿主构建改变类型。

## 地图

- Runtime.md：ReAct、工具、工作区、交办、Middleware——一个主体怎样做事，从外面能在哪三处插入。
- Organization.md：共享与保密、连续的地址、Handoff、历史分支与合并——Principal 怎样组织多个主体，保持连续与历史。
- Presets.md：预置 Charter 的规则与 Application——Library 提供哪些成品，装它们时守什么。
- Scenarios/：Library 的场景检验什么，每个场景一个文件；预期是对应的 block `library.scenario.<名字>`，测试拿它驱动真的 Kernel 与 Lang0。

## 原则

- **框架在任务中隐形，在元操作中显形。** 暴露决策需要的认知概念：任务、交办、整理历史、Handoff、构造、实例化。隐藏运行需要的实现概念：Journal、代际、存储、Handle、Reference、ID；模型面对的只有工具、名字与结果，由做法翻译，见 croon:docs/Kernel/Language.md「插口」。目的是不给模型非必要的认知负担。工具名用自然的说法，如“交办一个任务”。让主体了解工具的用途，在相关的情境下提醒；提醒不是强制。
- **模型看到的名字极简。** 凡是指向运行中拿到的东西，工具给模型的都是一个短名字，不是 Handle、ID 或 UUID：模型自己造出的，用它造的时候给的说法，如交办时写的「调研代码库」；别人交来的，由做法按来路起，如「审稿人1」「消息 3」「来自 W0」，不从内容里拿，交来的内容自称什么都不算，所以冒充不了。重名时做法加序号区分。名字与背后的 Reference 的对应由做法记着，模型写错或编造的名字解析不到。每个工具的返回值都守这一条，包括交办、工具目录的「用」、构造。
- **运行时的灵活性的上限由 Principal 接线决定。** 要不要给一个主体工具目录、构造、实例化，由它的 Principal 决定。没有接上的，它就做不了。运行中改接，由持有 Correct 的一方做。
- **Interface 不交给模型手写。** 要构造 Definition 的主体接上 `definition.infer`，只写 body 的源码；Interface 是 `main` 签名的派生物，手写就是同一个事实的第二份，也要逐字对上推不出全貌的细节。推导只有宿主做得到，见 ../Host/README.md「构造的推导」。
- **从外面只能在三处插入**：Required port（接保管者）、Provided port（交给它）、工具目录（列出可用的）。模型看到什么、记住什么，由做法决定。所以大多数情况下 ReAct 不用改，只往这三处插东西。

## 待定

- 库级提示词与多库组合：一个库要伴随提示词解释自己；不同的库同时引入时，最好能识别出来，并给出结合使用的教程提示词。识别的机制要到什么程度：只识别库的组合，还是也识别库与当前任务的匹配。提示词放成 modules Bundle 里的文件，要 croon-lang0:docs/README.md「待定」；按需给出说明的途径已有，即工具目录的「查」，见 Runtime.md 的「工具」。没有场景。
- 工具返回的运行时引用值怎么起短名字：模型造出的用模型给的说法，别人交来的按来路起，已定；工具返回的介于两者之间：调用是模型发起的，引用却来自提供者。按来路自动起，或让模型在调用时给，都不从内容里拿名字，都冒充不了，差别只在好不好用。没有场景。

参考：Runtime.md；Organization.md；Presets.md
