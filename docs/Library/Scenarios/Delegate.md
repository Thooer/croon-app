# W0 交办给子 Agent

W0 与 S 都用私有工作区持有显式快照，见 ../Runtime.md 的「工作区」。S 产生候选，W0 先比较；基准未变时采用同一个 Bundle，不产生一项复制工作区的执行。

预期的请求与结束值由 `crates/host/cpython/run_delegate.py` 在 CPython 3.12 上独立求出并逐条核对；它借用 croon-lang0 的 CPython 参考实现，见 croon-lang0:docs/README.md「Interface」。Kernel 与 Lang0 是被测实现，模型与文件执行按规格给结果。`$tree: "library"` 指本仓库的 library/ 加上生成模块，由测试装进临时目录。

## 场景

根构造 W0，把家目录 Bundle（Library 的包与工具清单）和初始目录 Bundle 交给 W0。W0 读家目录里的 tools.json，第一次模型调用选择交办 s；这时才从家目录读 s 的源码，经 infer、pack、construct 构造，再实例化，显式交付任务、s 的 Definition 与初始快照。S 写出新的目录 Bundle，W0 取到结束结果后先执行 diff，再由模型选择 merge。W0 的结束值携带最终采用的 Bundle。

每次交出都同时写入 `delivery.value` 和 `delivery.dependencies`；模型结果没有依赖，文件执行从显式快照输入得到产物依赖。收到的 Bundle 不自动成为当前位置，由 W0 的工作区明确采用。预期还检验 W0 在等待期间没有复制工作区、S 的产物只通过结束结果交回，以及每个模型和文件执行 Atom 在提交前崩溃后不重复副作用。

W0 交办用的是同一个 `delegate` 工具：预置的 s 按名字交办，自己造的给源码或路径。本场景只走预置的 s；造的路径由 `crates/host/tests/construct.rs` 覆盖，写错的源码在那里带着 Lang0 的诊断回到模型，造出来再交办与并入由 `agent.rs` 覆盖，纯计算、只用模型与接线出错的主体由 `shapes.rs` 覆盖。

预期是 `library.scenario.delegate`：每个 Instance 的 Journal 与用到的内容。

参考：../Runtime.md（工作区、交办）；../Presets.md（W0）。
