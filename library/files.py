"""文件工具显式采用一次执行交回的稳定快照。"""
from typing import Callable, TypedDict
import croon
from library.directory import Exec

RunOut = TypedDict("RunOut", {"exit": int, "stdout": str, "stderr": str})


def execute(exec: Exec, snapshot: str, argv: list[str], stdin: str = "",
            mounts: dict[str, str] = {}, timeout: int = 60):
    return croon.send(exec, {"snapshot": snapshot, "mounts": mounts, "argv": argv,
                             "stdin": stdin, "timeout": timeout},
                      dependencies=_dependencies(snapshot, mounts)).value


def _dependencies(snapshot: str, mounts: dict[str, str]) -> list[str]:
    values = [snapshot]
    for value in mounts.values():
        if value not in values:
            values.append(value)
    return sorted(values)


def file_tools(exec: Exec, state: dict[str, str]) -> list[Callable]:
    def invoke(argv, stdin="", timeout=60):
        r = execute(exec, state["snapshot"], argv, stdin, timeout=timeout)
        state["snapshot"] = r["snapshot"]
        return r

    def read_file(path: str) -> str:
        """读快照里的一个文件。

        Args:
            path: 相对 /work 的路径。
        """
        r = invoke(["cat", "--", path])
        if r["exit"] != 0:
            raise OSError(r["stderr"])
        return r["stdout"]

    def write_file(path: str, content: str) -> str:
        """写入文件并采用产物。

        Args:
            path: 相对 /work 的路径。
            content: 文件的新内容。
        """
        r = invoke(["sh", "-c", 'cat > "$1"', "sh", path], content)
        if r["exit"] != 0:
            raise OSError(r["stderr"])
        return f"已写入 {path}"

    def run(command: str, timeout: int = 60) -> RunOut:
        """运行 shell 命令并采用稳定产物，失败也保留改动。

        Args:
            command: 交给 sh -c 的命令。
            timeout: 秒，最多 600。
        """
        r = invoke(["sh", "-c", command], timeout=timeout)
        return {"exit": r["exit"], "stdout": r["stdout"], "stderr": r["stderr"]}

    return [read_file, write_file, run]
