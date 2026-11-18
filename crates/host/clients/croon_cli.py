"""把源码和宿主选定的路径绑定到一次 CLI 工具注册。只依赖 Python 标准库。"""

import json
import subprocess


def run_json(command, env=None):
    proc = subprocess.run(command, env=env, capture_output=True, text=True, check=True, timeout=240)
    return json.loads(proc.stdout)


class BoundSource:
    def __init__(self, binary, name, source, *, snapshot=None, config=None, env=None):
        inputs = ["--source", source]
        if snapshot is not None:
            inputs += ["--snapshot", snapshot]
        self.tool = run_json([binary, "describe", *inputs, "--tool", name], env)
        self._exec = [binary, "exec", *inputs, "--json"]
        if config is not None:
            self._exec += ["--config", config]

    def command(self, arguments, out):
        return [*self._exec, "--input", json.dumps(arguments), "--out", out]
