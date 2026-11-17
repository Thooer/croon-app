"""S：改代码的 Agent，在显式交付的目录里改，交回总结与产物。"""
from typing import TypedDict
import croon
from library.model import Generate
from library.directory import Exec
from library.files import file_tools
from library.react import react
from library.workspace import workspace

Out = TypedDict("Out", {"summary": str, "snapshot": str})

SYSTEM = "你在 /work 里改代码。改完用一两句话说清改了什么。"


def main(task: str, snapshot: str, engine: Generate, exec: Exec) -> Out:
    """改一处代码，交回总结与改过的 Bundle。"""
    ws = workspace(snapshot)
    summary = react(engine, file_tools(exec, ws), [{"role": "user", "text": task}], SYSTEM)
    return croon.deliver({"summary": summary, "snapshot": ws["current"]}, [ws["current"]])
