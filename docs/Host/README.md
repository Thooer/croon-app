# 宿主

宿主是根在系统外的装配：打开 Kernel，装上 Language 与宿主 Port，创建 root entry，再作为 root entry 的客户端在系统内交任务、取结果。根、root entry 与 binding 的语义见 croon:docs/Kernel/README.md「根」与 croon:docs/Kernel/Port.md「根绑定」；本文只写应用一侧的决定。

## 位置

宿主是本仓库里唯一依赖 Kernel 运行时的部分。Directory 与 Model 的提供者只依赖 croon 的公开契约，装进哪个宿主都一样；宿主把它们、Lang0 与 Kernel 组成一个系统。装配本来是根的事，根在应用一侧，所以宿主归本仓库，不归 croon。

宿主自己不进 Journal：配置凭据、装 Language 与 Port、创建 root entry、把结果写到磁盘，都是根在系统外的事。它在系统内做的每一步，都是 root entry 的 Atom。

## 装配

一次运行按顺序做五件事：

1. 打开 Kernel：Language 只装 `lang0@1`；宿主 Port 按恢复身份装上模型（`model.generate`）与文件执行（`directory.exec`）。凭据只在宿主配置与提供者里，不进 Definition、输入或 Journal。
2. 导入：Library 的目录 Bundle、预置 Definition 与用户仓库。用户仓库经 Directory 的导入成为目录 Bundle；带不带 .git、跳不跳被忽略的文件，由宿主配置写明，宿主不猜。
3. 创建 root entry，按「root entry」的 Interface 与 binding 接线。
4. 作为客户端驱动 root entry：构造要运行的 Definition，实例化它并附带 engine 与 exec，向它的 Notify 登记并附带接到 done 的 back，按 via 取结束结果。
5. 结果快照由根物化到一个新的空目录。不覆盖用户的仓库，写回见「待定」。

## root entry

| Port | 种类 | 来源 |
|---|---|---|
| pack、construct、instantiate | Required | Kernel |
| engine | Required | 宿主，`model.generate` |
| exec | Required | 宿主，`directory.exec` |
| back | Required | self，接到自己的 done |
| done | Provided | 收 `definition.outcome` |

Required port 在诞生时写定，运行中再接新东西是 Kernel 的待定，所以 root entry 的 Interface 就是这个系统能用的全部外部能力。第一版只接一个模型；要异质检验时换一份声明多个模型的 root Definition，是增量。root entry 不接自己的 Look，也不收别的到达：它只交任务、等结束。

## 客户端

第一版的客户端是宿主进程里的一次性命令：交一个任务，等结束，写出结果，退出。它的执行位置在系统外：每一步用固定的键报出；宿主崩溃后用同一个 Kernel 目录重开，以原键重报，已提交的直接得到结果，不会重做，见 croon:docs/Kernel/Definition/Cognition.md「求值器在系统外」。

## 三档用法

装配路径只有一条，区别在用户提供到哪一层：

| 档 | 用户提供 | 宿主补齐 |
|---|---|---|
| Application | 任务、仓库、模型配置 | 预置的 W0 与 S |
| Library | 自己用 Lang0 写、import Library 模块的 Definition | Library 的目录 Bundle |
| Kernel | 自己的 Definition 或 Charter | 只有 Language 与宿主 Port |

三档用同一份 root entry 与同一套宿主 Port，不同的只是第 2、4 步交给 construct 与 instantiate 的东西。

## 待定

- 宿主配置的形状：模型配置的路径、Docker 装配、Kernel 与 state 目录。随宿主的实现写进 schema。
- 写回用户已有的仓库：以导入时的目录 Bundle 为基准三方合并，还是拒绝并报告；见 Library/Organization.md 的待定「root entry 与磁盘上的仓库」。没有场景。
- 经 socket 连上的 root entry 与持续对话，见 Plan.md 的「运行闭环」。场景：W0 持续对话。
- 多个模型槽位。场景：异质检验。

参考：../Model/README.md（宿主适配、录制与回放）；../Directory/README.md（导入、物化）；../Library/Presets.md（W0）。
