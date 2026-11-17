import json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3]))
from experiments.checking import made, rerun, reported, same, signature, uses_model, verdict, load

attempt = pathlib.Path(sys.argv[1])
sources = made(attempt)
if not sources:
    verdict(False, "没有构造成功的主体")
    raise SystemExit
name, source = sources[-1]
if signature(source) != ['numbers']:
    verdict(False, f"接口与任务写明的不符：参数是 {signature(source)}，要 ['numbers']", stage="interface", name=name)
    raise SystemExit
cases = [([3, 1, 3, 2, 1, 4], [3, 1, 2, 4]), ([], []), ([5, 5, 5], [5]), ([1, 2, 3], [1, 2, 3]), ([-1, -1, 0, 0], [-1, 0])]
bad = []
for given, want in cases:
    code, text, _ = rerun(attempt, source, {'numbers': given})
    if code != 0 or not same(text, want):
        bad.append({"input": given, "want": want, "got": text[:200]})
if bad:
    verdict(False, f"{len(bad)}/{len(cases)} 个输入不对", stage="wrong", name=name, bad=bad)
    raise SystemExit
# W0 的转述里要有去重后的四个数，按它们在文字里出现的先后（不要求格式）。
final = load(attempt)["final_text"]
import re
nums = [int(n) for n in re.findall(r"-?\d+", final)]
ok_report = any(nums[i:i + 4] == [3, 1, 2, 4] for i in range(len(nums)))
missing = [] if ok_report else ["3, 1, 2, 4"]
verdict(not missing, "全部对，W0 转述对" if not missing else f"主体对，W0 的转述里没有 {missing}", stage="report", name=name)
