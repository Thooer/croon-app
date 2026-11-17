# Library

Library 是随 App 交给用户的一组包，用 Lang0 与 Kernel 的 Port 写成，见 Packages.md。它不是机制：Kernel 对它不做任何特殊处理，它与用户自己构造的 Definition 是同一种东西，可以整份替换，也可以只用一部分。它不产生权利：其中的 Definition 拿到的 Reference 仍只来自接线。谁来装由根决定，装它就是构造加实例化；Application 是它的成品形态。本文件夹是它的内容，分四份，见「地图」。

## 地图

- Packages.md：包、家目录与管理器——Library 是什么、装在哪、怎样看、装、载。
- Runtime.md：ReAct、名字空间、工具、工作区、交办、Middleware——一个主体怎样做事，从外面能在哪三处插入。
- Organization.md：共享与保密、连续的地址、Handoff、历史分支与合并——Principal 怎样组织多个主体，保持连续与历史。
- Presets.md：预置 Charter 的规则与 Application——Library 提供哪些成品，装它们时守什么。
- Scenarios/：Library 的场景检验什么，每个场景一个文件；预期是对应的 block `library.scenario.<名字>`，测试拿它驱动真的 Kernel 与 Lang0。

## 原则

- **框架在任务中隐形，在元操作中显形。** 暴露决策需要的认知概念：任务、交办、整理历史、Handoff、构造、实例化。隐藏运行需要的实现概念：Journal、代际、存储、Handle、Reference、ID；模型面对的只有工具、名字与结果，由做法翻译，见 croon:docs/Kernel/Language.md「插口」。目的是不给模型非必要的认知负担。工具名用自然的说法，如“交办一个任务”。让主体了解工具的用途，在相关的情境下提醒；提醒不是强制。
- **模型看到的名字极简。** 凡是指向运行中拿到的东西，工具给模型的都是一个短名字，不是 Handle、ID 或 UUID：模型自己造出的，用它造的时候给的说法，如交办时写的「调研代码库」；别人交来的，由做法按来路起，如「审稿人1」「消息 3」「来自 W0」，不从内容里拿，交来的内容自称什么都不算，所以冒充不了。重名时做法加序号区分。名字与背后的 Reference 的对应由做法记着，模型写错或编造的名字解析不到。每个工具的返回值都守这一条，包括交办与实例化。
- **运行时的灵活性的上限由 Principal 接线决定。** 要不要给一个主体管理器、构造、实例化，由它的 Principal 决定。没有接上的，它就做不了。运行中改接，由持有 Correct 的一方做。
- **Interface 不交给模型手写。** 要构造 Definition 的主体接上 `definition.infer`，只写 body 的源码；Interface 是 `main` 签名的派生物，手写就是同一个事实的第二份，也要逐字对上推不出全貌的细节。推导只有宿主做得到，见 ../Host/README.md「构造的推导」。
- **从外面只能在三处插入**：Required port（接保管者）、Provided port（交给它）、工具表（载入的工具，见 Runtime.md「工具」）。模型看到什么、记住什么，由做法决定。所以大多数情况下 ReAct 不用改，只往这三处插东西。

## 待定

- 工具返回的运行时引用值怎么起短名字：模型造出的用模型给的说法，别人交来的按来路起，已定；工具返回的介于两者之间：调用是模型发起的，引用却来自提供者。按来路自动起，或让模型在调用时给，都不从内容里拿名字，都冒充不了，差别只在好不好用。没有场景。

参考：Packages.md；Runtime.md；Organization.md；Presets.md
