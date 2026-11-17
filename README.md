# croon-app

一个让模型在隔离容器里改你的代码、并且**能被打断后接着做**的命令行工具。

```
croon-app run --config host.json --repo 你的仓库 --out 结果目录 "修好 a.py 里的 add"
```

它做三件事：把你的仓库整份交给模型驱动的主体，让它在没有网络的容器里读写文件、跑命令；它觉得需要时，会**自己写一份小程序造出另一个主体，把活交给它**，审过再并入；每一步都记在磁盘上，被杀、关机、模型服务断连之后 `resume` 接着做，不重做已经完成的。你的仓库不会被改，结果放在你指定的新目录里。

它是 [Croon](https://github.com/Thooer/croon) 这套 Agent 系统的第一个应用：Croon 的 Kernel 负责记录与恢复，`lang0` 是主体用的小语言，本仓库把它们接上模型、容器和一组预置的库。**用它不需要懂这些**；后面「内容」里才出现这些词。

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

早期，设计在被实现检验的阶段。**能做的**：

- 给一个仓库和一句任务，一个由模型驱动的主体（W0）读写文件、运行命令；它也能在运行中**自己写一份 Lang0 源码，造出另一个 Agent，把活交给它**，审过改动再并入。
- 运行可以中断：被杀、关机、模型服务断连之后，对同一个 Kernel 目录 `resume`，接着做，不重做已经完成的。
- 直接运行一份 Lang0 源码：`croon-app exec`。

**检验过的**：我们用 gpt-6-astra 与 gpt-6.1-sol 在 14 个小任务上各跑过一两次（纯计算、改文件、调用模型三类；10 个用来调整，4 个留出来最后才跑）：在最后一版代码上，gpt-6-astra 在 10 个调整用的任务上全部通过，留出的 4 个也全部通过。**一个任务一次的样本只能说明这条路走得通，不是通过率，也不能推到别的任务与别的模型。** 做法、判据与每次实验的记录格式见 docs/Experiments.md，任务与检查器在 experiments/。

**还没有的**：

- 造出来的 Agent 与改过的 Library 不会留到下一次运行；任务目录的改动只交回新目录，没有写回原仓库的命令。设计已经定了，见 docs/Plan.md。
- 没有交互式的长对话，一次运行是「一个任务、等结束」。
- 只有一个模型接口（OpenAI 兼容的 Chat Completions）。
- 只在 Linux、本机、单租户、Docker 上验证过。

进度与下一步见 docs/Plan.md；还没进入设计的方向见 docs/Roadmap.md。

## 上手

**最短的说法**：给它一个仓库目录和一句话，它在隔离的容器里改，把改好的**整份目录**放到你指定的新位置，你的仓库不动。你再自己对比、拿走。

**需要**：Linux、Docker、Python 3、curl、Rust（stable），一个 OpenAI 兼容的模型服务和它的密钥。三个仓库要并排放在同一个目录下：

```
任意目录/
  croon/          https://github.com/Thooer/croon
  croon-lang0/    https://github.com/Thooer/croon-lang0
  croon-app/      本仓库
```

**1. 构建**

```
cd croon-app
cargo build
```

**2. 建镜像**。主体的命令在容器里跑，容器没有网络；任务镜像里有 git、python3、pytest、ripgrep。另一个镜像用来在命令结束后采集产物：

```
docker build -t croon-work:2 -f crates/directory/images/work.Dockerfile crates/directory/images
docker pull python:3.12-slim
```

`docker pull` 走的是 Docker Hub，慢的话（我们遇到过 5 分钟）要给 Docker 配 registry mirror，与本项目无关；`docker images python` 能看到是否已经有。建任务镜像时包源慢，可以换：`--build-arg ALPINE_MIRROR=https://mirrors.aliyun.com/alpine`。

**3. 写两份配置**

```
cp config/agent-provider.example.json config/agent-provider.json     # 填 base_url、api_key、model
cp config/host.example.json /某处/host.json                           # 把里面的「/绝对路径」都改成真实的
```

`agent-provider.json` 是模型服务的地址、密钥与模型名。`config/agent-provider*.json` 已被 .gitignore 排除，密钥不会进 Git。

`host.json` 里的字段分两类：

- **这一次运行的**：`system`（名字）、`kernel`、`directory.state`、`model.record`（录像）。**每开一个新任务都要换成新的**，否则会报「这个 Kernel 目录里已经有一次运行」。同一个 `kernel` 目录里只有一次运行。
- **这台机器的**：镜像名、`cpus`、`memory_bytes` 与其余几项上限，保持示例里的值即可；`image` 要与上一步建的镜像同名。

路径都要写绝对路径；`kernel` 与 `state` 目录会自己创建，`record` 的录像文件不能已经存在。

**4. 跑一个任务**

```
mkdir -p /tmp/demo/repo && printf 'def add(a, b):\n    return a - b\n' > /tmp/demo/repo/a.py
target/debug/croon-app run --config /某处/host.json --repo /tmp/demo/repo --out /tmp/demo/out \
  "a.py 里的 add 应当返回两数之和，修好它"
```

大约十几秒到一两分钟，**运行时终端不会输出任何东西**，结束时打印它的总结。结果在 `/tmp/demo/out`，是整份目录（含 `.git`），**不会**写回你的仓库。要看改了什么：

```
diff -ru /tmp/demo/repo /tmp/demo/out
```

注意总结里的文件链接（`/work/a.py`）是容器里的路径，不是你机器上的。一次 `run` 要调用几次模型，会产生费用；`model.record` 指向的录像文件里能看到每次往来和 token 用量。

**5. 中断了怎么办**。被杀、关机、模型服务断连之后，对同一份配置 `resume`，接着做，不重做已经完成的；任务与目录取自它自己的记录，不用再给：

```
target/debug/croon-app resume --config /某处/host.json --out /tmp/demo/out2
```

`--out` 要是还不存在或空的目录。已经结束的运行再 `resume` 得到同样的结果。

**6. 不要模型也能试**：`exec` 直接运行一份 Lang0 源码。它有**自己的** Kernel 目录，所以**要另写一份配置**：把第 3 步的 `host.json` 复制一份 `host-exec.json`，改 `system`、`kernel`、`directory.state` 三项为新的值，`model` 一项改成 `{"replay": "/某处/empty.json"}`（文件内容是 `{"entries":[]}`）。

```
printf 'def main(n: int) -> int:\n    return n + 1\n' > /tmp/demo/m.py
target/debug/croon-app exec --config /某处/host-exec.json --source /tmp/demo/m.py --input '{"n":41}' --out /tmp/demo/o
```

打印 `42`。`--out` 必须给，但这个例子没有目录产物，不会建出 `/tmp/demo/o`。

## 常见问题

- **`docker ps -a` 里留下很多 `croon-exec-…` 的 Created 容器**：这是设计，不是泄漏。每次文件执行留下两份只占名字的空容器，用来保证一个已经作废的执行不会被迟到的启动复活；它们不运行、几乎不占资源。不要手动删除正在运行的任务相关的那几个；确认没有运行中的任务之后，`docker rm $(docker ps -aq --filter label=croon.provider)` 可以清掉。见 docs/Directory/README.md「实现」。
- **模型有时先去找 `rg`**：镜像里已经装了。写文件时父目录不存在会失败，模型会自己改正。
- **进度看不到**：现在没有。运行时可以看录像文件（`model.record`）里的条目数增长，或者 `docker ps` 看有没有容器在跑。
- **报错「这个 Kernel 目录里已经有一次运行」**：你复用了上一次的 `kernel`。要继续它用 `resume`；要新任务换一个新目录与新的 `system`。

## 测试

```
cargo test                                   # 不需要 Docker，不调用模型
cargo test -- --include-ignored              # 另外跑需要 Docker 的（要先建好镜像）
cargo test --test smoke -- --ignored         # 会调用 config/agent-provider.json 里的真模型，产生费用
```

需要 Docker 的测试都标了 `#[ignore]`，不是没跑过：默认跳过，是因为别人的机器上不一定有镜像。`recovery` 与 `kill` 很慢（几分钟），其余几十秒。

## 开发

本仓库是独立的 Cargo workspace，通过路径依赖 `../croon` 与 `../croon-lang0`，所以三个仓库要并排放置。Library 里 `library/` 下的模块是手写的；`kernel.py`、`model.py`、`directory.py`、`infer.py`、`tools.py` 由宿主在每次运行时按当前的 Port 规格生成，不进 Git。

文档入口是 `docs/README.md`。具体领域从 `docs/Directory/README.md`、`docs/Model/README.md`、`docs/Host/README.md` 和 `docs/Library/README.md` 进入。

读上游按 crate 头部进：`croon-eval`（Value、Schema、Definition 的形状与 Language 契约）、`croon-port`（宿主 Port 与内容交付）、`croon-tree`（Tree）、`kernel`（运行时与宿主义务）、`lang0`（写法、Interface、模块）。上游文档在 croon:docs/README.md 与 croon-lang0:docs/README.md，按那里的「地图」找。

## 许可证

Apache License 2.0，全文见 LICENSE。
