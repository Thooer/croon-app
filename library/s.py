"""S：被交办的子 Agent，在显式交付的目录 Bundle 中改代码，交回总结与产物 Bundle。"""
from typing import TypedDict
import croon
from library.model import Generate
from library.directory import Exec
from library.files import file_tools
from library.react import react

Out = TypedDict("Out", {"summary": str, "snapshot": str})

SYSTEM = "你在 /work 里改代码。改完用一两句话说清改了什么。"


def main(task: str, snapshot: str, engine: Generate, exec: Exec) -> Out:
    """改一处代码，交回总结与改过的 Bundle。"""
    state = {"snapshot": snapshot}
    summary = react(engine, file_tools(exec, state), [{"role": "user", "text": task}], SYSTEM)
    return croon.deliver({"summary": summary, "snapshot": state["snapshot"]}, [state["snapshot"]])
