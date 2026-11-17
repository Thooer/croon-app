import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3]))
from experiments.checking import made_sources, rerun, verdict

attempt = pathlib.Path(sys.argv[1])
sources = made_sources(attempt)
if not sources:
    verdict(False, "没有构造成功的主体")
    raise SystemExit
# 检验方自己的输入：含负数、零、空列表，写死答案过不了。
cases = [([], 0), ([0], 0), ([-2, -3, 5, 8], 6), ([7, 9, 11], 0), ([100, 200, 3], 300)]
name, source = sources[-1]
bad = []
for numbers, want in cases:
    code, text, _ = rerun(attempt, source, {"numbers": numbers})
    if code != 0 or text.splitlines()[0].strip() != str(want):
        bad.append({"input": numbers, "want": want, "got": text[:200]})
verdict(not bad, "全部对" if not bad else f"{len(bad)}/{len(cases)} 个输入不对", name=name, bad=bad)
