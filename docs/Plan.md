# 开发规划

全文是打算，按待定读：设计以各部分为准，这里只说 croon-app 在哪个阶段、做完了什么、下一步做什么、怎样算做完。跨部分的实现选择见 croon:docs/Kernel/Engineering.md「选择」，绑某一部分的见各部分的「实现」。各阶段只收选定场景需要解决的问题，必要的代码验证可以穿插进行，不要求全部设计完成才开始实现。

## 范围

第一版只做本地单租户：一台机器，一个系统，一个根。云上多租户第一版不做，接口见 croon:docs/Kernel/README.md「待定」里的「云上多租户」：每个操作带系统 ID，存储按系统分开。

croon-app 拥有模型适配、Directory 与 Docker 文件执行、Library 与 Application；Kernel 的通用契约和 Lang0 的求值器由上游仓库拥有。本仓库只写应用侧的决定，Kernel 与 lang0 的语义见各自的权威文档。

## 生命周期

| 阶段 | 完成判据 | 瓶颈 | 做什么 |
|---|---|---|---|
| 0 设计验证 | 「现在」一节做完的几样 | 设计没被实现撞过 | 不拉人；主仓库全开源；写设计文但不发流量；公开验收标准 |
| 1 最小闭环 | 通过「内核分离」「运行闭环」「制造闭环」的判据 | 项目死亡率最高 | 不拉人；聚焦这三样 |
| 2 外部验证 | 别人能按 README 跑通 | 从「我能跑」到「别人能跑」 | 找 3 到 5 个验证者；修文档与环境；定扩展协议 |
| 3 第一批贡献 | 有人开始写扩展 | 贡献入口不清楚 | 按「内核分离」的边界开官方扩展仓库；切边界清楚的活；定许可证；发生态内容；写扩展的设计手册 |
| 4 生态治理 | 到时再定 | 来的人改变项目性质：社区在改项目 | 写治理文档，划出不可妥协的保证与边界 |
| 5 商业化 | 到时再定 | 全开源下怎么持续：规模起来 | Kernel 永远开源；价值在扩展、托管、支持 |

## 现在

生命周期的 0。运行闭环的一次性版本已经接上真的外部执行：根经宿主装起系统，交一个任务给 W0，取回结果写进新目录；中断后同一个 Kernel 目录重跑即恢复。已完成：

- **宿主装配**：host.config 装起 Kernel、Lang0、模型与 Docker 文件执行，app 自己的 root Definition，一次性客户端 `croon-app run`，见 Host/README.md。端到端由 host 的 `host`、`delegation` 测试验收，用真 Docker 与脚本化或回放的模型。
- **模型**：OpenAI Chat Completions 适配器；录制与回放按逻辑行动领用；失败按谁能修好分成 use 与宿主故障，见 Model/README.md。真 Provider 由 `smoke` 冒烟。
- **工作区**：私有工作区收拢快照、派生与采用；并入按目录三方合并，不看提交状态，不在 /work 里生成提交，结果确定，见 Library/Runtime.md「工作区」。
- **恢复**：每个提交点前后停下，重跑结果与记录序列相同（`recovery`）；exec 在途时 SIGKILL，旧 attempt 退役、新 attempt 重做（`kill`）；宿主故障时客户端带原因退出，修好后重跑重开接着走（`failure`）。
- **场景**：Delegate 场景归本仓库，root 用 host.root，预期由 CPython 独立求出并逐条核对，见 Library/Scenarios/Delegate.md。

**尚未落地**：没有上手路径，README 不写怎样建任务镜像、写 host.config、跑 `croon-app run`，`config/` 下也没有模板；需要真 Docker 的测试都标了 ignore，没有统一的跑法与前提说明，其中 `smoke` 会调用真模型。exec 的耗时没有实测，每次工具调用都起一整套容器；root entry 只在进程内，没有 socket 与持续对话；W0 还不能在运行中构造新的 Definition；结果只写进新目录，写回用户仓库仍待定。

## 下一步

顺序是推断，前一步的结果会改变后一步。

1. **上手路径**：README 加「运行」一节，写建镜像（含换源参数）、host.config 与 `croon-app run`；提交 `config/` 下的配置模板；写明需要真 Docker 的测试怎样跑、前提是什么、哪些会调用真模型。判据是别人按 README 能跑通一次任务，即阶段 2 的入口。
2. **实测 exec**：用真模型跑几个真实任务，量每次工具调用的耗时与一个任务里 exec 的次数，再决定容器池、增量物化这类优化要不要做、做哪一种。
3. **持续对话**：经 socket 的 root entry，根能对运行中的 W0 连发几句、软打断与硬打断；场景见 croon:docs/Kernel/README.md 的「W0 持续对话」。
4. **制造闭环**：W0 运行中构造另一份 Definition 再实例化，代码依赖取自自己 Definition 的完整 Bundle；判据见「制造闭环」。
5. **写回仓库**：以导入时的目录 Bundle 为基准把结果并回用户仓库，冲突时报告；见 Library/Organization.md 的待定。

## 内核分离

提供者装配与进程内执行收取已分离；状态与执行环境仍有默认绑定。定位、零环境特权、测试与生态边界以 croon:docs/Kernel/README.md「制度机」为准；前三项保留已完成部分的验收范围，第四项是下一刀：

1. **场景与依赖**：先走通不含 LLM 的审批，检验相同的授权、Control、记录与恢复。可先用单 Entry 和固定审批程序验证核心，再由系统边界承接多 Entry，不能把前者当成多人接入已完成。清点 llm、exec、Lang0、Library 及它们的形状、分派和构建依赖，区分提供者语义与 Kernel 必须强制的通道约束。
2. **实现分离**：把具体求值器、服务形状、适配与智能体预置放到提供者或用途一侧；核心保留通用契约、Reference 检查、持久执行与效果通道的强制约束。装配由根显式完成，智能体组合也按普通接线运行。沿既有调用边界切分，不先增加转发层或通用注册系统。
3. **验收与交付**：核心能独立构建、测试，不要求 Lang0、模型 SDK 或 Library；审批与 W0 → S 同时在分离后的核心上成立。独立包与扩展入口按此边界验收。
4. **状态与执行环境**：Bundle 由 Definition 显式携带，目录、容器与执行能力按需要接入。先走两个场景：没有文件状态的固定程序；同一个主体处理两个分别授权的仓库。推清可变状态由谁持有、执行能接触什么、变化由哪本 Journal 解释，以及暂停、重开怎样恢复；沿诞生、Atom 结果、Bundle 产物与 Language、Library 的用法消解默认业务状态。验收：无文件状态时无需目录服务，两份状态可分别授权，各次变化与恢复仍有唯一主人。具体形状随场景收敛。

本轮直接替换旧契约：删除旧 Tree/记录兼容路径，场景和实现按新模型重写。内容寻址、Journal 追加和同键重放仍由保证约束；功能可组合性由场景验收。拆成独立包或发布物以本节验收为准；元循环独立推进。

## 运行闭环

首刀与第二刀已完成，见「现在」；接下来接真实的外部执行。

- **首刀留下的**：只用 Cognition，root entry 在进程内，不含 Charter、联网和 socket；显式 Bundle 场景与 croon:docs/Kernel/Scenarios/README.md「脚手架」留作回归集。
- **接真的**：接模型适配、Directory/Execution 的容器通道与经 socket 的 root entry。内容库、记录库与 Kernel 自己执行的 Port 已由核心实现。是否需要 Charter，由场景决定。
- **完成判据**：根能启动 Agent、交任务、取得结果；中断后按通道语义恢复，已提交的结果可重放，执行产物与采用关系可追溯，未授予的权利不能使用。
- **可能阻塞的**：真实 exec 的执行生命周期与故障后的效果收齐，见 croon:docs/Kernel/Instance.md「执行」与 croon:docs/Kernel/Reference.md「通道」；Language 的系统内构造不是本闭环的前提。冻结的子集与语义不随实现路线改变。

### 第二刀

已完成；以下保留验收的范围与判据。

- **范围**：Kernel 场景只使用 fake Language 和显式 Bundle；Lang0、模型、文件执行与 Library 的 Delegate 集成在对应仓库维护。
- **判据**：源码通过构造检查，实际求值得到预期的交办、等待、结果交付与并入；Journal 与预期相符、满足四条不变量；在每个事务提交前后中断，恢复后得到与不中断时相同的 Journal。预期写成可取用的规格，不由求值器自己的输出充当预期。
- **CPython 差分的范围**：Lang0 仓库中的 CPython 绑定给出相同的假结果，对照结束值与 Atom 先后；覆盖文本、消息、dict 顺序、推导式的求值顺序与整除，已列出的语义差异单独检查。
- **暂不扩展**：并发、生成器、通用 class、with、Checkpoint、Directory 的一等纯函数操作，不因求值器以后可能需要而提前实现；类型声明所需的写法按首版清单定。资源按 croon-lang0:docs/Evaluator.md「资源」的规则计量，验证用的上限只作试验参数，实测后再定冻结的数值，不把临时值当成 `lang0@1` 的发布语义。

## 制造闭环

在运行闭环上接入构造、实例化与 Library 的交办工具。判据：Agent 能构造并运行另一个 Agent，完成授权、交办与结果交付；越权请求被拒，文件改动由接收方按规则并入。

制造闭环之后，「跑通」以下面两条为准；Application，如个人生活管理，排在它们之后。

- **一天内复刻**：市面上任何一个 Agent 框架出现，不到一天就能用 Croon 把它的核心逻辑写出来，并且同样用 Croon 写出它的测试、跑完。复刻的是核心逻辑，不是源码。复刻不出的，就是 croon:docs/Kernel/README.md「运作方式」的缺口。
- **复刻者**：造一个专门制造或复刻别的 Agent 的主体，它把做过的经验存下来，拿到一个 Agent 的源码或文档架构，就能批量复刻。

复刻的对照：别的框架回答过的问题，Croon 都要有一个落点。先做 DSH（DeepSeek Harness）：它用到的技术各回答什么问题，Croon 用什么组合回答；结果进 Research，缺口进 croon:docs/Kernel/README.md「运作方式」。

## Application

Application（应用）：随框架预置的几类完整系统，即一组 Definition、预置的 Charter、接线与提示词。一份 Application 就是若干已知 Definition 的组合，按内容寻址存在内容库里；实例化它的一方拿到它的 Port 与 Control。Library 是零件，Application 是装好的成品。当前按实现的先后收：

- **W0**：根的第一个 Agent，一个 ReAct，工具有文件与命令、构造、交办、工具目录。根经它制造并运行新的主体，主体也能制造并运行主体，这是自足的最小闭环：不需要先在外面写好 Definition 再导入。
- **编码与工作流**：制造通用的编码类 Agent 与 AI 工作流。
- **个人生活管理**：LifeOS 类的个人生活管理系统。每天的信息量大且杂乱，塞不进一个上下文，也不能在一个上下文里同时处理所有方面。
- **人与智能体的组织**：人与智能体在同一个系统里运行的组织架构。

## 测试

- **场景**：首刀的预期在 croon:docs/Kernel/Scenarios/README.md「格式」；脚手架拿假的 Language、llm 与 exec 驱动真的 Kernel，跑完与预期比对，做法见那里的 README.md。第二刀另用真的 Lang0 求值，保留假的 llm 与 exec；判据见「第二刀」。
- **确定性**：假的录下结果再回放；重放的确定性用性质测试；对账者与恢复本身再崩溃一次，结果收敛。
- **崩溃的窗口**各测一次，恢复后得到同样的 Journal，不重复副作用（按通道语义重做的除外）：隔离执行已结束、产物尚未提交；产物已保存、记录事务提交前；记录提交后、结果送回 Language 前；交出提交后、对方取之前。Kernel 一侧与执行在途时崩溃相同，Docker 的 retire 与采集由 Directory/Execution 测试验证。
- **逃逸的用例**（子孙进程、double-fork、setsid）见 Directory/README.md 的隔离与恢复边界。

## 以后的方向

场景需要时再纳入：

- 联网：可用性要它。做法在 croon:docs/Kernel/Reference.md「通道」与待定；第一版不联网。
- 历史管理：执行恢复依赖 Journal 的分叉待定。
- LLM 表现量化：Lang0 的写法与检查行为可运行、任务及判据明确之后。同一场景写两版，先写不受限的 Python、再写 Lang0，每处差异标出归处，不能有悄悄改变语义又不报错的；之后测模型写 Lang0 的正确率、构造检查通过率与修复轮数。
- Provider 的管理面：KV Cache 的管理、推理的控制面。排在靠后。
- 生态入口：经 MCP、A2A 接别的框架与客户端，Croon 作调用方或被调用方。依赖联网与系统边界，排在阶段 2、3；见 croon:docs/Kernel/README.md「待定」里的「系统边界」与 croon:docs/Kernel/Plugin.md「待定」。
- 独立软件交付：是否把一份 Application 连同它的 Definition、资源、界面与 Kernel 运行时一起打包，安装后即可使用；运行权利与凭据由部署时的根接入。内嵌运行时、容器依赖、安装与升级，等首个 Application 的场景明确再定。

参考：README.md；Directory/README.md；Model/README.md；Library/README.md；Library/Runtime.md；Library/Organization.md；Library/Presets.md。
