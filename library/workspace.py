"""工作区：一个主体当前采用哪份目录 Bundle，加上派生候选、把候选并回来的规则。

主人是调用这些函数的 Instance：工作区是 body 携带的 dict，每次移动都是一次执行结果加一次采用，重放即恢复。
规则见 croon-app:docs/Library/Runtime.md「工作区」。
"""
from typing import NotRequired, TypedDict
import croon
from library.directory import Exec

Fork = TypedDict("Fork", {"base": str, "candidate": NotRequired[str]})
Workspace = TypedDict("Workspace", {"current": str, "forks": dict[str, Fork]})


def workspace(snapshot: str) -> Workspace:
    """从一份目录 Bundle 开一个私有工作区。"""
    return {"current": snapshot, "forks": {}}


def execute(exec: Exec, snapshot: str, argv: list[str], stdin: str = "",
            mounts: dict[str, str] = {}, timeout: int = 60):
    """从一份显式快照执行一次，不采用结果。"""
    return croon.send(exec, {"snapshot": snapshot, "mounts": mounts, "argv": argv,
                             "stdin": stdin, "timeout": timeout},
                      dependencies=_dependencies(snapshot, mounts)).value


def _dependencies(snapshot: str, mounts: dict[str, str]) -> list[str]:
    values = [snapshot]
    for value in mounts.values():
        if value not in values:
            values.append(value)
    return sorted(values)


def advance(ws: Workspace, exec: Exec, argv: list[str], stdin: str = "",
            mounts: dict[str, str] = {}, timeout: int = 60):
    """从当前位置执行一次，把当前位置推进到交回的快照；命令失败也推进。"""
    r = execute(exec, ws["current"], argv, stdin, mounts, timeout)
    ws["current"] = r["snapshot"]
    return r


def fork(ws: Workspace, name: str) -> str:
    """在当前位置登记一个候选位，记下 base，返回实际用的名字；名字重复时加序号，不覆盖已有的。"""
    unique = name
    n = 2
    while unique in ws["forks"]:
        unique = f"{name}{n}"
        n += 1
    ws["forks"][unique] = {"base": ws["current"]}
    return unique


def offer(ws: Workspace, name: str, candidate: str):
    """记下一个候选位交回的快照；不移动当前位置。"""
    ws["forks"][name]["candidate"] = candidate


def adopt(ws: Workspace, exec: Exec, name: str):
    """把一个候选并回当前位置：当前位置没动过就直接采用，动过就三方合并；合并失败时当前位置不变。"""
    f = ws["forks"].get(name)
    if f is None:
        raise KeyError(f"没有名为 {name} 的候选")
    if "candidate" not in f:
        raise RuntimeError(f"{name} 还没有交回")
    if ws["current"] == f["base"]:
        ws["current"] = f["candidate"]
        return
    r = execute(exec, ws["current"],
                ["sh", "-c", "cd /work && git fetch /mnt/s HEAD && git merge --no-edit FETCH_HEAD"],
                mounts={"s": f["candidate"]})
    if r["exit"] != 0:
        raise RuntimeError(r["stderr"])
    ws["current"] = r["snapshot"]
