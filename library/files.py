"""文件工具：读、写、运行，都在调用方的私有工作区里，见 library.workspace。"""
from typing import Any, Callable, TypedDict
from library.directory import Exec
from library.workspace import Workspace, advance

RunOut = TypedDict("RunOut", {"exit": int, "stdout": str, "stderr": str})


def file_tools(exec: Exec, ws: Workspace) -> list[Callable]:
    def read_file(path: str) -> str:
        """读快照里的一个文件。

        Args:
            path: 相对 /work 的路径。
        """
        r = advance(ws, exec, ["cat", "--", path])
        if r["exit"] != 0:
            raise OSError(r["stderr"])
        return r["stdout"]

    def write_file(path: str, content: str) -> str:
        """写入文件并采用产物。

        Args:
            path: 相对 /work 的路径。
            content: 文件的新内容。
        """
        r = advance(ws, exec, ["sh", "-c", 'cat > "$1"', "sh", path], content)
        if r["exit"] != 0:
            raise OSError(r["stderr"])
        return f"已写入 {path}"

    def run(command: str, timeout: int = 60) -> RunOut:
        """运行 shell 命令并采用稳定产物，失败也保留改动。

        Args:
            command: 交给 sh -c 的命令。
            timeout: 秒，最多 600。
        """
        r = advance(ws, exec, ["sh", "-c", command], timeout=timeout)
        return {"exit": r["exit"], "stdout": r["stdout"], "stderr": r["stderr"]}

    return [read_file, write_file, run]


def tools(supply: dict[str, Any], ws: Workspace) -> list[Callable]:
    """函数工具包 files：读、写、运行，都在 ws 里。supply 是主体愿意交出的 Port，这里只用 exec。"""
    return file_tools(supply["exec"], ws)
