"""工作区：一个主体当前采用哪份目录 Bundle，加上派生候选、把候选并回来的规则。

主人是调用这些函数的 Instance：工作区是 body 携带的 dict，每次移动都是一次执行结果加一次采用，重放即恢复。
规则见 croon-app:docs/Library/Runtime.md「工作区」。
"""
from typing import Any, NotRequired, TypedDict
import croon
from library.directory import Exec

Fork = TypedDict("Fork", {"base": str, "candidate": NotRequired[str]})
Workspace = TypedDict("Workspace", {"current": str, "forks": dict[str, Fork]})

# 目录级三方合并：/work 是当前位置，/mnt/b 是 base，/mnt/c 是 candidate。
# 临时仓库在 /tmp，三棵目录各用自己的索引；git 不收名为 .git 的路径，-f 让被忽略的文件也参与。
# 冲突时把冲突的路径写到 stderr 并以 1 退出；没有冲突时把 /work 从当前树移到结果树，/work/.git 不动。
MERGE = "\n".join([
    "set -e",
    "export GIT_DIR=/tmp/croon-merge GIT_CONFIG_NOSYSTEM=1 HOME=/tmp",
    "export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0='*'",
    "export GIT_AUTHOR_NAME=croon GIT_AUTHOR_EMAIL=croon@localhost GIT_COMMITTER_NAME=croon GIT_COMMITTER_EMAIL=croon@localhost",
    "export GIT_AUTHOR_DATE=1970-01-01T00:00:00Z GIT_COMMITTER_DATE=1970-01-01T00:00:00Z",
    "git init -q",
    "tree() { GIT_INDEX_FILE=/tmp/croon-merge-$2 git --work-tree=\"$1\" add -A -f . && GIT_INDEX_FILE=/tmp/croon-merge-$2 git write-tree; }",
    "B=$(tree /mnt/b base); O=$(tree /work ours); T=$(tree /mnt/c theirs)",
    "CB=$(git commit-tree \"$B\" -m base)",
    "CO=$(git commit-tree \"$O\" -p \"$CB\" -m ours)",
    "CT=$(git commit-tree \"$T\" -p \"$CB\" -m theirs)",
    "if ! R=$(git merge-tree --write-tree --name-only \"$CO\" \"$CT\"); then",
    "  printf '%s\\n' \"$R\" | sed -n '2,/^$/p' | sed '/^$/d' >&2",
    "  exit 1",
    "fi",
    "R=$(printf '%s\\n' \"$R\" | head -n 1)",
    "GIT_INDEX_FILE=/tmp/croon-merge-ours git --work-tree=/work read-tree -m -u \"$O\" \"$R\"",
])


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


def tree(directory: str) -> dict[str, Any]:
    """一份目录 Bundle 的 Tree：名字到 {"file": ID, "x": 可执行}、{"dir": ID} 或 {"link": 目标}。纯读取，不起 exec。"""
    return croon.read_json(croon.read_json(directory)["content"])


def entry(ws: Workspace, path: str) -> dict[str, Any]:
    """当前位置里一个路径的条目；空路径是根目录。"""
    node = {"dir": ws["current"]}
    for part in path.split("/"):
        if part == "" or part == ".":
            continue
        if "dir" not in node:
            raise OSError(f"{path} 的上级不是目录")
        entries = tree(node["dir"])
        if part not in entries:
            raise OSError(f"没有 {path}")
        node = entries[part]
    return node


def read_text_at(ws: Workspace, path: str) -> str:
    """读当前位置里一个文件的文本，不起 exec。"""
    node = entry(ws, path)
    if "file" not in node:
        raise OSError(f"{path} 不是文件")
    return croon.read_text(croon.read_json(node["file"])["content"])


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
    """把一个候选并回当前位置：当前位置没动过就直接采用，动过就按目录三方合并；冲突时当前位置不变。"""
    f = ws["forks"].get(name)
    if f is None:
        raise KeyError(f"没有名为 {name} 的候选")
    if "candidate" not in f:
        raise RuntimeError(f"{name} 还没有交回")
    if ws["current"] == f["base"]:
        ws["current"] = f["candidate"]
        return
    r = execute(exec, ws["current"], ["sh", "-c", MERGE],
                mounts={"b": f["base"], "c": f["candidate"]})
    if r["exit"] != 0:
        raise RuntimeError(f"{name} 与当前位置冲突：{r['stderr']}")
    ws["current"] = r["snapshot"]
