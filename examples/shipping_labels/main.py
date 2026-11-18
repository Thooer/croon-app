"""按份数生成发货标签。"""
from typing import TypedDict
import croon
from library.directory import Exec
from library.workspace import advance, workspace

Printed = TypedDict("Printed", {"copies": int, "file": str, "snapshot": str})


def main(copies: int, snapshot: str, exec: Exec) -> Printed:
    """把 label.txt 的内容按指定份数写进 print.txt。

    Args:
        copies: 打印份数，取 1 到 5 的整数。
    """
    if copies < 1 or copies > 5:
        raise ValueError("打印份数须在 1 到 5 之间")
    ws = workspace(snapshot)
    script = 'set -e; rm -f print.txt; i=0; while [ "$i" -lt "$1" ]; do cat label.txt; i=$((i+1)); done > print.txt'
    result = advance(ws, exec, ["sh", "-c", script, "sh", str(copies)])
    if result["exit"] != 0 or result["timed_out"]:
        raise RuntimeError(f"生成标签失败：{result['stderr']}")
    return croon.deliver({"copies": copies, "file": "print.txt", "snapshot": ws["current"]}, [ws["current"]])
