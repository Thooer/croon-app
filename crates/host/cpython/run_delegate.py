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


def file_entry(name, text):
    """家目录里的一个文件：croon.read_json 读它的 Bundle 描述，再读 content。"""
    return {f"sha256:f-{name}": {"value": {"content": f"sha256:c-{name}", "dependencies": []}},
            f"sha256:c-{name}": {"value": text}}


def home_contents(tools_json, s_source):
    """W0 读的家目录：顶层 Tree 里有 tools.json 与 library/，library/ 里有 s.py。"""
    out = {"sha256:lhome": {"value": {"content": "sha256:lhome-tree", "dependencies": []}},
           "sha256:lhome-tree": {"value": {"tools.json": {"file": "sha256:f-tools", "x": False},
                                          "library": {"dir": "sha256:llibdir"}}},
           "sha256:llibdir": {"value": {"content": "sha256:llibdir-tree", "dependencies": []}},
           "sha256:llibdir-tree": {"value": {"s.py": {"file": "sha256:f-s", "x": False}}}}
    # construct 返回的 s：W0 交办前读它的 Interface。
    out["sha256:ls"] = {"value": {"content": "sha256:lscontent", "dependencies": []}}
    out["sha256:lscontent"] = {"value": {"language": "lang0@1", "interface": S_INTERFACE,
                                         "body": {"source": S_SOURCE, "modules": "sha256:lhome"}}}
    out.update(file_entry("tools", tools_json))
    out.update(file_entry("s", s_source))
    return out


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
S_SOURCE = (ROOT / "library" / "s.py").read_text()
# 家目录里的工具清单：规范文本，与宿主放进家目录的一致。
TOOLS_JSON = json.loads((ROOT / "home" / "tools.json").read_text())
S_INTERFACE = values["sha256:ls"]["$bundle"]["interface"]
S_TOOL_INPUT = next(t["input"] for t in values["sha256:lw0send0"]["tools"] if t["name"] == "s")
S_IN = {"task": "修好 a.py", "snapshot": "sha256:lrepo"}
SCRIPTS = {
    "s": ({"0": llm("", [{"id": "c1", "name": "write_file", "input": {"path": "a.py", "content": "fixed\n"}}]),
           "1": done_exec(snapshot="sha256:lrepo2"),
           "2": llm("改好了 a.py", [])},
          S_IN, {"engine": "port", "exec": "port"}, {}),
    # W0：启动时读清单，把清单里的 s 构造出来载入（读源码、infer、pack、construct），模型直接调用 s：实例化、登记、取、diff，再 merge。
    "w0": ({"0": ok({"interface": S_INTERFACE, "tool_input": S_TOOL_INPUT}),
            "1": ok({"id": "sha256:ls"}),
            "2": ok({"id": "sha256:ls"}),
            "3": llm("", [{"id": "c1", "name": "s", "input": {"task": "修好 a.py"}}]),
            "4": ok(None, attach={"look": {"handle": "h-slook", "spec": "sha256:pslook"},
                                  "notify": {"handle": "h-snotify", "spec": "sha256:psnotify"},
                                  "correct": {"handle": "h-scorrect", "spec": "sha256:pscorrect"}},
                    derived={"engine": {"handle": "h-sengine", "revoker": rev(5)}, "exec": {"handle": "h-sexec", "revoker": rev(6)}}),
            "5": ok(None, derived={"to": {"handle": "h-sdone", "revoker": rev(7)}}),
            "6": {"arrival": {"port": "done", "sender": "i-s", "via": "h-sdone", "delivery": {"value": {"ok": True, "value": S_OUT, "dependencies": ["sha256:lrepo2"]}, "dependencies": ["sha256:lrepo2"]}, "attach": {}}},
            "7": done_exec(" a.py | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n"),
            "8": llm("", [{"id": "c2", "name": "merge", "input": {"name": "s"}}]),
            "9": llm("已修好 a.py", [])},
           {"task": "修好 a.py", "home": "sha256:lhome", "snapshot": "sha256:lrepo"},
           {"engine": "port", "exec": "port", "instantiate": "port", "pack": "port", "construct": "port", "infer": "port", "back": "port"}, {"done": "inbox"}),
}

out = {}
for name, (results, inp, refs, inboxes) in SCRIPTS.items():
    source = (work / "library" / f"{name}.py").read_text()
    iface = croon.interface(source, loader)
    croon.DRIVER = croon.Driver(results, contents=home_contents(TOOLS_JSON, S_SOURCE), birth={"definition": {"body": {"modules": "sha256:lhome"}}, "seat": None, "roster": None})
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
