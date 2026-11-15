# AGENTS

croon-app 仓库的规则。文中的「我」是用户，「你」是 AI agent。

croon-app 是建立在 croon 与 croon-lang0 之上的完整应用：模型适配、Docker 执行、Directory、Library 与 Application。它不承诺 API 稳定——预置与组合随需要演进。

## 位置

| 位置 | 收什么 |
|---|---|
| README.md | croon-app 是什么、定位、当前状态 |
| AGENTS.md | 本文 |
| docs/README.md | 文档入口：部分、依赖方向、地图、名词表 |
| docs/Plan.md | 范围、生命周期、阶段与验收；全文是打算 |
| docs/Directory/README.md | Directory 的文件执行服务 |
| docs/Model/README.md | 模型服务 Port 形状 |
| docs/Host/README.md | 宿主装配：root entry、binding、客户端 |
| docs/Library/README.md | Library 总览：原则、地图与生成模块 |
| docs/Library/Runtime.md | 运行环境、工具调用、交办 |
| docs/Library/Organization.md | 组织与历史 |
| docs/Library/Presets.md | 预置 Charter 与 Application |
| docs/Library/Scenarios/ | Library 场景的预期：每个场景一个文件、一个 json block，schema/ 下是它的展开副本 |
| docs/Research/ | 应用侧研究线索 |
| docs/Draft.md | 用户草稿，不是设计；AI 只在用户让读时读 |

- 置信度由位置决定：正文是已确认的，「待定」是未决的，docs/Plan.md 全文按待定读；没确认的留在讨论里，或写成待定问题。
- 这张表只管写到哪里，不管从哪里读；找读法看 docs/README.md 的「地图」。
- 写代码之前，先把机制写进文档（定义、约束、理由，代码取不取用都行），并让我看懂。
- App 的文档只写 App 自己的决定。Kernel 与 lang0 的语义只引名字，不复述理由：引用来源在 croon 与 croon-lang0 的对应文档中。

## 从哪里读

三个方向的入口。项目内的不复述内容，只说去哪看；上游的两个以 crate 头部与公开 API 为准，文档只补头部指不到的判断。

| 要读什么 | 入口 |
|---|---|
| 本仓库由哪些部分构成、依赖什么 | README.md 的「内容」；docs/README.md 的「构成」与「地图」 |
| 正在改动的那一部分怎么设计 | docs/ 里对应部分的 README.md；未决的写在那份文件末尾的「待定」 |
| 现在做到哪了、下一步做什么 | docs/Plan.md；进度只在那里 |
| 一个场景应当跑出什么 | docs/Library/Scenarios/ 下那个场景的文件 |
| 公开面的义务（实现 Port、写 Language、装宿主） | crates/*/src/lib.rs 的 `//!` 头部；头部是义务的唯一权威 |
| 上游 Kernel 的保证、Reference、Journal、Definition | croon 仓库；按 croon:docs/README.md「地图」里按角色的几行进 |
| 上游 Kernel 的公开面 | `croon-eval`、`croon-port`、`croon-tree` 的 crate 头部；Kernel 运行时看 `kernel` 的头部与它的 tests/host.rs |
| Lang0 的写法、求值与模块 | croon-lang0 仓库；公开面是 `lang0` crate 的头部与 `lang0::interface`、`lang0::library` |

读上游先按 crate 头部进，不要从 docs 正文逐篇读：头部写实现之前必须知道的东西，正文写为什么。为了弄清上游行为而不得不读它的实现，就是上游的文档缺口，在同一次改动里补上那份头部或对应小节。

## 文档与代码

### 唯一出处

- 一个事实只有一处权威。别处要用，引用它的名字，不复写。同一件事写了两处，删掉信息更少的那处。
- 理由跟着决策写：改决策时理由一起改。research/ 只放不绑定某个决策的内容。
- 例子不承担规格：形状只定义一次，例子只展示用法。
- 形状以 `crates/*/schema/*.json` 为权威，代码用 `include_str!` 取用，不手写第二份；内部实现只写在代码里，文档不写。写完代码，检查两边的重叠：形状重复的，改成 schema/ 下的文件；只关乎实现的，从文档删掉；两边都要的，定一处作权威，另一处引用它的名字。
- 测试的预期能写成机器读得懂的规格（如对 Directory 或执行的断言）的，写在文档里，代码读它；写不成的放在代码一侧，不在文档里再写一遍。
- **公开面的义务写在 crate 头部**：一个 crate 的 `//!` 头部写消费者实现它之前必须知道、签名里看不出来的东西——要做出哪些决定、弄错会怎样失败、被保证什么。这些义务以头部为唯一权威，docs 不复述；理由与场景留在 docs，头部末尾只指过去。改 trait 或公开函数的提交，同一次里把头部一起看。头部指路时的目标必须真的含那份理由；指不到就补 docs，不要补一份义务。
- **语义集中**：同一个意思只用一个概念说。一串机制在几处各拼一遍，就是缺一个概念：给它一个名字与一处权威，由别处引用它，或说明它是哪个已有概念的用法。

### 载体

| 载体 | 承担什么 | 代码能否取用 |
|---|---|---|
| 表格 | 枚举、映射、清单 | 否 |
| JSON | schema、接口、注册表 | 能 |
| SQL | 表结构、键、约束 | 能 |
| Mermaid | stateDiagram：状态机；其他图，如时序图 | stateDiagram 能；其他只给人看 |
| 散文 | 判断、理由、权衡 | 否 |

- 载体各管各的：JSON 与 SQL 管形状，状态图管迁移，散文管为什么。状态图的标签是操作的名字，操作的形状在 JSON，条件与理由在散文。图上一条线能从形状推出就不画；散文不复述字段。
- 只画变化慢的东西。
- 代码要用的东西必须写成能取用的载体。
- 自己定义的 JSON 字段、枚举字符串与 `$defs` 名字用 snake_case；JSON Schema 的关键字照标准，ID、路径与用户起的名字不受此限。

### 取用

- 代码取用的 JSON 形状放在 `crates/*/schema/*.json`，用 `include_str!` 编进来。名字用 ASCII 小写字母、数字与点，全部文档内唯一。
- 文档里的 fence block 带名字时（语言标签后跟空格和名字），名字是这份形状的权威标识；schema/ 文件是已展开的副本，两者同步。
- block 只在定义它的地方写一次。另一个 JSON block 要用它的形状，写 `{"$block": "名字"}` 或 `{"$block": "名字#/JSON Pointer"}`；schema/ 文件里已展开，代码直接取用。展开时宿主的 `$ref` 不变；展开出的内容里，`$ref` 所属那份 Schema 的根严格在片段内部的，连同那个根上的 `$defs` 原样保留；其余的按它在原 block 里的意思解开，`$defs` 删掉；与 `$block` 并列的键覆盖展开出的同名键。

### 检查

代码使用的 JSON 形状以 `crates/*/schema/*.json` 为权威，并由各 crate 通过 `include_str!` 编译进来。测试直接验证保证和失败原因；不设文档行数、token、提交钩子或文档引用的自动检查。

### 文件边界

- 在非耦合的位置拆分：改它时必须一起改的放一起。
- 每部分先写成一个文件。在非耦合的位置拆分；文件夹里的 README.md 承担该部分的核心。
- 结构等内容到了再建。
- 正文不插链接；需要定位设计出处时，末尾标「参考：」并列本仓库内相对路径。跨仓库引用写 `croon:docs/Kernel/Port.md「Kernel 的 Port」` 或 `croon-lang0:docs/README.md「模块」`。
- 引用单向：设计之间按 docs/README.md 里的依赖方向引用；research/ 可以引用设计；设计不引用 research/。

### 出处与验证

- 出处只写在 research/。未对照原文的标「待核验」，只作线索，不作结论的依据。research/ 的正文区分研究观察与我们的推断。
- 验证得出的结论写在结论处。验证代码放在 spike/ 下，提交到 main；验证代码不是实现。
- 分诊：Draft.md 的条目与别的 AI 交来的调研、验证、构想，都只当线索，保意图，不保做法。逐条定归宿：按「位置」并进 docs 已有的内容；docs 已有、结构已消解它或没有价值的，删；Draft.md 的条目还不成形的，留着，写一句缺什么。指向策略的研究发现，只查机制有没有给设计者留出空间：有，只进 Research；没有，才是缺口。先给我看每条的归宿与理由，我确认后再动文件；落进 docs 之后，才从原处删掉。分诊覆盖 docs/Draft.md 与 docs/Research/，不直接把它们当设计正文。

### 历史

历史在 Git 里。文档直接改写，不留版本标记或废弃段落。理由变了就改写理由，不保留旧理由。决定之后删掉候选与比较，只留定义、约束、理由。关键决策克制地写为什么；反直觉的地方，即很可能被提出反驳的地方，再写为什么不用别的做法，其余被否的做法不留。

## 上下游

- 上游：croon 与 croon-lang0。提供者只依赖 croon-eval、croon-port、croon-tree；Kernel 运行时只由宿主依赖；scenarios 只在测试里用。
- 下游：无。
- 本仓库拥有：Model、文件执行、Library 与 `library.*` 生成模块、Application。`library/kernel.py`、`library/model.py`、`library/directory.py`、`library/infer.py` 由 `cargo run -p host --example generate_library -- library` 产生，不进 Git。
- 读上游先按 croon:docs/README.md「地图」里按角色的几行，从 crate 头部进。为了弄清上游的行为而不得不读它的实现，就是上游的文档缺口：在同一次改动里补上上游的 crate 头部或对应小节，再继续。
