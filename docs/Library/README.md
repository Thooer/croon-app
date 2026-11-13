# Library

Library 是随框架预置的一批 Definition 与做法，用 Lang0 与 Kernel 的 Port 写成。它不是机制：Kernel 对它不做任何特殊处理，它与用户自己构造的 Definition 是同一种东西，可以整份替换，也可以只用一部分。它不产生权利：其中的 Definition 拿到的 Reference 仍只来自接线。谁来装由根决定，装它就是构造加实例化；Application 是它的成品形态。本文件夹是它的内容，分三份，见「地图」。代码是一份目录 Bundle，用它的 Cognition 把它放进 body.modules，见 ../Lang0/README.md 的「模块」。源码在仓库根的 library/；kernel.py 只从 reference.kernel 生成 KernelPort 与 Outcome；model.py 从 model.generate 生成 Generate，directory.py 从 directory.exec 生成 Exec。这三个模块在构建时生成，不进 Git，Library 目录是 library/ 加上它们。

## 地图

- Runtime.md：ReAct、工具、交办、Middleware——一个 Cognizer 怎样做事，从外面能在哪三处插入。
- Organization.md：共享与保密、连续的地址、Handoff、历史分支与合并——Principal 怎样组织多个 Cognizer，保持连续与历史。
- Presets.md：预置 Charter 的规则与 Application——Library 提供哪些成品，装它们时守什么。

## 原则

- **框架在任务中隐形，在元操作中显形。** 暴露决策需要的认知概念：任务、交办、整理历史、Handoff、构造、实例化。隐藏运行需要的实现概念：Journal、代际、存储、Handle、Reference、ID；模型面对的只有工具、名字与结果，由做法翻译，见 ../Kernel/Language.md 的「插口」。目的是不给模型非必要的认知负担。工具名用自然的说法，如“交办一个任务”。让 Cognizer 了解工具的用途，在相关的情境下提醒；提醒不是强制。
- **模型看到的名字极简。** 凡是指向运行中拿到的东西，工具给模型的都是一个短名字，不是 Handle、ID 或 UUID：模型自己造出的，用它造的时候给的说法，如交办时写的「调研代码库」；别人交来的，由做法按来路起，如「审稿人1」「消息 3」「来自 W0」，不从内容里拿，交来的内容自称什么都不算，所以冒充不了。重名时做法加序号区分。名字与背后的 Reference 的对应由做法记着，模型写错或编造的名字解析不到。每个工具的返回值都守这一条，包括交办、工具目录的「用」、构造。
- **运行时的灵活性的上限由 Principal 接线决定。** 要不要给一个 Cognizer 工具目录、构造、实例化，由它的 Principal 决定。没有接上的，它就做不了。运行中改接，由持有 Correct 的一方做。
- **从外面只能在三处插入**：Required port（接保管者）、Provided port（交给它）、工具目录（列出可用的）。模型看到什么、记住什么，由做法决定。所以大多数情况下 ReAct 不用改，只往这三处插东西。

## 待定

- 库级提示词与多库组合：一个库要伴随提示词解释自己；不同的库同时引入时，最好能识别出来，并给出结合使用的教程提示词。识别的机制要到什么程度：只识别库的组合，还是也识别库与当前任务的匹配。提示词放成 modules Bundle 里的文件，要 ../Lang0/README.md 的待定「首版子集的清单」；按需给出说明的途径已有，即工具目录的「查」，见 Runtime.md 的「工具」。没有场景。
- 工具返回的运行时引用值怎么起短名字：模型造出的用模型给的说法，别人交来的按来路起，已定；工具返回的介于两者之间：调用是模型发起的，引用却来自提供者。按来路自动起，或让模型在调用时给，都不从内容里拿名字，都冒充不了，差别只在好不好用。没有场景。

参考：（移至仓库 croon/croon-lang0）。
