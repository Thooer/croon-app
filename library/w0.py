"""W0：用户伸向 App 的手。自带的只有管理器：看 Library，载入清单里的工具，造 Agent 载入，审过交回的再并入。"""
from typing import TypedDict
from croon import Inbox
from library.kernel import Construct, Instantiate, Pack
from library.infer import Infer
from library.agents import Done
from library.manager import manager_tools
from library.model import Generate
from library.directory import Exec
from library.react import react
from library.tools import load
from library.workspace import read_json_at, workspace
import croon

Out = TypedDict("Out", {"summary": str, "snapshot": str})

SYSTEM = "你是 W0。能改仓库里的文件、跑命令。Library 里有可复用的模块：用 look 看有什么、怎么用。要造一个新的 Agent，先 look，再用 make 把 Lang0 源码造出来载入，它就成了你的一个工具，直接调用。交回目录的先看 diff，满意再 merge。"


def main(task: str, home: str, snapshot: str, engine: Generate, exec: Exec, instantiate: Instantiate,
         pack: Pack, construct: Construct, infer: Infer, back: Done, done: Inbox(Done)) -> Out:
    """改仓库里的代码，必要时交办给 Agent。

    Args:
        task: 要做的事。
        home: 家目录的 Bundle ID，里面有包与工具清单。
    """
    ws = workspace(snapshot)
    mine = workspace(home)
    # 交办的一方愿意交出的：持有不等于全部转交，没列在这里的，被交办的主体接不上。
    supply = {"engine": engine, "exec": exec}
    tools = []
    listed = {}
    for item in read_json_at(mine, "tools.json")["tools"]:
        if item["kind"] == "functions":
            tools.extend(load(item["package"], supply, ws))
        else:
            listed[item["name"]] = item["path"]
    ports = {"instantiate": instantiate, "pack": pack, "construct": construct, "infer": infer}
    tools.extend(manager_tools(ws, mine, exec, ports, back, done, supply, listed, tools))
    summary = react(engine, tools, [{"role": "user", "text": task}], SYSTEM)
    return croon.deliver({"summary": summary, "snapshot": ws["current"]}, [ws["current"]])
