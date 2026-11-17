import json, pathlib, sys
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
tape = json.loads((attempt / "cassette.json").read_text())["entries"]
inner = [e for e in tape if "W0" not in e["request"]["system"]]
answers = [e["outcome"]["value"]["text"].strip() for e in inner]
if len(answers) < 2:
    verdict(False, f"主体只调了 {len(answers)} 次模型，要两次", answers=answers)
    raise SystemExit
last_two = answers[-2:]
ok = last_two == ["正面", "负面"]
verdict(ok, "两次判断都对" if ok else f"判断不对：{last_two}", answers=last_two)
