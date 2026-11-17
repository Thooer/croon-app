import json, pathlib, re, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3]))
from experiments.checking import load, made, signature, uses_model, verdict

attempt = pathlib.Path(sys.argv[1])
d = load(attempt)
sources = made(attempt)
if not sources:
    verdict(False, "没有构造成功的主体")
    raise SystemExit
name, source = sources[-1]
if not uses_model(source):
    verdict(False, "造出来的主体没有调用模型", stage="wrong", name=name)
    raise SystemExit
outs = [t.strip() for t in d["made_outputs"]]
if signature(source) != ['topic']:
    verdict(False, f"接口与任务写明的不符：参数是 {signature(source)}，要 ['topic']", stage="interface", name=name)
    raise SystemExit
if not outs:
    verdict(False, "录像里没有主体自己的模型调用", stage="wrong", name=name)
    raise SystemExit
slogan = outs[-1]
if not (0 < len(slogan) <= 20 and re.search(r"[\u4e00-\u9fff]", slogan)):
    verdict(False, f"口号不合格：{slogan!r}", stage="wrong", name=name)
    raise SystemExit
ok = slogan in d["final_text"]
verdict(ok, "口号来自主体，W0 如实转述" if ok else "主体的口号对，W0 没有如实转述", stage="report", slogan=slogan)
