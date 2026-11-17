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

**3. 写配置**。两份文件，都很短：

```
cp config/agent-provider.example.json config/agent-provider.json     # 填 base_url、api_key、model
mkdir -p ~/.config/croon
cp config/host.example.json ~/.config/croon/config.json              # 把里面的「/绝对路径」改成真实的
```

`agent-provider.json` 是模型服务的地址、密钥与模型名；`config/agent-provider*.json` 已被 .gitignore 排除，密钥不会进 Git。`~/.config/croon/config.json` 最少只要写模型服务在哪：

```
{"model": {"provider": "/绝对路径/croon-app/config/agent-provider.json", "record": true}}
```

`"record": true` 把每次模型往来录在这次运行的目录里，之后能看到每一步用了多少 token。镜像名、资源上限这些都有默认值，要改才写，见 docs/Host/README.md「配置与会话」。不想用 `~/.config/croon/config.json` 的话，每条命令加 `--config 文件`。

**4. 跑一个任务**

```
mkdir -p /tmp/demo/repo && printf 'def add(a, b):\n    return a - b\n' > /tmp/demo/repo/a.py
target/debug/croon-app run --repo /tmp/demo/repo --out /tmp/demo/out "a.py 里的 add 应当返回两数之和，修好它"
```

运行时每一步在标准错误上打印一行（模型第几次、用了多少 token、在容器里跑了什么命令），第一行告诉你**这次运行的会话名**，`resume` 要用它。最后在标准输出打印它的总结，结果在 `/tmp/demo/out`：整份目录（含 `.git`），**不会**写回你的仓库。要看改了什么：

```
diff -ru /tmp/demo/repo /tmp/demo/out
```

注意总结里的文件链接（`/work/a.py`）是容器里的路径，不是你机器上的。一次 `run` 要调用几次模型，会产生费用。`--quiet` 关掉进度行。每次运行的所有东西（记录、容器的状态、录像）在 `~/.local/state/croon/sessions/<会话名>/`。

**5. 中断了怎么办**。被杀、关机、模型服务断连之后，对同一个会话 `resume`，接着做，不重做已经完成的；任务与目录取自它自己的记录，不用再给：

```
target/debug/croon-app resume --session 会话名 --out /tmp/demo/out2
```

不记得会话名：`croon-app resume`（什么参数都不带）会列出最近的几个。会话名拼错了，它会说「找不到会话」并列出已有的。`--out` 要是还不存在或空的目录。已经结束的运行再 `resume` 得到同样的结果。

**6. 不要模型也能试**：`exec` 直接运行一份 Lang0 源码，不经模型，所以**不需要第 3 步的配置，也不需要密钥**；刚装好、想先确认它能工作，就从这里开始。它也是一次独立的运行，有自己的会话，不会撞上别的：

```
printf 'def main(n: int) -> int:\n    return n + 1\n' > /tmp/demo/m.py
target/debug/croon-app exec --source /tmp/demo/m.py --input '{"n":41}' --out /tmp/demo/o
```

打印 `42`。`--out` 必须给，但这个例子没有目录产物，不会建出 `/tmp/demo/o`。源码里要是问了模型，那一次调用会失败，说这次运行没有配置模型。

## 常见问题

- **`docker ps -a` 里留下很多 `croon-exec-…` 的 Created 容器**：这是设计，不是泄漏。每次文件执行留下两份只占名字的空容器，用来保证一个已经作废的执行不会被迟到的启动复活；它们不运行、几乎不占资源。确认没有运行中的任务之后，`docker rm $(docker ps -aq --filter label=croon.provider)` 可以清掉。见 docs/Directory/README.md「实现」。
- **模型有时先去找 `rg`**：镜像里已经装了。写文件时父目录不存在会失败，模型会自己改正。
- **报错「会话 … 里已经有一次运行」**：你给了一个已经用过的 `--session` 名字。要继续它用 `resume`；要做新的，换一个名字，或者不给 `--session`，它自己取。
- **想自己控制每个目录**（测试、实验脚本这样做）：在配置里写 `system`、`kernel`、`directory.state`、`model.record` 的绝对路径，就不再用会话目录。

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
