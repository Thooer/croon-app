# 宿主

宿主是根在系统外的装配：打开 Kernel，装上 Language 与宿主 Port，创建 root entry，再作为 root entry 的客户端在系统内交任务、取结果。根、root entry 与 binding 的语义见 croon:docs/Kernel/README.md「根」与 croon:docs/Kernel/Port.md「根绑定」；本文只写应用一侧的决定。

## 位置

宿主是本仓库里唯一依赖 Kernel 运行时的部分。Directory 与 Model 的提供者只依赖 croon 的公开契约，装进哪个宿主都一样；宿主把它们、Lang0 与 Kernel 组成一个系统。装配本来是根的事，根在应用一侧，所以宿主归本仓库，不归 croon。

宿主自己不进 Journal：配置凭据、装 Language 与 Port、创建 root entry、把结果写到磁盘，都是根在系统外的事。它在系统内做的每一步，都是 root entry 的 Atom。

## 装配

一次运行按顺序做五件事：

1. 打开 Kernel：Language 只装 `lang0@1`；宿主 Port 按恢复身份装上模型（`model.generate`）与文件执行（`directory.exec`）。凭据只在宿主配置与提供者里，不进 Definition、输入或 Journal。
2. 导入：Library 的目录 Bundle、预置 Definition 与用户仓库。用户仓库经 Directory 的导入整份成为目录 Bundle，.git 与被忽略的文件都在内：W0 的合并要从候选里的仓库取提交。只想交出一部分的，先准备好那个目录。
3. 创建 root entry，按「root entry」的 Interface 与 binding 接线。
4. 作为客户端驱动 root entry：构造要运行的 Definition，实例化它并附带 engine 与 exec，向它的 Notify 登记并附带接到 done 的 back，按 via 取结束结果。
5. 结果快照由根物化到一个新的空目录。不覆盖用户的仓库，写回见「待定」。

## 配置

一次运行装配的全部东西写在一份配置文件里，由启动入口显式指定；宿主不从环境变量或当前目录找配置。路径都是绝对路径。

```json host.config
{
  "description": "宿主配置：一个系统的名字、Kernel 目录、模型来源与文件执行的装配",
  "type": "object",
  "properties": {
    "system": {"type": "string", "description": "系统名；恢复身份由它派生"},
    "kernel": {"type": "string", "description": "Kernel 目录：记录库与内容库"},
    "model": {
      "anyOf": [
        {"type": "object", "properties": {"provider": {"type": "string", "description": "符合 model.provider 的配置文件"}}, "required": ["provider"], "additionalProperties": false},
        {"type": "object", "properties": {"provider": {"type": "string"}, "record": {"type": "string", "description": "录像写到这里，不能已经存在"}}, "required": ["provider", "record"], "additionalProperties": false},
        {"type": "object", "properties": {"replay": {"type": "string", "description": "要回放的录像"}}, "required": ["replay"], "additionalProperties": false}
      ]
    },
    "directory": {
      "type": "object",
      "properties": {
        "state": {"type": "string", "description": "提供者的私有 state 目录"},
        "docker": {"type": "string"},
        "python": {"type": "string"},
        "image": {"type": "string", "description": "任务镜像"},
        "collector_image": {"type": "string", "description": "可信采集镜像，要有 python3"},
        "platform": {"type": "string"},
        "cpus": {"type": "string"},
        "memory_bytes": {"type": "integer"},
        "work_bytes": {"type": "integer"},
        "temporary_bytes": {"type": "integer"},
        "pids": {"type": "integer"},
        "output_bytes": {"type": "integer"},
        "nodes": {"type": "integer"},
        "archive_bytes": {"type": "integer"},
        "cleanup_timeout_seconds": {"type": "integer"}
      },
      "required": ["state", "docker", "python", "image", "collector_image", "platform", "cpus", "memory_bytes", "work_bytes", "temporary_bytes", "pids", "output_bytes", "nodes", "archive_bytes", "cleanup_timeout_seconds"],
      "additionalProperties": false
    }
  },
  "required": ["system", "kernel", "model", "directory"],
  "additionalProperties": false
}
```

恢复身份是 `<系统名>/model`、`<系统名>/directory` 与 `<系统名>/infer`，写进 root entry 的 binding。文件执行的容器名与 state 锁由恢复身份派生，所以一台机器上的两个系统要用不同的名字；同一个名字必须始终配同一个 Kernel 目录与 state 目录。录制与回放沿用模型的恢复身份：换录像不换系统名。录制中断后不续录：录像文件已存在时录制器拒绝打开；要重录，换一盘新录像，同时换一个新的系统名与 Kernel 目录。原 Kernel 目录里已提交的结果不会再请求模型，新录像就缺了那几条，之后回放对不上。

## 运行参数

配置文件之外，调用宿主的程序还可以直接给两样运行参数，都可以不给：

- **模型**：一个已经构造好的 `model.generate` 宿主 Port，给了就代替配置里的模型来源。它是 Kernel 档用户接自己的模型实现的入口，也是测试接脚本化模型的入口。它的恢复身份必须是 `<系统名>/model`：恢复身份写进 Journal，同一个 Kernel 目录换了模型实现仍要对得上；对不上时宿主在打开 Kernel 之前就拒绝。
- **故障点**：原样交给 Kernel，在事务提交前后让它停下，用来测试恢复。生产运行不给。

## root entry

| Port | 种类 | 来源 |
|---|---|---|
| pack、construct、instantiate | Required | Kernel |
| engine | Required | 宿主，`model.generate` |
| exec | Required | 宿主，`directory.exec` |
| infer | Required | 宿主，`definition.infer` |
| back | Required | self，接到自己的 done |
| done | Provided | 收 `definition.outcome` |

root entry 接上 `infer`，运行中的主体才能把写好的源码变成构造得了的 Definition：只接 `pack` 与 `construct` 时，模型得手写 Interface。

Definition 是本仓库自己的，不借 croon 场景里的共享 root：

```json host.root
{
  "evaluator": "external",
  "interface": {
    "description": "root entry：根在系统内交任务、取结果",
    "signature": {"in": {}, "out": {}},
    "provides": {
      "done": {"description": "收结束状态", "in": {"$block": "definition.outcome"}, "attach": {"additionalProperties": false}}
    },
    "requires": {
      "pack": {"$block": "reference.kernel#/pack"},
      "construct": {"$block": "reference.kernel#/construct"},
      "instantiate": {"$block": "reference.kernel#/instantiate"},
      "engine": {"$block": "model.generate"},
      "exec": {"$block": "directory.exec"},
      "infer": {"$block": "definition.infer"},
      "back": {"$block": "host.root#/interface/provides/done"}
    }
  }
}
```

Required port 在诞生时写定，运行中再接新东西是 Kernel 的待定，所以 root entry 的 Interface 就是这个系统能用的全部外部能力。第一版只接一个模型；要异质检验时换一份声明多个模型的 root Definition，是增量。root entry 不接自己的 Look，也不收别的到达：它只交任务、等结束。

## 构造的推导

`definition.infer` 从一份 Definition 草稿的 body 推出 Interface：纯计算，没有世界效果，通道是可丢弃的。它不建立构造事实：construct 会把推出的 Interface 与 Definition 里写的按规范文本比对，推错也过不了构造，Kernel 不把宿主的推导当成事实。

```json definition.infer
{
  "description": "从 Definition 草稿的 body 推出 Interface；纯计算，不建立构造事实",
  "in": {
    "type": "object",
    "properties": {
      "language": {"enum": ["lang0@1"]},
      "body": {
        "type": "object",
        "properties": {
          "source": {"type": "string"},
          "modules": {"type": "string", "description": "模块目录的 Tree ID，要在本次交付的依赖里"}
        },
        "required": ["source"],
        "additionalProperties": false
      }
    },
    "required": ["language", "body"],
    "additionalProperties": false
  },
  "attach": {"additionalProperties": false},
  "out": {
    "value": {
      "type": "object",
      "properties": {"interface": {"type": "object"}},
      "required": ["interface"],
      "additionalProperties": false
    },
    "attach": {"additionalProperties": false}
  }
}
```

写好的 Interface 不交给模型手写：它是 `main` 签名的派生物，手写就是同一个事实的第二份，要逐字对上 docstring 的 `Args:`、required 的顺序与 Port spec 的全文，写错一点就构造不了。推导只有宿主做得到，`lang0::interface` 是 Rust 的公开函数，body 里取不到。所以运行中构造主体时，模型只写源码，Interface 由这个 Port 算。

- **读取范围**：草稿的 `modules` 要在本次交付的 dependencies 里，否则读不到模块源码。宿主 Port 只读本次请求闭包，不从调用者别处补齐。
- **失败**：Lang0 的诊断原样交回，带 `路径:行:列` 与「用什么代替」的说明，作为 use 失败回到调用者；装载、资源耗尽这类是宿主故障，停下调用者，不写业务结果。
- **重放**：结果记进 Journal，重做不重算。宿主构建变了，推出的接口也可能变，固进 Library 的生成模块随目录 Bundle 一起定格。

## 客户端

第一版的客户端是宿主进程里的一次性命令：交一个任务，等结束，写出结果，退出。同一个 Kernel 目录里已经有 root entry 时，宿主不再创建，按原键重报同样的几步，这就是恢复；任务或仓库变了，报出的就不是原来的 Atom，是重放分歧。

宿主故障由宿主处置，见 croon:docs/Kernel/Instance.md「执行」：

- **运行中**：只要有 Instance 被宿主停下，这次运行就跑不完，一次性客户端也修不了宿主，所以不再等，带着停止原因报错退出，如模型配置错了。在途的文件执行不用收尾，下次启动时的恢复会退役它的 attempt。
- **再跑一次**：修好原因后重跑同一个 Kernel 目录，就是显式的恢复。宿主装上 Port 之后，重开被停下的 Instance，从 Journal 接着走；原因还在，它会再停一次，再报一次。Kernel 拒绝重开的（如还有没确认的世界效果）照实报出。

业务失败不在此列：它是 Journal 里的事实，重跑只是重放出同样的失败，要重新做就换一个 Kernel 目录。它的执行位置在系统外：每一步用固定的键报出；宿主崩溃后用同一个 Kernel 目录重开，以原键重报，已提交的直接得到结果，不会重做，见 croon:docs/Kernel/Definition/Cognition.md「求值器在系统外」。

## 三档用法

装配路径只有一条，区别在用户提供到哪一层：

| 档 | 用户提供 | 宿主补齐 |
|---|---|---|
| Application | 任务、仓库、模型配置 | 预置的 W0 与 S |
| Library | 自己用 Lang0 写、import Library 模块的 Definition | Library 的目录 Bundle |
| Kernel | 自己的 Definition 或 Charter | 只有 Language 与宿主 Port |

三档用同一份 root entry 与同一套宿主 Port，不同的只是第 2、4 步交给 construct 与 instantiate 的东西。

## 待定

- 写回用户已有的仓库：以导入时的目录 Bundle 为基准三方合并，还是拒绝并报告；见 Library/Organization.md 的待定「root entry 与磁盘上的仓库」。没有场景。
- 经 socket 连上的 root entry 与持续对话，见 Plan.md 的「下一步」。场景：W0 持续对话。
- 多个模型槽位。场景：异质检验。

参考：../Model/README.md（宿主适配、录制与回放）；../Directory/README.md（导入、物化）；../Library/Presets.md（W0）。
