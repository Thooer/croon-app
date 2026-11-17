import json, pathlib, re, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3]))
from experiments.checking import made_sources, verdict

attempt = pathlib.Path(sys.argv[1])
sources = made_sources(attempt)
if not sources:
    verdict(False, "没有构造成功的主体")
    raise SystemExit
name, source = sources[-1]
if "library.model" not in source or "croon.send" not in source:
    verdict(False, "造出来的主体没有调用模型", name=name)
    raise SystemExit
record = json.loads((attempt / "record.json").read_text()) if (attempt / "record.json").exists() else {}
tape = json.loads((attempt / "cassette.json").read_text())["entries"]
# 主体自己的那次模型调用：system 里没有 W0 的请求。
inner = [e for e in tape if "W0" not in e["request"]["system"]]
if not inner:
    verdict(False, "录像里没有主体自己的模型调用", name=name)
    raise SystemExit
slogan = inner[-1]["outcome"]["value"]["text"].strip()
ok = 0 < len(slogan) <= 20 and re.search(r"[\u4e00-\u9fff]", slogan)
verdict(bool(ok), "口号来自主体、长度与文字合格" if ok else f"口号不合格：{slogan!r}", slogan=slogan, length=len(slogan))
