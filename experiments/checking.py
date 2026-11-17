"""检查器共用的几个函数：读一次尝试的整理结果，在检验方自己的输入上用 `croon-app exec` 把造出来的源码再跑一遍。

检查器由实验方固定，被测主体不能改它；检查的是实际结果，不是 W0 的转述。见 docs/Experiments.md。
"""
import json
import pathlib
import re
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
APP = ROOT / "target" / "debug" / "croon-app"


def verdict(passed, reason, stage="", **details):
    """打印检查器的判定。stage 只在失败时给，且要在 Experiments.md 的阶段表里。"""
    print(json.dumps({"passed": passed, "reason": reason, "stage": stage if not passed else "", "details": details}, ensure_ascii=False))


def load(attempt):
    return json.loads((attempt / "digest.json").read_text())


def made(attempt, name=None):
    """这次尝试里构造成功的主体：(工具名, 源码)，按先后。给了 name 只取那个名字起头的。"""
    items = [(m["tool"], m["source"]) for m in load(attempt)["makes"] if m["ok"] and m["tool"]]
    return [i for i in items if name is None or i[0].startswith(name)]


PORTS = ("Generate", "Exec", "Pack", "Construct", "Instantiate", "Infer", "Look", "Notify", "Correct", "Revoke", "Inbox")


def signature(source):
    """源码里 main 的参数名：只算调用方给的值，Port 类型的参数是接线，不算。粗读，用来核对任务写明的接口。"""
    m = re.search(r"def main\(([^)]*)\)", source, re.S)
    if not m:
        return []
    names = []
    for part in m.group(1).split(","):
        part = part.strip()
        if part and ":" in part:
            name, kind = part.split(":", 1)
            if not any(p in kind for p in PORTS):
                names.append(name.strip())
    return names


def uses_model(source):
    return "library.model" in source and "croon.send" in source


def rerun(attempt, source, inputs, snapshot=None, timeout=300):
    """在全新的 Kernel 目录里把源码按检验方的输入跑一遍。返回 (退出码, 输出文本, 结果目录)。

    不调模型：配置的模型来源是空录像，主体若要问模型，在这里会失败。要问模型的任务不用这个函数。
    """
    work = pathlib.Path(tempfile.mkdtemp(prefix="croon-check-"))
    cfg = json.loads((attempt / "host.json").read_text())
    empty = work / "empty.json"
    empty.write_text('{"entries":[]}')
    cfg["system"] = f"check-{int(time.time() * 1000)}"
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


def same(text, want):
    """exec 的输出是 JSON 文本：按 JSON 解析后再比，空格与大小写不造成误判。"""
    try:
        return json.loads(text.splitlines()[0]) == want
    except Exception:
        return False


def reported(attempt, wanted):
    """W0 的最后一句话里有没有这些字符串（转述对不对）。"""
    text = load(attempt)["final_text"]
    return [w for w in wanted if w not in text]
