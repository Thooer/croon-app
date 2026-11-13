# 目录与文件执行

本部分定义 Tree 及使用它的文件执行服务。它依赖 Kernel 的 Value、Bundle 与交付契约，不依赖 Lang0；代码模块装载和文件工具可以共用它。Kernel 不解释文件、路径或目录条目。

## Tree

Tree 的目录内容形状见 croon:docs/Tree.md「Tree」。本部分只定义使用 Tree 的导入、物化与文件执行服务。

导入使用 lstat，符号链接保存目标而不跟随；只接受普通文件、目录、符号链接和可执行位。物化拒绝路径穿越，不沿先前创建的链接写文件；同一目录依赖范围的校验独立于宿主文件系统。

比较、选子目录和合并是不可变内容上的计算。结果尚未写入时只是候选数据；存入内容库用 pack，执行提供者在一次产物中可以批量建立依赖图。git、.git 和被忽略的文件都只是目录内容，是否纳入由调用方明确选择。

## 文件执行

每次请求给出初始快照及只读挂载，全部在这次交付闭包内。独立可写副本挂在 /work，只读输入在 /mnt/<名字>；不继承上次执行留下的可变文件。运行中快照不变，结果只交付新产物。

```json directory.exec
{
  "description":"从显式快照建立独立环境，运行命令，交付稳定产物；不自动采用结果",
  "in":{
    "type":"object",
    "properties":{
      "snapshot":{"type":"string"},
      "mounts":{"type":"object","additionalProperties":{"type":"string"}},
      "argv":{"type":"array","items":{"type":"string"}},
      "stdin":{"type":"string"},
      "timeout":{"type":"integer","description":"秒，正数，至多 600"}
    },
    "required":["snapshot","mounts","argv","stdin","timeout"],"additionalProperties":false
  },
  "attach":{"additionalProperties":false},
  "out":{
    "value":{
      "type":"object",
      "properties":{
        "exit":{"type":"integer"},"stdout":{"type":"string"},"stderr":{"type":"string"},
        "timed_out":{"type":"boolean"},"truncated":{"type":"boolean"},"snapshot":{"type":"string"}
      },
      "required":["exit","stdout","stderr","timed_out","truncated","snapshot"],"additionalProperties":false
    },
    "attach":{"additionalProperties":false}
  }
}
```

argv 非空，timeout 在上界内，挂载名字遵循 Tree 名字规则。非零退出和超时都在业务结果里表达，仍交付产物；无法取得稳定产物是宿主故障，保持原执行身份等待收齐，不伪造无变化。输出字节按装配的上限截断并以 UTF-8 有损转换成文本，truncated 如实报告；截断上限属于服务装配。

snapshot 的 ID 是领域数据，结果 dependencies 明确包含该根。请求里的 snapshot、mounts 若不在本次交付范围，以 denied 拒绝，不能从调用者其余内容补齐。重新执行只依赖固定输入、装配的镜像和参数；已提交结果不重算。

## 隔离与恢复

每次一个隔离环境，镜像根文件系统只读，/work 和临时目录为本次私有，无原始网络、凭据或共享可写挂载。装配明确镜像、平台、CPU、内存、磁盘、进程数和输出上限；缺少能力报宿主故障，不能静默降级。

回收覆盖 PID namespace 或 cgroup 的全部子孙进程，包括 double-fork、setsid。停止全部写者之后才扫描 /work 建立产物；扫描不得跟随链接。临时目录随环境丢弃。跨调用缓存、容器池和增量物化等性能优化先不引入。

提供者按稳定恢复身份和物理 attempt ID 命名环境。start 幂等；retire 封住迟到启动并回收写者；同一 attempt 在确认 retire 后不能复活。Kernel 保留固定接受项，重做使用新 attempt。实际进程的生存与回收由提供者兑现，业务 Journal 和提交由 Kernel 兑现。

## 实现

Linux 导入与物化以不跟随链接的目录 FD 为锚，用 openat(O_NOFOLLOW) 读写子项；lstat 只分类，打开后再检查普通文件。导入前由提供者停止写者；物化目标为空且属本次私有范围。普通链接保留字面目标，容器内解析它时只能接触隔离环境。目录节点、字节和收集 archive 的上限由装配显式给出，耗尽按宿主故障停下。

文件提供者使用显式路径的 Docker CLI 与 Python 标准库 tarfile。镜像必须已在本机，--pull=never；装配固定任务与可信采集镜像、平台及资源；拒绝镜像自带的可写 volume。任务与采集器显式限定 CPU、内存、进程数，临时文件受 tmpfs 限额；stdout、stderr 各自按输出上限截断。/work 和 /tmp 用限额私有 tmpfs，其他输入从本次私有物化目录只读挂入。初始快照只读挂在 /croon-input，再复制进 /work；镜像需提供 /bin/sh、cp 与 sleep；它的 PID 1 只保活环境，命令经 docker exec 在 /work 运行。

宿主 watchdog 在 deadline 发出按固定容器 ID 的 pause，不依赖 Kernel 调用 collect；CLI 故障传回收取路径，保留身份等待修复。宿主进程崩溃由恢复 retire 收尾。命令退出或超时后先确认 docker pause，冻结整个任务 cgroup，再启动独立可信采集容器。采集器只共享任务的 PID namespace，以 SYS_PTRACE 读取 /proc/1/root/work；它使用只读 root、network none、cap-drop ALL，不加入任务 cgroup。可信 Python 先检查节点和字节预算，再流式输出普通文件、目录与符号链接的 tar。archive 以固定大小缓冲直接限额写入私有暂存文件，不保留整份内存副本；采集器退出码为零且按固定 ID 删除后，才原子改名确认 archive 完整。采集超限或失败立即尝试删除采集器，任务仍保持冻结；清理失败保留身份并报宿主故障。archive 完整落盘后用 rm -f 确认所有写者终止；随后 tarfile 流式解析，先检查路径、节点数与文件大小，只接受普通文件、目录、符号链接，拒绝 hardlink 和特殊文件。解析不调用 extract 或 extractall；只在私有空目录内建立已校验条目，父项必须是真目录。全部解析成功后才导入 Bundle，失败保留原 attempt 待收齐；暂停不冒充已终止。

提供者恢复身份与 attempt 派生稳定容器名。同一恢复身份必须绑定相同私有 state 目录、镜像和装配；装配负责防止并行部署借不同目录绕开同一把锁。每次 start 和 retire 共用持久文件锁；启动前先落盘 attempt 目录，同进程重复 start 共用 Execution。重启后旧 attempt 只允许 retire，再用新 attempt 重做。collect 保留已收取结果，重试不会重启命令。

start 的任一步失败也封印原 attempt 并尝试 retire；若 Docker CLI 已不可用，保留接受目录和封印，恢复后继续清理。retire 先持久封印，回收旧采集器与任务容器，并分别建立、核对同名 tombstone 容器。tombstone 没有输入或挂载，root 只读、network none、cap-drop ALL，入口为 /bin/true。它占住名字，使已在途的旧 create 不能在确认退役后放出写者；后续 start、exec、收集与删除使用已固定的实际容器 ID。若间隙被旧 create 抢先占名，retire 报故障，下一次继续回收，不能确认成功。retire 成功后删除输入、archive、解析产物和 stdin，只保留封印与锁。封印与 tombstone 长期保留，每个 attempt 占一份小宿主状态与两份容器元数据；在 Docker 请求无法证明不会迟到的前提下，删除它会失去退役保证。

案例：A 从 R0 启动两次执行，产生 R1、R2；两者可以同时存在，A 决定采用或合并。暂停其中一个 Instance 只让该次执行有限收尾，不会把产物设成别方的当前状态。难改的点是目录依赖准确性、链接隔离和 retire 的物理保证。
