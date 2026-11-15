# 开发规划

全文是打算，按待定读：设计以各部分为准，这里只说 croon-app 在哪个阶段、做完了什么、下一步做什么、怎样算做完。跨部分的实现选择见 croon:docs/Kernel/Engineering.md「选择」，绑某一部分的见各部分的「实现」。各阶段只收选定场景需要解决的问题，必要的代码验证可以穿插进行，不要求全部设计完成才开始实现。

## 范围

第一版只做本地单租户：一台机器，一个系统，一个根。云上多租户第一版不做，接口见 croon:docs/Kernel/README.md「待定」里的「云上多租户」：每个操作带系统 ID，存储按系统分开。

croon-app 拥有模型适配、Directory 与 Docker 文件执行、Library 与 Application；Kernel 的通用契约和 Lang0 的求值器由上游仓库拥有。本仓库只写应用侧的决定，Kernel 与 lang0 的语义见各自的权威文档。

## 生命周期

| 阶段 | 完成判据 | 瓶颈 | 做什么 |
|---|---|---|---|
| 0 设计验证 | 「现在」一节做完的几样 | 设计没被实现撞过 | 不拉人；主仓库全开源；写设计文但不发流量；公开验收标准 |
| 1 最小闭环 | 运行闭环（一次性版本已完成，见「现在」）与「制造闭环」的判据 | 项目死亡率最高 | 不拉人；聚焦这三样 |
| 2 外部验证 | 别人能按 README 跑通 | 从「我能跑」到「别人能跑」 | 找 3 到 5 个验证者；修文档与环境；定扩展协议 |
| 3 第一批贡献 | 有人开始写扩展 | 贡献入口不清楚 | 按 croon:docs/Kernel/README.md「制度机」的边界开官方扩展仓库；切边界清楚的活；定许可证；发生态内容；写扩展的设计手册 |
| 4 生态治理 | 到时再定 | 来的人改变项目性质：社区在改项目 | 写治理文档，划出不可妥协的保证与边界 |
| 5 商业化 | 到时再定 | 全开源下怎么持续：规模起来 | Kernel 永远开源；价值在扩展、托管、支持 |

时机：

- **拉人**：阶段 1 结束前不拉；第一个不是贡献者，是能一起撞实现的人。
- **流量**：跟着阶段走，跑通那天才发第一篇带代码的。
- **扩展仓库**：核心契约和 Bundle/交付形状稳定后再开；提前开会把未收敛的边界交给扩展。
- **公开**：三个仓库都还没公开。公开前补上手路径与许可证，各仓库扫一遍历史里的凭据；公开时只推 main，不用 --all、--mirror、--tags，旧历史留在仓库之外。

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
4. **制造闭环**：按「制造闭环」的三步，先做 Cognition 版：W0 接上 construct，运行中构造另一份 Definition 再实例化、交办、并入。
5. **写回仓库**：以导入时的目录 Bundle 为基准把结果并回用户仓库，冲突时报告；见 Library/Organization.md 的待定。

## 制造闭环

一个主体在运行中造出另一个主体，交办一件事，审过结果再并入。它检验「Definition 与 Charter 是数据」这两个最大的设计赌注，也是三条保证（授权只收窄、修正权独立、可追溯）第一次合在一起被看见。分三步，前一步不通不做后一步：

1. **Cognition 版**：W0 接上 construct，经工具构造一份 Cognition Definition，实例化、交办、并入。代码依赖取自自己 Definition 的完整 Bundle，模型碰不到 ID。不依赖 Kernel 的新功能。
2. **模板与说明**：Library 提供 Charter 生成函数（N 个同样的 Seat、审计者、熔断者）与工具目录「查」里的说明，模型选模板、填参数，不手写整份 JSON。
3. **Charter 版**：同样的流程，造的是 Charter。依赖 Kernel 的终止闭环（门结束时 Collective 与其余成员同次结束）与对 Collective 的 Correct，见 croon:docs/Kernel/Instance.md「待定」。

判据写成失败就说明设计错了的形式：

- 模型写错的 Definition 在构造时被拒，报错带路径，几轮内改对；改不对，说明「模型直接写 Definition」这条路不成立，改由 Library 生成。
- 造出的主体跑完，结果经 Notify 回到发起者；越权的接线被拒；检查器查得出「审计者被执行者控制」这类图性质。
- 全程任一处杀掉宿主进程，重跑得到相同的结果。
- 委托深度先定：W0 造出的主体能否再委托，按 Library/Runtime.md「委托不超过两层」的默认接线写进场景。

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

- **场景**：Library 的场景预期在 Library/Scenarios/，测试拿真的 Kernel 与 Lang0、假的模型与文件执行驱动，逐提交点与逐 provider Atom 在途崩溃比对 Journal；预期由 CPython 独立求出并核对。
- **端到端**：host 的集成测试用真 Docker，需要本机有 croon-work:1 与 python:3.12-slim，都标了 ignore；模型用脚本化或回放的，录像只在临时目录。`smoke` 调用真模型，会产生费用。
- **恢复**：每个提交点前后停下、exec 在途 SIGKILL、宿主故障后修好重跑，重跑结果相同，每个逻辑行动只提交一次。跨 Kernel 目录只比结果与记录序列，同一目录内逐字比。

## 以后的方向

场景需要时再纳入：

- 联网：可用性要它。做法在 croon:docs/Kernel/Reference.md「通道」与待定；第一版不联网。
- 历史管理：执行恢复依赖 Journal 的分叉待定。
- LLM 表现量化：Lang0 的写法与检查行为可运行、任务及判据明确之后。同一场景写两版，先写不受限的 Python、再写 Lang0，每处差异标出归处，不能有悄悄改变语义又不报错的；之后测模型写 Lang0 的正确率、构造检查通过率与修复轮数。
- Provider 的管理面：KV Cache 的管理、推理的控制面。前缀复用现在只能在基础设施一侧做，经 HTTP 够不到；线索见 croon:docs/Kernel/Port.md 的待定。排在靠后。
- 生态入口：经 MCP、A2A 接别的框架与客户端，Croon 作调用方或被调用方。依赖联网与系统边界，排在阶段 2、3；见 croon:docs/Kernel/README.md「待定」里的「系统边界」与 croon:docs/Kernel/Plugin.md「待定」。
- 独立软件交付：是否把一份 Application 连同它的 Definition、资源、界面与 Kernel 运行时一起打包，安装后即可使用；运行权利与凭据由部署时的根接入。内嵌运行时、容器依赖、安装与升级，等首个 Application 的场景明确再定。

参考：README.md；Directory/README.md；Model/README.md；Library/README.md；Library/Runtime.md；Library/Organization.md；Library/Presets.md。
