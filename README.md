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

**需要**：Linux、Docker、Python 3、Rust（stable），一个 OpenAI 兼容的模型服务和它的密钥。三个仓库要并排放在同一个目录下：

```
任意目录/
  croon/          Kernel：https://github.com/Thooer/croon
  croon-lang0/    Lang0：https://github.com/Thooer/croon-lang0
  croon-app/      本仓库
```

**1. 构建**

```
cd croon-app
cargo build
```

**2. 建任务镜像**（主体的命令在里面跑，没有网络；镜像里有 git、python3、pytest）

```
docker build -t croon-work:2 -f crates/directory/images/work.Dockerfile crates/directory/images
docker pull python:3.12-slim          # 采集产物用的可信镜像
```

官方源慢时，建镜像可以换源：`--build-arg ALPINE_MIRROR=https://mirrors.aliyun.com/alpine`。

**3. 写配置**

```
cp config/agent-provider.example.json config/agent-provider.json     # 填 base_url、api_key、model
cp config/host.example.json /某处/host.json                           # 把里面的「/绝对路径」都改掉
```

`config/agent-provider*.json` 已被 .gitignore 排除，密钥不会进 Git。host.json 里的路径都要写绝对路径，kernel 与 state 目录会自己创建。

**4. 跑一个任务**

```
mkdir -p /tmp/demo/repo && printf 'def add(a, b):\n    return a - b\n' > /tmp/demo/repo/a.py
target/debug/croon-app run --config /某处/host.json --repo /tmp/demo/repo --out /tmp/demo/out \
  "a.py 里的 add 应当返回两数之和，修好它"
```

结果在 `/tmp/demo/out`，你的仓库不会被改。一次 `run` 要调用几次模型，会产生费用。中途断了：

```
target/debug/croon-app resume --config /某处/host.json --out /tmp/demo/out2
```

**5. 不要模型也能试**：`exec` 直接运行一份 Lang0 源码，不调用模型。host.json 的 `model` 写成 `{"replay": "/某处/empty.json"}`，`empty.json` 的内容是 `{"entries":[]}`：

```
printf 'def main(n: int) -> int:\n    return n + 1\n' > /tmp/demo/m.py
target/debug/croon-app exec --config /某处/host.json --source /tmp/demo/m.py --input '{"n":41}' --out /tmp/demo/o
```

输出 `42`。

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
