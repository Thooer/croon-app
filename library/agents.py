"""运行中造 Agent 并按它的 Interface 交办：模型只写 Lang0 源码，接线由目标自己的 Interface 决定。"""
from typing import Any, TypedDict
import croon
from croon import Inbox, Port, receive
from library.kernel import Construct, Instantiate, Outcome, Pack
from library.infer import Infer
from library.model import Generate
from library.directory import Exec

# 造出的 Agent：Definition 的 ID 与它的 Interface 一起记着，交办时不用再去内容库读。
Made = TypedDict("Made", {"definition": str, "interface": Any})
Interface = TypedDict("Interface", {"description": str, "signature": Any, "provides": Any, "requires": Any})
Done = Port(Outcome, doc="收结束状态")


def _roots(values: list[str]) -> list[str]:
    """Bundle ID 的集合：排序、去重。"""
    out = []
    for value in sorted(values):
        if value not in out:
            out.append(value)
    return out


# 交办方能附带的 Port。instantiate 不在其中：交办出去的 Agent 不能再交办。
SUPPLIED = ["engine", "exec"]


def _unsupplied(interface: Any) -> list[str]:
    """目标要、交办方给不了的 Port，按名字排序。"""
    out = []
    for port in sorted(interface["requires"]):
        if port not in SUPPLIED:
            out.append(port)
    return out


def agent_tools(pack: Pack, construct: Construct, infer: Infer, modules: str) -> Any:
    """返回运行中造 Agent 的两个工具，与记着造出来的 Agent 的表：名字到 Definition 与 Interface。"""
    agents = {}

    def construct_agent(name: str, source: str) -> str:
        """构造一个 Agent，之后用 delegate_to 按名字交办。

        Args:
            name: 给它起的名字。
            source: Lang0 源码，须有顶层 def main，参数与返回值都标类型；可以 import library 里的模块。
        """
        body = {"source": source, "modules": modules}
        inferred = croon.send(infer, {"language": "lang0@1", "body": body}, dependencies=[modules]).value
        definition = {"language": "lang0@1", "interface": inferred["interface"], "body": body}
        packed = croon.send(pack, {"content": {"json": definition}, "dependencies": [modules]},
                            dependencies=[modules]).value
        built = croon.send(construct, {"definition": packed["id"]}, dependencies=[packed["id"]]).value
        unique = name
        n = 2
        while unique in agents:
            unique = f"{name}{n}"
            n += 1
        agents[unique] = {"definition": built["id"], "interface": inferred["interface"]}
        missing = _unsupplied(inferred["interface"])
        if missing:
            return f"已构造 {unique}，但它要 {', '.join(missing)}，交办时接不上：先改源码去掉这些 Port"
        return f"已构造 {unique}"

    def describe_agent(name: str) -> Interface:
        """看一个 Agent 要什么输入、需要哪些 Port。

        Args:
            name: 造出来的 Agent 的名字。
        """
        made = agents.get(name)
        if made is None:
            raise KeyError(f"没有名为 {name} 的 Agent")
        return made["interface"]

    return [construct_agent, describe_agent], agents


def run_agent(instantiate: Instantiate, engine: Generate, exec: Exec, back: Done, done: Inbox(Done),
              made: Made, value: Any, dependencies: list[str]) -> Any:
    """按目标的 Interface 实例化它，等它结束，交回它的结束值。

    目标要的 Port 不在交办一方手里的，报错而不硬接；`instantiate` 默认不在接线里，
    所以交办出去的 Agent 不能再交办，见 docs/Library/Runtime.md「委托不超过两层」。

    Args:
        made: 造这个 Agent 时记下的那一项：Definition 的 ID 与 Interface。
        value: 按它 Signature 的 in 写好的输入，先用 describe_agent 看清。
        dependencies: 输入要用到的 Bundle。
    """
    missing = _unsupplied(made["interface"])
    if missing:
        raise RuntimeError(f"目标要 {', '.join(missing)}，交办的一方接不上")
    attach = {}
    for port in made["interface"]["requires"]:
        if port == "engine":
            attach[port] = engine
        else:
            attach[port] = exec
    r = croon.send(instantiate,
                   {"definition": made["definition"],
                    "input": {"value": value, "dependencies": _roots(dependencies)}, "seats": {}, "self": {}},
                   dependencies=_roots(dependencies + [made["definition"]]), attach=attach)
    reg = croon.send(r.attach["notify"], {}, attach={"to": back})
    a = receive(done, via=reg.derived["to"].handle)
    if not a.value["ok"]:
        raise RuntimeError(a.value["error"]["message"])
    return a.value["value"]
