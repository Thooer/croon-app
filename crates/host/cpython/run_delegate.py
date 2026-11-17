"""在 CPython 上独立求出 Delegate 场景里 S 与 W0 报出的 Atom 与结束值，并与场景预期逐条核对。

用法：在仓库根跑 python3 -B crates/host/cpython/run_delegate.py

- library/ 与生成模块都取自本仓库：生成模块由 `cargo run -p host --example generate_library` 写进临时目录。
- croon 的 CPython 参考实现取自并排放置的 croon-lang0（crates/lang0/cpython/croon.py），语义只有那一个出处。

预期是 block library.scenario.delegate（`cargo run --example block` 展开 `$block` 后读它），它检验什么见 docs/Library/Scenarios/Delegate.md；Kernel 与 Lang0 是被测实现，这里不用它们。
一致时打印 ok；不一致时打印差异并以非零退出。
"""
import importlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]
LANG0 = ROOT.parent / "croon-lang0" / "crates" / "lang0" / "cpython"
# 场景与 croon 的共享场景用同一个名字空间：本场景没定义的占位符（如 sha256:empty）在那里。
SHARED = "scenario.reconnect"

work = pathlib.Path(tempfile.mkdtemp())
shutil.copytree(ROOT / "library", work / "library")
subprocess.run(["cargo", "run", "-q", "--offline", "-p", "host", "--example", "generate_library", "--",
                str(work / "library")], cwd=ROOT, check=True)
sys.path[:0] = [str(work), str(LANG0)]
import croon  # noqa: E402


def loader(name):
    if name not in croon._LOADED:
        croon._LOADED[name] = croon._Module((work / (name.replace(".", "/") + ".py")).read_text(), name, loader)
    return croon._LOADED[name]


croon.LOADER = loader


def block(name):
    """一份 block，`$block` 已展开；场景的 block 在 croon 的 scenarios crate。"""
    example = "block" if name.startswith("library.") else "block_shared"
    out = subprocess.run(["cargo", "run", "-q", "--offline", "-p", "host", "--example", example, "--", name],
                         cwd=ROOT, check=True, capture_output=True, text=True).stdout
    return json.loads(out)


# 场景的预期：S 的 Definition 在里面，W0 交办前要读它的 Interface。
expected = block("library.scenario.delegate")
values = {**block(SHARED)["values"], **expected["values"]}


def contents_of(bundle_id, content_id):
    """一份 Bundle 的描述与它的 content：croon.read_json 读两层。"""
    spec = values[bundle_id]["$bundle"]
    dependencies = values[bundle_id]["dependencies"]
    return {bundle_id: {"value": {"content": content_id, "dependencies": dependencies}},
            content_id: {"value": {"language": spec["language"], "interface": spec["interface"], "body": spec["body"]}}}


def ok(value, attach=None, derived=None, dependencies=[]):
    return {"outcome": {"ok": True, "value": value, "dependencies": dependencies}, "attach": attach or {}, "derived": derived or {}, "accepted": True}


def llm(text, calls):
    return ok({"text": text, "reasoning": None, "calls": calls, "finish": "tool_calls" if calls else "stop",
               "usage": {"input": 1, "output": 1}})


def done_exec(stdout="", snapshot="sha256:lrepo"):
    return ok({"exit": 0, "stdout": stdout, "stderr": "", "timed_out": False, "truncated": False, "snapshot": snapshot}, dependencies=[snapshot])


def rev(n):
    return {"handle": f"h-rev{n}", "spec": "sha256:psrevoke"}


S_OUT = {"summary": "改好了 a.py", "snapshot": "sha256:lrepo2"}
SCRIPTS = {
    "s": ({"0": llm("", [{"id": "c1", "name": "write_file", "input": {"path": "a.py", "content": "fixed\n"}}]),
           "1": done_exec(snapshot="sha256:lrepo2"),
           "2": llm("改好了 a.py", [])},
          {"task": "修好 a.py", "snapshot": "sha256:lrepo"}, {"engine": "port", "exec": "port"}, {}),
    "w0": ({"0": llm("", [{"id": "c1", "name": "delegate", "input": {"agent": "s", "input": {"task": "修好 a.py"}}}]),
            "1": ok(None, attach={"look": {"handle": "h-slook", "spec": "sha256:pslook"},
                                  "notify": {"handle": "h-snotify", "spec": "sha256:psnotify"},
                                  "correct": {"handle": "h-scorrect", "spec": "sha256:pscorrect"}},
                    derived={"engine": {"handle": "h-sengine", "revoker": rev(5)}, "exec": {"handle": "h-sexec", "revoker": rev(6)}}),
            "2": ok(None, derived={"to": {"handle": "h-sdone", "revoker": rev(7)}}),
            "3": {"arrival": {"port": "done", "sender": "i-s", "via": "h-sdone", "delivery": {"value": {"ok": True, "value": S_OUT, "dependencies": ["sha256:lrepo2"]}, "dependencies": ["sha256:lrepo2"]}, "attach": {}}},
            "4": done_exec(" a.py | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n"),
            "5": llm("", [{"id": "c2", "name": "merge", "input": {"name": "s"}}]),
            "6": llm("已修好 a.py", [])},
           {"task": "修好 a.py", "worker": "sha256:ls", "snapshot": "sha256:lrepo"},
           {"engine": "port", "exec": "port", "instantiate": "port", "pack": "port", "construct": "port", "infer": "port", "back": "port"}, {"done": "inbox"}),
}

out = {}
for name, (results, inp, refs, inboxes) in SCRIPTS.items():
    source = (work / "library" / f"{name}.py").read_text()
    iface = croon.interface(source, loader)
    croon.DRIVER = croon.Driver(results, contents=contents_of("sha256:ls", "sha256:lscontent"), birth={"definition": {"body": {"modules": "sha256:lrepo"}}, "seat": None, "roster": None})
    mod = importlib.import_module(f"library.{name}")
    args = dict(inp)
    for k in refs:
        args[k] = croon.Ref({"port": k}, iface["requires"][k])
    for k in inboxes:
        args[k] = croon.InboxRef(k)
    try:
        delivery = mod.main(**args)
        end = {"ok": True, "value": delivery.value, "dependencies": delivery.dependencies}
    except croon.Failure as e:
        end = {"ok": False, "error": {"kind": e.kind_, "message": str(e), "data": e.data_}, "dependencies": []}
    out[name] = {"interface": iface, "atoms": croon.DRIVER.atoms, "end": end}

# ---- 与场景预期核对 ----

def resolve(v):
    """值的占位符换成它的内容；`$block` 只会指向本场景的 values。"""
    if isinstance(v, str) and v in values:
        return values[v]
    if isinstance(v, dict) and set(v) == {"$block"}:
        return values[v["$block"].split("#/values/", 1)[1]]
    return v


problems = []
for name, instance in (("s", "i-s"), ("w0", "i-w0")):
    want, end_want = [], None
    for record in expected["journals"][instance]:
        what = record["what"]
        if "send" in what:
            want.append({"send": {"to": what["send"]["to"], "value": resolve(what["send"]["delivery"]["value"])}})
        elif "receive" in what:
            want.append({"receive": what["receive"]["filter"]})
        elif "end" in what:
            end_want = resolve(what["end"])
    got = [{"send": {"to": a["send"]["to"], "value": a["send"]["delivery"]["value"]}} if "send" in a
           else {"receive": a["receive"]} for a in out[name]["atoms"]]
    if got != want:
        problems.append({"instance": instance, "atoms": {"got": got, "want": want}})
    if out[name]["end"] != end_want:
        problems.append({"instance": instance, "end": {"got": out[name]["end"], "want": end_want}})

if problems:
    json.dump(problems, sys.stdout, ensure_ascii=False, indent=1)
    print()
    sys.exit(1)
print("ok")
