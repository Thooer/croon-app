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
if signature(source) != ['review']:
    verdict(False, f"接口与任务写明的不符：参数是 {signature(source)}，要 ['review']", stage="interface", name=name)
    raise SystemExit
if len(outs) < 2:
    verdict(False, f"主体只调了 {len(outs)} 次模型，要两次", stage="wrong", name=name, outs=outs)
    raise SystemExit
last = outs[-2:]
if last != ["正面", "负面"]:
    verdict(False, f"判断不对：{last}", stage="wrong", name=name)
    raise SystemExit
ok = "正面" in d["final_text"] and "负面" in d["final_text"]
verdict(ok, "两次判断都对，W0 转述对" if ok else "主体对，W0 没有转述", stage="report")
