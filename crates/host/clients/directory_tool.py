"""用 CLI 或本地 stdio MCP 调用一个已绑定输入和结果目录的 Croon 包。"""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from uuid import uuid4

import anyio
from jsonschema import ValidationError, validate

from croon_cli import BoundSource

SCHEMAS = Path(__file__).resolve().parents[1] / "schema"


def schema(name):
    return json.loads((SCHEMAS / f"{name}.json").read_text(encoding="utf-8"))


class DirectoryTool:
    def __init__(self, binary, package, binding, host_config=None):
        settings = json.loads(binding.read_text(encoding="utf-8"))
        validate(settings, schema("host.directory.binding"))
        if any(not path.strip() for path in settings.values()):
            raise ValueError("绑定文件里的目录路径须非空")
        base = binding.absolute().parent
        snapshot = (base / settings["input_dir"]).resolve(strict=True)
        self.results = (base / settings["results_dir"]).resolve()
        if not snapshot.is_dir():
            raise ValueError(f"输入目录应为已有的目录：{snapshot}")
        if self.results.is_relative_to(snapshot):
            raise ValueError("结果存放位置须在输入目录之外，以免下次导入上次的产物")
        self.source = BoundSource(binary, package.name, package / "main.py", snapshot=snapshot, config=host_config)

    async def call(self, arguments):
        validate(arguments, self.source.tool["input"])
        self.results.mkdir(parents=True, exist_ok=True)
        session = f"tool-{uuid4().hex}"
        destination = self.results / session
        with tempfile.TemporaryDirectory(prefix=".croon-", dir=self.results) as staging:
            out = Path(staging) / "out"
            command = self.source.command(arguments, out) + ["--session", session, "--quiet"]
            try:
                with anyio.fail_after(240):
                    process = await anyio.run_process(command, check=False)
            except TimeoutError as exc:
                raise ValueError(f"会话 {session} 超出 240 秒的调用时限") from exc
            if process.returncode:
                diagnostic = process.stderr.decode("utf-8", errors="replace").strip()
                raise ValueError(f"{session}（退出 {process.returncode}）：{diagnostic}")
            value = json.loads(process.stdout)
            if not out.is_dir():
                raise ValueError(f"{session} 没有交回目录产物")
            out.rename(destination)
        return {"value": value, "directory": str(destination)}


async def serve(binding):
    from mcp import types
    from mcp.server.lowlevel import Server
    from mcp.server.stdio import stdio_server

    tool = binding.source.tool
    server = Server("croon")

    descriptor = types.Tool(
        name=tool["name"], description=tool["description"], inputSchema=tool["input"],
        outputSchema=schema("host.directory.result"),
    )

    @server.list_tools()
    async def list_tools():
        return [descriptor]

    @server.call_tool(validate_input=False)
    async def call_tool(name, arguments):
        if name != tool["name"]:
            raise ValueError(f"没有注册工具 {name}")
        return await binding.call(arguments)

    async with stdio_server() as (reader, writer):
        await server.run(reader, writer, server.create_initialization_options())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="croon-app 可执行文件")
    parser.add_argument("--package", type=Path, required=True, help="含 main.py 的包目录，目录名用作工具名")
    parser.add_argument("--binding", type=Path, required=True, help="使用者的目录绑定文件")
    parser.add_argument("--config", type=Path, help="传给 exec 的宿主配置")
    modes = parser.add_subparsers(dest="mode", required=True)
    modes.add_parser("describe", help="显示绑定后模型可见的工具说明")
    modes.add_parser("serve", help="经 stdio 提供 MCP 服务")
    modes.add_parser("call", help="直接运行并交回结果").add_argument("arguments", help="JSON 业务参数")
    args = parser.parse_args()
    binding = DirectoryTool(args.binary.resolve(), args.package.resolve(), args.binding, args.config)
    if args.mode == "serve":
        anyio.run(serve, binding)
    elif args.mode == "describe":
        print(json.dumps(binding.source.tool, ensure_ascii=False))
    else:
        result = anyio.run(binding.call, json.loads(args.arguments))
        print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as exc:
        sys.exit(exc.stderr.strip() or str(exc))
    except ValidationError as exc:
        sys.exit(exc.message)
    except (OSError, ValueError, subprocess.TimeoutExpired) as exc:
        sys.exit(str(exc) or type(exc).__name__)
