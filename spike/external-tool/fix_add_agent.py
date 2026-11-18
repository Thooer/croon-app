from typing import TypedDict
import croon
from library.model import Generate
from library.directory import Exec
from library.files import file_tools
from library.react import react
from library.workspace import workspace

Out = TypedDict("Out", {"summary": str, "snapshot": str})

def main(task: str, snapshot: str, engine: Generate, exec: Exec) -> Out:
    """修复交办的代码并交回可审查的目录。"""
    ws = workspace(snapshot)
    summary = react(engine, file_tools(exec, ws), [{"role": "user", "text": task}], "你负责在 /work 中检查并修复代码。遵循仓库的适用指令，保持改动最小，验证目标行为，最后说明修改和验证结果。")
    return croon.deliver({"summary": summary, "snapshot": ws["current"]}, [ws["current"]])