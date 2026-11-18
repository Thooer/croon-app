"""从进程外注册并调用一个纯计算工具。用法见仓库 README「从自己的程序调用」。"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def run_json(binary, *args, env):
    proc = subprocess.run(
        [binary, *args], env=env, capture_output=True, text=True, timeout=240, check=True
    )
    return json.loads(proc.stdout)


def bind(binary, name, source, out, env, *, snapshot=None, config=None):
    """注册者选源码、配置和目录。模型调用时只传模型的参数。"""
    inputs = ["--source", source]
    if snapshot is not None:
        inputs += ["--snapshot", snapshot]
    tool = run_json(binary, "describe", *inputs, "--tool", name, env=env)
    spec = {
        "type": "function",
        "function": {
            "name": tool["name"],
            "description": tool["description"],
            "parameters": tool["input"],
        },
    }
    settings = [] if config is None else ["--config", config]

    def invoke(arguments):
        return run_json(
            binary, "exec", *inputs, *settings, "--input", json.dumps(arguments),
            "--out", out, "--json", env=env,
        )

    return spec, invoke


def main(binary):
    source = Path(__file__).with_name("tally.py")
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        env = dict(os.environ, XDG_CONFIG_HOME=str(work / "config"), XDG_STATE_HOME=str(work / "state"))
        tool, invoke = bind(binary, "count_passing", source, work / "out", env)
        spec = tool["function"]
        print(spec["name"], spec["description"], list(spec["parameters"]["properties"]))

        result = invoke({"scores": [3, 9, 8], "minimum": 8})
        assert result == {"summary": "达标 2 项", "count": 2}, result
        print(json.dumps(result, ensure_ascii=False))

        try:
            invoke({"scores": "oops", "minimum": 8})
        except subprocess.CalledProcessError as exc:
            assert exc.stdout == "" and exc.stderr, exc
            print(f"错误输入的退出码 {exc.returncode}")
        else:
            raise AssertionError("错误输入被接受")


if __name__ == "__main__":
    try:
        main(Path(sys.argv[1]).resolve())
    except subprocess.CalledProcessError as exc:
        sys.stderr.write(exc.stderr)
        sys.exit(exc.returncode)
