# croon-app

`croon-app` 是建立在 Croon Kernel 与 `lang0@1` 之上的应用仓库。它提供模型服务、目录与文件执行、Library、认知模式和预置 Application，把通用 Kernel 组合成可使用的 Agent 系统。

## 内容

- `crates/directory/`：Tree、目录导入、物化和文件执行的领域形状。
- `crates/execution/`：Linux/Docker 隔离执行、产物收集和恢复。
- `library/`：ReAct、文件工具、交办和预置 Application 使用的 Lang0 模块。
- `docs/Directory/`：目录与文件执行。
- `docs/Model/`：模型服务 Port。
- `docs/Library/`：运行、组织、预置 Charter 与 Application。
- `docs/Research/`：应用侧的研究线索。

## 状态

应用组合仍在设计验证阶段。Library 的交办闭环、显式快照与模型/文件执行的组合已有场景；真实模型适配、完整客户端和更多 Application 按各自场景推进。

## 开发

本仓库是独立 Cargo workspace。构建与测试只使用本仓库的 crates 和依赖。Library 中的模块与生成文件由本仓库的构建流程产生，不依赖另一个仓库的源码目录。

文档入口是 `docs/README.md`。具体领域从 `docs/Directory/README.md`、`docs/Model/README.md` 和 `docs/Library/README.md` 进入。
