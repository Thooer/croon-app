# croon-app

`croon-app` 是建立在 Croon Kernel 与 `lang0@1` 之上的应用仓库。它提供模型服务、目录与文件执行、Library、认知模式和预置 Application，把通用 Kernel 组合成可使用的 Agent 系统。

## 内容

- `crates/directory/`：Linux 导入与物化、Docker 隔离执行、产物收集和恢复，即 Directory 的文件执行 Port。
- `crates/model/`：`model.generate` 的宿主适配。
- `crates/host/`：根在系统外的装配、`definition.infer` 宿主 Port、Library 生成模块与端到端测试。
- `docs/Host/`：宿主装配。
- `library/`：ReAct、文件工具、工作区、交办与预置 Application 使用的 Lang0 包。
- `docs/Directory/`：目录与文件执行。
- `docs/Model/`：模型服务 Port。
- `docs/Library/`：包与管理器、运行、组织、预置 Charter 与 Application。
- `docs/Research/`：应用侧的研究线索。

## 状态

应用组合仍在设计验证阶段。一次性的运行闭环已接上真模型与 Docker 文件执行；W0 能在运行中构造 Agent 并交办给它，目前只验证了改文件的主体；真实模型造出可用主体的能力还没有实测。进度与下一步见 docs/Plan.md。

## 开发

本仓库是独立 Cargo workspace。构建与测试只使用本仓库的 crates 和依赖。Library 中的模块与生成文件由本仓库的构建流程产生，不依赖另一个仓库的源码目录。

文档入口是 `docs/README.md`。具体领域从 `docs/Directory/README.md`、`docs/Model/README.md`、`docs/Host/README.md` 和 `docs/Library/README.md` 进入。依赖并排放置的 croon 与 croon-lang0 仓库（Cargo path 依赖）。

读上游按 crate 头部进：`croon-eval`（Value、Schema、Definition 的形状与 Language 契约）、`croon-port`（宿主 Port 与内容交付）、`croon-tree`（Tree）、`kernel`（运行时与宿主义务）、`lang0`（写法、Interface、模块）。上游文档在 croon:docs/README.md 与 croon-lang0:docs/README.md，按那里的「地图」找。
