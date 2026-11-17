"""管理器：看 Library 里有什么，造一个 Agent 载入，并入交回的目录；载入的 Agent 是工具表里的原生工具。

运行见 library.agents：按目标的 Interface 给输入、接线、收回结束值。工作区这一层只在目标的 in 或结束值里有
`snapshot` 时才介入，纯计算的主体既拿不到工作区，也不多起一次 exec。
见 croon-app:docs/Library/Packages.md「管理器」与 Runtime.md「工具」「交办」。
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


def _names(tools: list[Any]) -> list[str]:
    """工具表里已有的名字。"""
    out = []
    for t in tools:
        out.append(t["name"] if isinstance(t, dict) else croon.tool(t).name)
    return out


def manager_tools(ws: Workspace, home: Workspace, exec: Exec, ports: Any, back: Done,
                  done: Inbox(Done), supply: Any, listed: Any, tools: list[Any]) -> list[Callable]:
    """看、造、并入三个工具；清单里的 Agent 当场载入工具表，造出来的也载入。

    ws 是仓库的工作区，home 是 Library 目录的；ports 是自己要用的 Kernel Port：instantiate、pack、construct、
    infer；supply 是愿意交给被交办主体的 Port，持有不等于全部转交；listed 是清单里的 Agent，名字到它在
    Library 目录里的源码路径；tools 是主体的工具表，载入就是往里加一项：{name, description, input, call}。
    """
    pack = ports["pack"]
    construct = ports["construct"]
    infer = ports["infer"]
    instantiate = ports["instantiate"]

    def load(name: str, source: str, mods: str) -> str:
        """构造一个 Agent 并载入工具表，返回它在工具表里的名字。"""
        interface = infer_interface(infer, source, mods)
        missing = unsupplied(interface, supply)
        if missing:
            raise RuntimeError(f"它要 {', '.join(missing)}，交办的一方给不了：先改源码去掉这些 Port")
        taken = _names(tools) + ["look", "make", "merge"]
        unique = name
        n = 2
        while unique in taken:
            unique = f"{name}{n}"
            n += 1
        definition = build(pack, construct, source, mods, interface)
        signature = interface["signature"]
        takes = _has(signature["in"], "snapshot")
        gives = _has(signature["out"], "snapshot")
        props = {}
        required = []
        for key in signature["in"].get("properties", {}):
            if key != "snapshot":
                props[key] = signature["in"]["properties"][key]
        for key in signature["in"].get("required", []):
            if key != "snapshot":
                required.append(key)
        schema = {"type": "object", "properties": props, "required": required, "additionalProperties": False}
        description = interface["description"]
        if gives:
            description = description + " 交回改过的目录：结果里的 name 与 diff 给你审，满意后用 merge(name) 并入。"

        def call(value: Any) -> Any:
            base = ws["current"]
            given = value
            dependencies = []
            if takes:
                given = dict(value)
                given["snapshot"] = base
                dependencies = [base]
            label = ""
            if gives:
                label = fork(ws, unique)
            got = run_agent(instantiate, supply, back, done, definition, given, dependencies)
            if not gives:
                return got
            offer(ws, label, got["snapshot"])
            d = execute(exec, ws["current"], ["git", "diff", "--no-index", "--stat", "/mnt/a", "/mnt/b"],
                        mounts={"a": base, "b": got["snapshot"]})
            rest = {k: v for k, v in got.items() if k != "snapshot"}
            return {"result": rest, "name": label, "diff": d["stdout"]}

        tools.append({"name": unique, "description": description, "input": schema, "call": call})
        return unique

    for name in sorted(listed.keys()):
        load(name, read_text_at(home, listed[name]), home["current"])

    def look(name: str = "", source: bool = False) -> Any:
        """看 Library 里有什么、怎么用。不给 name：总说明与所有模块各一句话；给模块名（如 library.files）：它的公开面，
        函数的签名与说明、类型与 Port 的形状；再给 source=True 才是源码。写 Agent 之前先看。

        Args:
            name: 模块名，如 library.workspace。
            source: 要源码，不要公开面。
        """
        modules = home["current"]
        if name == "":
            listing = croon.send(infer, {"modules": modules}, dependencies=[modules]).value["modules"]
            return {"readme": read_text_at(home, "library/README.md"), "modules": listing}
        if source:
            return read_text_at(home, name.replace(".", "/") + ".py")
        return croon.send(infer, {"modules": modules, "module": name}, dependencies=[modules]).value["surface"]

    def make(name: str, source: str = "", path: str = "") -> Any:
        """造一个 Agent 并载入工具表：给 source（Lang0 源码，须有顶层 def main，参数与返回值都标类型，可以
        import library 里的模块）或 path（工作区里的 .py 文件，或含 main.py 的目录）。造好后它就是一个工具，
        以返回的名字直接调用，输入按它 main 的参数写。名字重了会加序号。它要 engine 与 exec 之外的 Port 时，
        交办的一方给不了。

        Args:
            name: 工具的名字。
            source: Lang0 源码。
            path: 工作区里的源码路径。
        """
        if (source == "") == (path == ""):
            raise ValueError("source 与 path 给一个")
        modules = home["current"]
        mods = modules
        text = source
        if path != "":
            got = _source(ws, pack, modules, path)
            text = got[0]
            mods = got[1]
        unique = load(name, text, mods)
        for t in tools:
            if isinstance(t, dict) and t["name"] == unique:
                return {"tool": unique, "input": t["input"]}
        return {"tool": unique}

    def merge(name: str) -> str:
        """把一次交办交回的产物并入自己的工作区：没动过就直接采用，动过就按目录三方合并，冲突时不动。

        Args:
            name: 交办时返回的 name。
        """
        adopt(ws, exec, name)
        return f"已并入 {name}"

    return [look, make, merge]
