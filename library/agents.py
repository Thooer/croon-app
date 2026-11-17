"""实例化一个 Agent 并等它结束：按目标自己声明的 Interface 接线，不看它是谁造的。

目标要什么 Port，从交办一方愿意交出的（`supply`，名字到引用值）里按名字取；取不到的当场报错，
取到的是否恰好覆盖、规格是否相同由 Kernel 在实例化时检查，这里不另写一份。
见 croon-app:docs/Library/Runtime.md「交办」。
"""
from typing import Any
import croon
from croon import Inbox, Port, receive
from library.kernel import Construct, Instantiate, Outcome, Pack
from library.infer import Infer

Done = Port(Outcome, doc="收结束状态")


def _roots(values: list[str]) -> list[str]:
    """Bundle ID 的集合：排序、去重。"""
    out = []
    for value in sorted(values):
        if value not in out:
            out.append(value)
    return out


def interface_of(definition: str) -> Any:
    """一份 Definition Bundle 的 Interface，从它自己的内容里读；它要在本次已取得的范围里。"""
    return croon.read_json(croon.read_json(definition)["content"])["interface"]


def unsupplied(interface: Any, supply: Any) -> list[str]:
    """目标要、交办一方不愿或不能交出的 Port，按名字排序。"""
    out = []
    for port in sorted(interface["requires"]):
        if port not in supply:
            out.append(port)
    return out


def infer_interface(infer: Infer, source: str, modules: str) -> Any:
    """从源码推出 Interface；写错的源码得到 Lang0 的诊断。纯计算，没有副作用。"""
    body = {"source": source, "modules": modules}
    return croon.send(infer, {"language": "lang0@1", "body": body}, dependencies=[modules]).value["interface"]


def build(pack: Pack, construct: Construct, source: str, modules: str, interface: Any) -> str:
    """把源码立成 Definition Bundle 并构造，返回它的 ID。"""
    definition = {"language": "lang0@1", "interface": interface, "body": {"source": source, "modules": modules}}
    packed = croon.send(pack, {"content": {"json": definition}, "dependencies": [modules]},
                        dependencies=[modules]).value
    return croon.send(construct, {"definition": packed["id"]}, dependencies=[packed["id"]]).value["id"]


def run_agent(instantiate: Instantiate, supply: Any, back: Done, done: Inbox(Done),
              definition: str, value: Any, dependencies: list[str]) -> Any:
    """实例化目标，等它结束，原样交回它的结束值。

    Args:
        supply: 交办一方愿意交出的 Port，名字到引用值；目标声明的名字在里面才附带。
        definition: 目标的 Definition Bundle ID。
        value: 按它 Signature 的 in 写好的输入。
        dependencies: 输入要用到的 Bundle。
    """
    missing = unsupplied(interface_of(definition), supply)
    if missing:
        raise RuntimeError(f"目标要 {', '.join(missing)}，交办的一方给不了")
    attach = {}
    for port in interface_of(definition)["requires"]:
        attach[port] = supply[port]
    r = croon.send(instantiate,
                   {"definition": definition, "input": {"value": value, "dependencies": _roots(dependencies)},
                    "seats": {}, "self": {}},
                   dependencies=_roots(dependencies + [definition]), attach=attach)
    reg = croon.send(r.attach["notify"], {}, attach={"to": back})
    a = receive(done, via=reg.derived["to"].handle)
    if not a.value["ok"]:
        raise RuntimeError(a.value["error"]["message"])
    return a.value["value"]
