"""W0：用户伸向 App 的手。自带的只有管理器：载入家目录清单里的工具，造 Agent 并交办，审过交回的再并入。"""
from typing import TypedDict
from croon import Inbox
from library.kernel import Construct, Instantiate, Pack
from library.infer import Infer
from library.agents import Done
from library.delegation import delegation_tools
from library.model import Generate
from library.directory import Exec
from library.react import react
from library.tools import load
from library.workspace import read_json_at, workspace
import croon

Out = TypedDict("Out", {"summary": str, "snapshot": str})

SYSTEM = "你是 W0。能改仓库里的文件、跑命令，也能把一件事交办给 Agent：现成的见下，也可以自己写 Lang0 源码造新的；交回的先看 diff，满意再并入。"


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
    lines = []
    for item in read_json_at(mine, "tools.json")["tools"]:
        if item["kind"] == "functions":
            tools = tools + load(item["package"], supply, ws)
        else:
            name = item["name"]
            listed[name] = item["path"]
            lines.append(f"- {name}：{item['description']}")
    ports = {"instantiate": instantiate, "pack": pack, "construct": construct, "infer": infer}
    tools = tools + delegation_tools(ws, mine, exec, ports, back, done, supply, listed)
    system = SYSTEM + "\n现成的 Agent：\n" + "\n".join(lines)
    summary = react(engine, tools, [{"role": "user", "text": task}], system)
    return croon.deliver({"summary": summary, "snapshot": ws["current"]}, [ws["current"]])
