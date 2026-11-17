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
if signature(source) != ['text']:
    verdict(False, f"接口与任务写明的不符：参数是 {signature(source)}，要 ['text']", stage="interface", name=name)
    raise SystemExit
if not outs:
    verdict(False, "录像里没有主体自己的模型调用", stage="wrong", name=name)
    raise SystemExit
k = outs[-1]
parts = [p for p in re.split(r"[,，、]", k) if p.strip()]
if len(parts) != 3:
    verdict(False, f"不是三个关键词：{k!r}", stage="wrong", name=name)
    raise SystemExit
ok = k in d["final_text"]
verdict(ok, "三个关键词来自主体，W0 如实转述" if ok else "主体对，W0 没有如实转述", stage="report", keywords=k)
