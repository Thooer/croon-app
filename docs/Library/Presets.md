# 预置

预置给谁、预置的 Charter 规则与 Application：Library 提供哪些成品，装它们时守什么。

## 预置给谁

预置给的是用户，不是某个主体。用户在系统里就是 root entry：它的求值器在系统外，今天是一次性客户端。预置分两类：能力，即 root binding 接上的模型、文件执行、推导，与 Kernel 的 pack、construct、instantiate；内容，即 App 的包与用户磁盘上的家目录，见 Packages.md「两层」。

W0 是用户实例化的、由模型驱动的手，它拿到的一切经接线从用户那里派生，不是继承：委托是显式的，可以收窄、撤销，用户持有对它的 Look 与 Correct。所以 W0 没有特权，用户可以少给它一些，比如不给联网；也可以换成自己写的手，宿主运行哪一个 Definition 由调用方指定，见 ../Host/README.md「运行哪一个 Definition」。共享的、要留存的东西属于用户，不属于 W0。人能做的与 W0 能做的是同一组操作，见 Packages.md「管理器」。

预置的 Charter 与别的 Charter 没有区别：构造一次、反复实例化；检查器、职责分离、熔断这些是 Library 的策略，不是 Kernel 的保证。

## 预置 Charter 的规则

- **预置 Charter 就是普通的 Charter Definition**：构造一次，反复实例化。哪些做法由 pin 固定、哪些留给 Principal 选择，按 croon:docs/Kernel/Definition/README.md「Seat」。Principal 只给 instantiate、不给 construct，Agent 也能按审过的结构组建团队。N 个同样的 Seat 由 Library 生成，Kernel 只看展开以后的。
- **检查器**：在实例化之前检查 Charter 的图性质，如审计者能否被执行者控制、敏感权利是否分开、全部往来是否经过指定成员、是否只增加身份而没有分权或监督。检查是 Charter 数据上的纯函数，查哪些性质由 Principal 选择。
- **独立检验有两方面**：一是检验者能否独立观察、纠正，二是方法能否发现执行者漏掉的问题。前者依赖身份与权利，后者依赖 Competence、模型、材料与上下文。
  - **观察与纠正**：检验者在另一个 Instance，持有执行者的 Look，直接读 Journal 与能读的状态；需要纠正时再持有对应的 Correct。检验者占 Charter 的 Seat，或由被检验方之外的 Principal 拥有；被检验者的工具与其委托、可随意纠正的 Agent 不能承担独立监督。
  - **方法与判断**：按任务配置不同的模型、材料或上下文。一个 Individual 里也能组织多种判断，但它们没有独立的权利边界，分界见 croon:docs/Kernel/Definition/README.md「Cognition 与 Charter」。需要强制模型不同的条件仍见那里的待定。
- **检验者的推理能力不低于被检验者**：这是 Competence，由选人的 Principal 判断。
- **判断的独立性靠异质**：同一模型的实例可能犯相似的错，引擎不同或上下文不同不必同时满足，引擎的影响更大。同质的多数共识有失败的下限，在都对才算对的任务上可能比单个更差。用加权，不用简单多数。
- **聚合由一个成员执行**：聚合规则固定到预置 Charter 时，用 pin 固定承担它的 Definition。任务能分解成前提时，先对前提聚合，再推结论。
- **生成不超过检验的吞吐**，否则漏过的错误会逐级放大。
- **熔断者**保管执行者的 Correct，按选定的异常规则决定何时暂停。它的 Required port 的 Port spec 只收暂停；暂停后默认不解除。要逐级降权，这份 Port spec 在暂停之外再收 withdraw，withdraw 的 port 用 enum 列出可收回的 Required port，于是可以先收回危险的接线，再暂停；resume 仍不在其中。收窄写在熔断者的 Required port 上，派生时接上这一节，由每次交出的合取强制。
- **登记者**保管 instantiate、产生出的 Instance 的 Control 与名册。在 Charter 里占写死的 Seat，Collective 的 instantiate 只接给它。成员要产生 Instance，就向它交出请求，附带回话途径；它按写死的规则实例化，把收窄过的几份交回请求者，收窄写在请求 Port 的回话途径的 Port spec 里，并在名册里记下谁请求了什么。Collective 的 Principal 经它导出的 Port 够到这些 Instance，按规则介入，如只在请求者被暂停时。所以成员被说动、被暂停，它产生的东西不会没人管。名册可变，不算改结构。判定可以按关系元组写，也可以按规则写。同样的做法用在一个 Agent 的 instantiate 上，就是让 Principal 够到它委托出去的 Agent。
- **接管者**保管 Correct，按写死的条件放出，如下层停摆、失职。
- **外部服务的 agent 只给 Provided port**，拿不到它的 Control，检验它只能看输出。关键的检验交给持有 Control 的内部成员，或用不同方法检验多个外部 agent 的结果。
- 反模式：自检代替监督；监督者由被监督者任命或控制；用相同模型的数量代替不同方法；没有熔断；把串行的依赖当成可以并行。

## Application

Application（应用）：随框架预置的几类完整系统，即一组 Definition、预置的 Charter、接线与提示词。一份 Application 就是若干已知 Definition 的组合，按内容寻址存在内容库里；实例化它的一方拿到它的 Port 与 Control。Library 是零件，Application 是装好的成品。下面按实现的先后排列。

- **W0**：用户伸向 App 的手，也是根的第一个 Agent，宿主不指定时运行的 Definition。它是一个 ReAct，自带的只有管理器；文件与命令、改代码的子代理都是 App 的包，由默认清单载入，默认只载少数几个，不把 Definition 都预置成工具。W0 制造并运行新的主体，主体也能制造并运行主体，这是自足的最小闭环：不需要先在外面写好 Definition 再导入。
- **编码与工作流**：制造通用的编码类 Agent 与 AI 工作流。
- **个人生活管理**：LifeOS 类的个人生活管理系统。每天的信息量大且杂乱，塞不进一个上下文，也不能在一个上下文里同时处理所有方面。
- **人与智能体的组织**：人与智能体在同一个系统里运行的组织架构。

## 待定

- 预置哪些 Charter、各自怎样写：评审、表决、熔断监督、群聊、派单、保管者、登记者；封闭与穿透，即 Control 往下够到多深，默认取哪种；判定默认用关系元组还是规则，按什么场景推荐。长驻的成员，写法见 Runtime.md 的待定。检查器默认查哪些性质：谁能改某个成员的状态、依赖者有没有读取的途径，都由接线与附带的声明算得出，见 croon:docs/Kernel/Definition/README.md「刚性」。场景：群聊房间、陪审团、四眼、熔断之后、外包的嵌套。
- Application 每类的具体设计。编码与工作流的场景种子：对抗验证，几个互相独立的验证者各自试图反驳一个结论，不确定时默认「已反驳」，过半未被反驳才成立；循环收敛，反复发现，按键去重，连续 N 轮无新增才停，每个新发现由几个不同视角独立判断；多维度并行审查，每个发现的验证做成流水线，一个维度审完就开始验证，不等别的维度。都是 Lang0 上写出的 Library 函数（旧稿，推断）。没有场景：种子还没写成场景。
- 跨系统的 Application 互联，如个人与组织的两套系统之间：依赖 croon:docs/Kernel/README.md「待定」里的「系统边界」。同一个根下的互联就是 root entry 的接线。没有场景：跨系统要联网。

参考：Packages.md；Runtime.md（交办、工具）；Organization.md（做法怎样变、历史分支与合并、Handoff）。
