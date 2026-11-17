"""检查器共用的几个函数：从一次尝试里取出造出来的源码，在检验方自己的输入上用 `croon-app exec` 再跑一遍。

检查器由实验方固定，被测主体不能改它；检查的是实际结果，不是 W0 的转述。见 docs/Experiments.md。
"""
import json
import pathlib
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
APP = ROOT / "target" / "debug" / "croon-app"


def verdict(passed, reason, **details):
    print(json.dumps({"passed": passed, "reason": reason, "details": details}, ensure_ascii=False))


def made_sources(attempt):
    """这次尝试里构造成功的主体：名字到源码，按先后。"""
    makes = json.loads((attempt / "makes.json").read_text())
    return [(m["name"], m["source"]) for m in makes if m["ok"]]


def rerun(attempt, source, inputs, snapshot=None, timeout=300):
    """在全新的 Kernel 目录里把源码按检验方的输入跑一遍。返回 (退出码, 输出文本)。

    不调模型：配置的模型来源是空录像，主体若要问模型，在这里会失败。要问模型的任务不用这个函数。
    """
    work = pathlib.Path(tempfile.mkdtemp(prefix="croon-check-"))
    cfg = json.loads((attempt / "host.json").read_text())
    system = f"check-{int(time.time() * 1000)}"
    empty = work / "empty.json"
    empty.write_text('{"entries":[]}')
    cfg["system"] = system
    cfg["kernel"] = str(work / "kernel")
    cfg["directory"]["state"] = str(work / "state")
    cfg["model"] = {"replay": str(empty)}
    config = work / "host.json"
    config.write_text(json.dumps(cfg))
    src = work / "main.py"
    src.write_text(source)
    args = [str(APP), "exec", "--config", str(config), "--source", str(src), "--input", json.dumps(inputs), "--out", str(work / "out")]
    if snapshot is not None:
        args += ["--snapshot", str(snapshot)]
    result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    return result.returncode, (result.stdout + result.stderr).strip(), work / "out"
