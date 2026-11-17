import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3]))
from experiments.checking import made_sources, rerun, verdict

attempt = pathlib.Path(sys.argv[1])
sources = made_sources(attempt)
if not sources:
    verdict(False, "没有构造成功的主体")
    raise SystemExit
cases = [("a A a", 1), ("x y z", 3), ("", 0), ("  Hello   hello  world ", 2), ("one", 1)]
name, source = sources[-1]
# 输入字段名由模型起，从它的 Interface 取不到时就试它 main 的第一个参数名。
import re
m = re.search(r"def main\(\s*(\w+)", source)
field = m.group(1) if m else "text"
bad = []
for text_in, want in cases:
    code, text, _ = rerun(attempt, source, {field: text_in})
    if code != 0 or text.splitlines()[0].strip() != str(want):
        bad.append({"input": text_in, "want": want, "got": text[:200]})
verdict(not bad, "全部对" if not bad else f"{len(bad)}/{len(cases)} 个输入不对", name=name, bad=bad)
