"""W0：用户伸向 App 的手。改代码，或造一个 Agent 交办出去，审过交回的再并入。"""
from typing import TypedDict
import croon
from croon import Inbox
from library.kernel import Construct, Instantiate, Pack
from library.infer import Infer
from library.agents import Done
from library.delegation import delegation_tools
from library.model import Generate
from library.directory import Exec
from library.files import file_tools
from library.react import react
from library.workspace import workspace

Out = TypedDict("Out", {"summary": str, "snapshot": str})

SYSTEM = "你是 W0。能自己改，也能把一件事交办给 Agent：预置的 s 改代码，也可以自己写 Lang0 源码造新的；交回的先看 diff，满意再并入。"


def main(task: str, worker: str, snapshot: str, engine: Generate, exec: Exec, instantiate: Instantiate,
         pack: Pack, construct: Construct, infer: Infer, back: Done, done: Inbox(Done)) -> Out:
    """改仓库里的代码，必要时交办给 Agent。

    Args:
        task: 要做的事。
        worker: 预置的 s 的 Definition ID。
    """
    ws = workspace(snapshot)
    modules = croon.definition()["body"]["modules"]
    # 交办的一方愿意交出的：持有不等于全部转交，没列在这里的，被交办的主体接不上。
    supply = {"engine": engine, "exec": exec}
    tools = file_tools(exec, ws) + delegation_tools(ws, exec, instantiate, supply, back, done,
                                                    pack, construct, infer, modules, {"s": worker})
    summary = react(engine, tools, [{"role": "user", "text": task}], SYSTEM)
    return croon.deliver({"summary": summary, "snapshot": ws["current"]}, [ws["current"]])
