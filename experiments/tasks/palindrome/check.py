import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3]))
from experiments.checking import made, rerun, reported, same, signature, uses_model, verdict, load

attempt = pathlib.Path(sys.argv[1])
sources = made(attempt)
if not sources:
    verdict(False, "没有构造成功的主体")
    raise SystemExit
name, source = sources[-1]
if signature(source) != ['text']:
    verdict(False, f"接口与任务写明的不符：参数是 {signature(source)}，要 ['text']", stage="interface", name=name)
    raise SystemExit
cases = [('A man, a plan, a canal: Panama', True), ('hello', False), ('', True), ('a', True), ('Ab,ba', True), ('ab', False)]
bad = []
for given, want in cases:
    code, text, _ = rerun(attempt, source, {'text': given})
    if code != 0 or not same(text, want):
        bad.append({"input": given, "want": want, "got": text[:200]})
if bad:
    verdict(False, f"{len(bad)}/{len(cases)} 个输入不对", stage="wrong", name=name, bad=bad)
    raise SystemExit
missing = reported(attempt, [])
verdict(not missing, "全部对，W0 转述对" if not missing else f"主体对，W0 的转述里没有 {missing}", stage="report", name=name)
