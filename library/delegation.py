"""交办的工具：造一个 Agent 并交办，收回结果；有工作区产物时再审阅、并入。

运行与工作区分两层：运行见 library.agents，按目标的 Interface 给输入、接线、收回结束值；
工作区这一层只在目标的 in 或结束值里有 `snapshot` 时才介入，纯计算的主体既拿不到工作区，也不多起一次 exec。
见 croon-app:docs/Library/Runtime.md「交办」与「工作区」。
"""
from typing import Any, Callable
import croon
from croon import Inbox
from library.directory import Exec
from library.kernel import Pack
from library.agents import Done, build, infer_interface, interface_of, run_agent, unsupplied
from library.workspace import Workspace, adopt, entry, execute, fork, offer, read_text_at, tree


def _has(schema: Any, key: str) -> bool:
    """一份 Schema 是对象，且有这个字段。"""
    return isinstance(schema, dict) and key in schema.get("properties", {})


def _source(ws: Workspace, pack: Pack, modules: str, path: str) -> Any:
    """工作区里一个路径的源码与模块目录：文件就用 Library 的模块，目录要有 main.py，其余的并进模块目录。"""
    node = entry(ws, path)
    if "file" in node:
        return [read_text_at(ws, path), modules]
    if "dir" not in node:
        raise OSError(f"{path} 不是文件或目录")
    mine = tree(node["dir"])
    if "main.py" not in mine or "file" not in mine["main.py"]:
        raise OSError(f"{path} 里没有 main.py")
    merged = tree(modules)
    for name in sorted(mine.keys()):
        if name == "main.py":
            continue
        if name in merged:
            raise ValueError(f"{path}/{name} 与 Library 里的同名")
        merged[name] = mine[name]
    roots = []
    for item in merged.values():
        child = item["file"] if "file" in item else item.get("dir")
        if child is not None and child not in roots:
            roots.append(child)
    roots = sorted(roots)
    packed = croon.send(pack, {"content": {"json": merged}, "dependencies": roots}, dependencies=roots).value
    main = croon.read_text(croon.read_json(mine["main.py"]["file"])["content"])
    return [main, packed["id"]]


def delegation_tools(ws: Workspace, home: Workspace, exec: Exec, ports: Any, back: Done,
                     done: Inbox(Done), supply: Any, listed: Any) -> list[Callable]:
    """交办与并入两个工具。

    ws 是仓库的工作区，home 是家目录的；ports 是交办自己要用的 Kernel Port：instantiate、pack、construct、
    infer；supply 是愿意交给被交办主体的 Port，持有不等于全部转交；listed 是清单里常驻的 Agent，名字到它在
    家目录里的源码路径，第一次交办时才构造。
    """
    pack = ports["pack"]
    construct = ports["construct"]
    infer = ports["infer"]
    instantiate = ports["instantiate"]
    agents = {}

    def make(name: str, source: str, mods: str) -> str:
        interface = infer_interface(infer, source, mods)
        missing = unsupplied(interface, supply)
        if missing:
            raise RuntimeError(f"它要 {', '.join(missing)}，交办的一方给不了：先改源码去掉这些 Port")
        unique = name
        n = 2
        while unique in agents:
            unique = f"{name}{n}"
            n += 1
        agents[unique] = build(pack, construct, source, mods, interface)
        return unique

    def delegate(agent: str, input: Any, source: str = "", path: str = "") -> Any:
        """交办一件事给一个 Agent，等它结束，交回它的结束值。

        agent 是它的名字：系统提示里列着的是现成的；自己造的，给 source 或 path，造好记在这个名字下，名字重了
        会加序号，用返回的 agent；之后只给名字就行。它要 engine 与 exec 之外的 Port 时，交办的一方给不了。
        input 里有 snapshot 的，工作区的当前位置由这里填，不用写；结束值带 snapshot 的，一并交回 name 与
        diff，满意了再用 merge 并入。

        Args:
            agent: Agent 的名字。
            input: 它的输入，按它 main 的参数写，不含 snapshot。
            source: Lang0 源码，须有顶层 def main，参数与返回值都标类型；可以 import library 里的模块。
            path: 工作区里的 .py 文件，或含 main.py 的目录，作为它的源码。
        """
        if source != "" and path != "":
            raise ValueError("source 与 path 只给一个")
        name = agent
        modules = home["current"]
        if source != "":
            name = make(agent, source, modules)
        if path != "":
            got = _source(ws, pack, modules, path)
            name = make(agent, got[0], got[1])
        if name not in agents and name in listed:
            name = make(name, read_text_at(home, listed[name]), modules)
        if name not in agents:
            raise ValueError(f"没有名为 {name} 的 Agent")
        definition = agents[name]
        signature = interface_of(definition)["signature"]
        takes = _has(signature["in"], "snapshot")
        gives = _has(signature["out"], "snapshot")
        base = ws["current"]
        value = input
        dependencies = []
        if takes:
            if not isinstance(input, dict):
                raise TypeError("input 要写成对象")
            value = dict(input)
            value["snapshot"] = base
            dependencies = [base]
        label = ""
        if gives:
            label = fork(ws, name)
        got = run_agent(instantiate, supply, back, done, definition, value, dependencies)
        if not gives:
            return {"agent": name, "result": got}
        offer(ws, label, got["snapshot"])
        d = execute(exec, ws["current"], ["git", "diff", "--no-index", "--stat", "/mnt/a", "/mnt/b"],
                    mounts={"a": base, "b": got["snapshot"]})
        rest = {k: v for k, v in got.items() if k != "snapshot"}
        return {"agent": name, "name": label, "result": rest, "diff": d["stdout"]}

    def merge(name: str) -> str:
        """把一次交办交回的产物并入自己的工作区：没动过就直接采用，动过就按目录三方合并，冲突时不动。

        Args:
            name: 交办时返回的 name。
        """
        adopt(ws, exec, name)
        return f"已并入 {name}"

    return [delegate, merge]
