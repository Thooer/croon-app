# croon-app

croon-app 是一个命令行编码 Agent。它默认运行的主 Agent 叫 W0。W0 可以在拿到任务之后写一段 [Lang0](https://github.com/Thooer/croon-lang0) 程序，给它起个名字，用 `make` 把它做成一个新的 Agent。下一轮，W0 的工具表里就有了这个名字，它可以直接调用，把一部分工作交出去。

W0 造 Agent 用的，正是当初造出 W0 自己的那个操作。启动时，宿主用 [Croon](https://github.com/Thooer/croon) 内核的 `construct` 构造 W0，再把它运行起来；工作中，W0 的 `make` 调用的也是这个 `construct`。因此，写应用的人不必预先决定每个帮手要做什么。W0 本身就跑在这个框架里，却也能在看到任务之后编写帮手，给它接上所需的工具，等它交回结果，再决定是否采用。

下面这次运行记录了 W0 怎样编写帮手，再把修复任务交给它。如果你平时用 Pi，可以先从发货标签包试起，把一个已有的 Croon Agent 注册到自己的会话里。[在 Pi 中使用发货标签包](examples/shipping_labels/README.md)

## 一次运行里发生了什么

我们给 W0 的目录里只有一个 `a.py`，其中的 `add` 把加法写成了减法。这次我们明确要求它把修复交出去，没给帮手起名，也没提供实现：

> 造一个 Agent，交办给它修好 a.py：add 应当返回两数之和。不要自己改 a.py。

W0 先用 `look` 查了两次 Library。第 3 次模型调用时，工具表里还没有 `fix_add_agent`：这是 W0 在这次 `make` 调用里给新 Agent 起的名字。它随名字交给 `make` 的程序如下（省略导入和返回类型 `Out` 的定义，只调整了换行）：

```python
def main(task: str, snapshot: str, engine: Generate, exec: Exec) -> Out:
    """修复交办的代码并交回可审查的目录。"""
    ws = workspace(snapshot)
    summary = react(
        engine, file_tools(exec, ws), [{"role": "user", "text": task}],
        "你负责在 /work 中检查并修复代码。遵循仓库的适用指令，保持改动最小，验证目标行为，最后说明修改和验证结果。"
    )
    return croon.deliver({"summary": summary, "snapshot": ws["current"]}, [ws["current"]])
```

`make` 返回了工具名 `fix_add_agent`，输入只有一个字符串 `task`。到了第 4 次模型调用，这个名字已经在工具表里，W0 直接调用它，交代它只改 `a.py` 并验证正数、负数和零。源码的 `main` 明明有四个参数，W0 在这里却只需说任务：工作区提供 `snapshot`，W0 按声明给帮手接上模型 `engine` 和文件执行 `exec`。帮手用库里的 `file_tools` 和 `react` 完成修复，并没有拿到继续构造 Agent 的权利。

帮手交回目录后，W0 看到的是改动统计：`a.py` 一个文件，一行增加、一行删除。它调用 `merge`，收到「已并入 fix_add_agent」。我们在运行结束后对比了输入目录和结果目录，唯一的差异是：

```diff
 def add(a, b):
-    return a - b
+    return a + b
```

## 三层：App、预置、Library

croon-app 由三层组成，彼此分开。App 负责把系统装进一台真实的机器；预置是随 App 交给你的几件成品；Library 是拼出这些成品的零件。成品知道零件，零件不知道成品，所以你可以只拿零件，拼出自己的东西。

### App

App 是宿主这一层。它在系统外装起内核、lang0、模型服务与文件执行，驱动一次运行，结束时把结果写进新的目录。它的几项承诺，各自落在具体的机制上。

你的目录只被导入，不被写回，结果只出现在你指定的新目录里。每一条命令都在一个新的容器里执行：没有网络，根文件系统只读，丢掉了全部 Linux capabilities，CPU、内存与进程数都有上限。宿主用自己的配置调用模型服务，不把凭据传给 Agent。但任务目录会整份导入，放在里面的密钥文件也会成为输入；`.gitignore` 不是导入时的过滤规则。

运行中断后，`resume` 会从这次运行的 Journal 里取回已完成的步骤。模型的请求和回答还可以另存成录像，供新运行在发出相同请求时取回当时的回答。

### 预置

预置的东西是交给你的，不是交给某个 Agent 的。它们没有任何特权，和你自己写的 Definition 是同一种东西。

W0 是其中最重要的一件。文档里管它叫「用户的手」：你在系统里做事，伸进去的就是它。它是一个 ReAct 循环，自带的只有 `look`（查阅 Library）、`make`（构造并载入 Agent）和 `merge`（并入交回的目录）；文件工具与改代码的帮手 S，都由一份默认清单载入。它的每一份权利都从你那里派生而来，你可以少给，也可以收回。如果你另有想法，也可以写一份自己的 Definition，用 `croon-app exec` 运行它，代替 W0。

S 是 W0 默认带着的帮手。它在一份快照副本里改代码，交回的改动由 W0 审过再并入。

更多的预置还在设计里：评审、表决、熔断监督、登记者这类预置的 Charter，以及三类完整的成品：编码与工作流、个人生活管理、人与 Agent 共处的组织。

### Library

Library 是一组用 lang0 写成的普通包。它在内核眼里没有任何特殊之处，和你自己写的代码是同一种东西，可以整份换掉，也可以只取一部分。

它能放心地自由组合，是因为代码与权利分开。代码是内容，谁都可以 import；权利只来自接线，没有接给它，它就没有。一件工具，就是一段代码加上它握住的几份 Reference，再配一份给模型看的说明，它能做的不会超过它握住的那几份。零件无论怎样拼，拼出来的东西都不会比你接给它的权利更大。在这里，灵活不必拿安全去换。

具体说来：ReAct 只是一个函数，引擎、工具表、系统提示与消息历史都是它的参数。工具可以是体内的一个函数、一个 Port、另一个 Agent（每次调用实例化一次），也可以只是一段教模型怎样组合已有工具的说明。要从外面改变一个 Agent，只有三处可以下手：它需要的 Required port、它提供的 Provided port，以及它的工具表。限流、计量预算、改写请求、换掉引擎，都在这三处完成，ReAct 本身几乎不必改动。工作区与目录的三方合并也是普通的包：派生一份候选，交给别人去改，再合回来。

## 往哪里去

下面这些是目标，不是现状。每一条都写了我们打算怎样判断它做到了没有。

**一天之内，复刻任何一个 Agent 框架的核心逻辑。** 别的框架里写死的东西，循环、委托、工具、监督，在 Croon 里都是可以组合的操作，所以我们相信它们都能被组合出来。判断的标准很具体：一个新框架出现，不到一天，用 Croon 写出它的核心逻辑，连同它的测试一起跑通。复刻不出来的那一处，就是内核缺的东西。这件事我们还没有做过第一次。

**一个专门造 Agent 的 Agent。** 它把每一次造的经验留下来，拿到一个框架的源码或文档，就能成批地复刻。它建立在上一条之上，也需要下一条：经验要能留下来。

**造出来的东西能留下来。** 今天，W0 造出的 Agent 只活在这一次运行里。设计已经定了：它们和改过的 Library 一起作为候选交回，由你像审一次代码改动那样决定是否采用，采用了，才留到下一次运行。这还没有实现。

**更完整的成品。** 编码与工作流之外，是替一个人打理每天纷杂事务的系统，以及人与 Agent 在同一套规则下共事的组织。

## 现在

早期，设计正在被实现检验。

现在能做的：给一个目录和一句任务，W0 读写文件、运行命令，需要时自己造一个 Agent、把活交出去，审过再并入；中断之后接着做；也可以不经模型，直接运行一份 lang0 程序。

检验过的：我们用 gpt-6-astra 与 gpt-6.1-sol 在 14 个小任务上各跑过一两次（纯计算、改文件、调用模型三类；10 个用来调整，4 个留到最后才跑），在最后一版代码上，gpt-6-astra 全部通过。每个任务一两次的样本，只说明这条路走得通，不是通过率，也推不到别的任务和模型。值得一提的是，实验找到的问题没有一个出在模型身上：目录权限过紧、模型服务偶发出错、报错不说用什么代替，每一处都是系统自己的缺口，修的也是系统。做法与记录见 docs/Experiments.md，任务与检查器在 experiments/。

还没有的：造出来的 Agent 与改过的 Library 不会留到下一次运行；任务目录的改动只交回新目录，没有写回原目录的命令；没有交互式的长对话；模型接口只有 OpenAI 兼容的 Chat Completions 一种；只在 Linux、本机、单租户、Docker 上验证过。

进度、已知没修的问题与下一步见 docs/Plan.md；还没进入设计的方向见 docs/Roadmap.md。

## 上手

**运行环境**为 Linux、Docker、Python 3、curl 和 Rust（stable）。将三个仓库并排放在同一个目录下。

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

**3. 运行 W0 或其他需要模型的 Agent 时，配置模型服务**。准备一个兼容 OpenAI Chat Completions 的服务，按下面的示例填写密钥和路径。试用第 6 步的纯计算程序或发货标签包时，可以跳过这一步。

```
mkdir -p ~/.config/croon
install -m 600 config/agent-provider.example.json ~/.config/croon/agent-provider.json  # 填 base_url、api_key、model
cp config/host.example.json ~/.config/croon/config.json  # 把 provider 改成上一个文件的绝对路径
```

这两份配置都放在任务目录之外。`~/.config/croon/config.json` 里的 `provider` 要填实际的绝对路径，不写 `~`。例如，主目录是 `/home/alice` 时：

```
{"model": {"provider": "/home/alice/.config/croon/agent-provider.json", "record": true}}
```

`"record": true` 把每次模型往来录在这次运行的目录里，之后能看到每一步用了多少 token。镜像名、资源上限这些都有默认值，要改才写，见 docs/Host/README.md「配置与会话」。不想用 `~/.config/croon/config.json` 的话，每条命令加 `--config 文件`。

**4. 跑一个任务**。`--repo` 是交给 W0 的目录。它在副本上改，`--out` 指定改完的整份目录放在哪里，原目录不会被写回。

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
mkdir -p /tmp/demo
printf 'def main(n: int) -> int:\n    return n + 1\n' > /tmp/demo/m.py
target/debug/croon-app exec --source /tmp/demo/m.py --input '{"n":41}' --out /tmp/demo/o
```

打印 `42`。`--out` 必须给，但这个例子没有目录产物，不会建出 `/tmp/demo/o`。源码里要是问了模型，那一次调用会失败，说这次运行没有配置模型。

### 从自己的程序调用

从一个计数工具开始。完成上面的构建和镜像准备后，在仓库根运行这两条命令就能看到自己程序要处理的两份 JSON。这个工具只做计算，无需模型配置。

```sh
tool_source=spike/external-tool/tally.py
target/debug/croon-app describe --source "$tool_source" --tool count_passing
target/debug/croon-app exec --source "$tool_source" --json \
  --input '{"scores":[3,9,8],"minimum":8}' --out /tmp/croon-tally-result
```

第一条给出 `name`、`description` 和 `input`。接入 OpenAI 兼容的工具调用时，直接把 `input` 用作函数的 `parameters`。收到对 `count_passing` 的调用后，把模型给的参数交给第二条命令。退出码为 0 时解析整个 stdout，这一次读到的是 `{"count":2,"summary":"达标 2 项"}`。退出码非零时，从 stderr 取诊断。

要注册自己的 Agent，换掉源码路径和注册名即可。`spike/external-tool/call.py` 里的 `bind` 演示了如何把上面两步接进 Python 程序。运行 `python3 -B spike/external-tool/call.py target/debug/croon-app` 可以同时试正常输入和错类型输入。

要写一个免模型的目录工具，可以从发货标签包开始。它随包附带可修改的 JSON 配置，用同一份源码处理不同批次，也能通过本地 MCP 接到 Pi。逐步操作见 [发货标签包](examples/shipping_labels/README.md)。

处理目录的 Agent 也能用同一个 `bind` 注册。上文 W0 编写的源码原样放在 `spike/external-tool/fix_add_agent.py`。用 `describe --source` 查看它的完整接口，可以看到 `task` 和 `snapshot`。下面的调用方在注册和执行时都用 `--snapshot` 指定输入目录，注册给模型的参数就只剩 `task`。这个例子会调用真实模型，先完成上手第 3 步的配置。

```sh
work=$(mktemp -d)
mkdir "$work/input"
printf 'def add(a, b):\n    return a - b\n' > "$work/input/a.py"
python3 -B spike/external-tool/repair.py target/debug/croon-app \
  --snapshot "$work/input" --out "$work/result" \
  '修复 a.py，使 add 返回两个参数的和。验证正数、负数和零。'
diff -u "$work/input/a.py" "$work/result/a.py"
```

`repair.py` 把工具说明打印到 stderr，把帮手的结束值打印到 stdout。退出码为 0 表示程序取得了结束值。要判断修复是否完成，看 `diff` 和你自己的测试。一项改动核对满意后再采用。每次调用需要交回目录的工具时，给 `--out` 一个新路径或空目录。

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

## 内容

| 层 | 在哪里 |
|---|---|
| App | `crates/host/`：装配、命令行、`definition.infer` 与端到端测试；`crates/directory/`：导入、物化与 Docker 隔离执行；`crates/model/`：模型服务的宿主适配、录制与回放 |
| 预置 | `library/w0.py`、`library/s.py`，与默认的工具清单 `home/tools.json` |
| Library | `library/` 下其余的包：`react`、`files`、`workspace`、`manager`、`agents`、`model_call`；说明在 `library/README.md` |
| 文档 | `docs/`：Directory、Model、Host、Library 各一部分，入口是 `docs/README.md` |

## 开发

本仓库是独立的 Cargo workspace，通过路径依赖 `../croon` 与 `../croon-lang0`，所以三个仓库要并排放置。`library/` 下的模块是手写的；`kernel.py`、`model.py`、`directory.py`、`infer.py`、`tools.py` 由宿主在每次运行时按当前的 Port 规格生成，不进 Git。

读上游按 crate 头部进：`croon-eval`（Value、Schema、Definition 的形状与 Language 契约）、`croon-port`（宿主 Port 与内容交付）、`croon-tree`（Tree）、`kernel`（运行时与宿主义务）、`lang0`（写法、Interface、模块）。上游文档在 croon:docs/README.md 与 croon-lang0:docs/README.md，按那里的「地图」找。

## 许可证

Apache License 2.0，全文见 LICENSE。
