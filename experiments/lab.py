"""实验的运行器与记录整理。机制、阶段与记录格式见 docs/Experiments.md。

用法（在仓库根）：
  python3 experiments/lab.py run    --batch 名字 --provider 模型配置 [--tasks a,b] [--set tune|holdout] [--repeat N] [--jobs J]
  python3 experiments/lab.py check  --batch 名字      对已有的尝试重新整理与判定，不再调模型
  python3 experiments/lab.py report --batch 名字
"""
import argparse
import concurrent.futures
import json
import pathlib
import shutil
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
TASKS = ROOT / "experiments" / "tasks"
RUNS = ROOT / "experiments" / "runs"
APP = ROOT / "target" / "debug" / "croon-app"
FAULT_MARKERS = ("宿主故障", "被宿主停下")
MAX_RESUME = 3
TIMEOUT = 1200  # 一次运行最多 20 分钟

# 容器与镜像的装配：与 crates/host/tests/support 里的一致，实验用同样的限额。
DIRECTORY = {
    "docker": shutil.which("docker") or "/usr/bin/docker", "python": sys.executable,
    "image": "croon-work:2", "collector_image": "python:3.12-slim", "platform": "linux/amd64",
    "cpus": "1", "memory_bytes": 536870912, "work_bytes": 67108864, "temporary_bytes": 67108864,
    "pids": 128, "output_bytes": 65536, "nodes": 20000, "archive_bytes": 134217728,
    "cleanup_timeout_seconds": 60,
}


def load_task(name):
    path = TASKS / name
    task = json.loads((path / "task.json").read_text())
    task["name"] = name
    task["path"] = path
    task.setdefault("set", "tune")
    return task


def commit(repo):
    out = subprocess.run(["git", "-C", str(repo), "rev-parse", "--short", "HEAD"], capture_output=True, text=True)
    return out.stdout.strip() or "?"


def dirty(repo, paths):
    out = subprocess.run(["git", "-C", str(repo), "status", "--porcelain", "--"] + paths, capture_output=True, text=True)
    return out.stdout.strip()


def host_config(attempt, system, provider, cassette):
    cfg = {
        "system": system,
        "kernel": str(attempt / "kernel"),
        "model": {"provider": str(provider), "record": str(cassette)},
        "directory": {"state": str(attempt / "state"), **DIRECTORY},
    }
    path = attempt / "host.json"
    path.write_text(json.dumps(cfg, indent=1))
    return path


def run_app(args, log):
    """跑一次 croon-app，返回 (退出码, 输出, 是否超时)。输出同时追加进日志。"""
    try:
        result = subprocess.run([str(APP)] + args, capture_output=True, text=True, timeout=TIMEOUT)
        text, code, timed_out = result.stdout + result.stderr, result.returncode, False
    except subprocess.TimeoutExpired as e:
        text, code, timed_out = (e.stdout or b"").decode() if isinstance(e.stdout, bytes) else (e.stdout or ""), 124, True
    with open(log, "a") as f:
        f.write(f"$ croon-app {args[0]}\n{text}\n")
    return code, text, timed_out


def is_host_fault(text):
    return any(m in text for m in FAULT_MARKERS)


def attempt_run(task, provider, attempt):
    """一次尝试：run，遇宿主故障就 resume，最多 MAX_RESUME 次。返回 (退出码, 输出, 宿主故障次数, 是否超时)。"""
    attempt.mkdir(parents=True)
    system = f"lab-{attempt.parent.parent.name}-{task['name']}-{attempt.name}"
    cassette = attempt / "cassette.json"
    config = host_config(attempt, system, provider, cassette)
    repo = attempt / "repo"
    if (task["path"] / "repo").exists():
        shutil.copytree(task["path"] / "repo", repo)
    else:
        repo.mkdir()
    tools = attempt / "tools.json"
    tools.write_text(json.dumps({"tools": task.get("tools", [{"kind": "functions", "package": "files"}])}))
    out = attempt / "out"
    log = attempt / "run.log"
    code, text, timed_out = run_app(["run", "--quiet", "--config", str(config), "--repo", str(repo), "--out", str(out),
                                     "--tools", str(tools), task["task"]], log)
    faults = 0
    while code != 0 and not timed_out and is_host_fault(text) and faults < MAX_RESUME:
        faults += 1
        if out.exists() and not any(out.iterdir()):
            out.rmdir()
        code, text, timed_out = run_app(["resume", "--quiet", "--config", str(config), "--out", str(out)], log)
    return code, text, faults, timed_out


def built_sources(kernel):
    """Journal 里 W0 实际交给 definition.infer 的源码，按先后：造出来的主体的真实源码，不论源码是写在 make 的参数里，
    还是 make(path=…) 从工作区读来的。键是 infer 请求的值里 body.source。"""
    import sqlite3
    db = pathlib.Path(kernel) / "journal.db"
    if not db.exists():
        return []
    values = pathlib.Path(kernel) / "values"
    conn = sqlite3.connect(db)
    out = []
    for body, in conn.execute("select body from records order by instance, seq"):
        send = json.loads(body)["what"].get("send") or {}
        if (send.get("to") or {}).get("port") != "infer":
            continue
        h = send["delivery"]["value"].split(":")[1]
        try:
            value = json.loads((values / h[:2] / h[2:]).read_bytes())
        except Exception:
            continue
        source = (value.get("body") or {}).get("source")
        if source is not None:
            out.append({"source": source, "ok": bool(send["result"]["outcome"]["ok"])})
    return out


def digest(cassette):
    """从录像整理这次尝试的经过。

    W0 与造出来的主体各自的模型调用：W0 的请求工具表里有 make，主体的没有。W0 的调用序列取它最长的那条请求
    （含整段历史）加上最后一条的回答。返回的结构写进 digest.json，检查器读它。
    """
    empty = {"model_calls": {"w0": 0, "made": 0}, "tokens": {"w0": {"input": 0, "output": 0}, "made": {"input": 0, "output": 0}},
             "calls": [], "makes": [], "made_outputs": [], "final_text": "", "turns": 0}
    if not cassette.exists():
        return empty
    entries = json.loads(cassette.read_text())["entries"]
    w0 = [e for e in entries if any(t["name"] == "make" for t in e["request"]["tools"])]
    made = [e for e in entries if not any(t["name"] == "make" for t in e["request"]["tools"])]
    out = json.loads(json.dumps(empty))
    for key, group in (("w0", w0), ("made", made)):
        out["model_calls"][key] = len(group)
        for e in group:
            usage = e["outcome"].get("value", {}).get("usage", {})
            out["tokens"][key]["input"] += usage.get("input") or 0
            out["tokens"][key]["output"] += usage.get("output") or 0
    out["made_outputs"] = [e["outcome"]["value"]["text"] for e in made if "value" in e["outcome"]]
    if not w0:
        return out
    longest = max(w0, key=lambda e: len(e["request"]["messages"]))
    history = list(longest["request"]["messages"])
    final = longest["outcome"].get("value", {})
    history.append({"role": "assistant", "text": final.get("text", ""), "calls": final.get("calls", [])})
    out["final_text"] = final.get("text", "") if not final.get("calls") else ""
    out["turns"] = sum(1 for m in history if m["role"] == "assistant")
    by_id = {}
    for m in history:
        if m["role"] == "assistant":
            for c in m["calls"]:
                item = {"name": c["name"], "input": c["input"]}
                by_id[c["id"]] = item
                out["calls"].append(item)
        elif m["role"] == "tool":
            for r in m["results"]:
                item = by_id.get(r["id"])
                if item is None:
                    continue
                item["is_error"] = r["is_error"]
                item["output"] = r["output"]
                if item["name"] == "make":
                    err = r["output"] if r["is_error"] and isinstance(r["output"], dict) else {}
                    out["makes"].append({"name": item["input"].get("name"), "source": item["input"].get("source", ""),
                                         "ok": not r["is_error"],
                                         "error": {"kind": err.get("error"), "message": err.get("message")} if err else None,
                                         "tool": r["output"].get("tool") if not r["is_error"] and isinstance(r["output"], dict) else None})
    return out


def stage_of(d, verdict, host_fault, timed_out, turns_limit):
    """失败阶段：按 Experiments.md 的表，取第一个符合的。检查器给了阶段就用它。"""
    if timed_out:
        return "timeout"
    if host_fault:
        return "host"
    if verdict.get("passed"):
        return ""
    if verdict.get("stage"):
        return verdict["stage"]
    if d["turns"] >= turns_limit:
        return "turns"
    made = [m for m in d["makes"] if m["ok"]]
    if not d["makes"] or not made:
        return "construct" if d["makes"] else "no_make"
    tools = {m["tool"] for m in made}
    ran = [c for c in d["calls"] if c["name"] in tools]
    if not ran or all(c.get("is_error") for c in ran):
        return "invoke"
    return "wrong"


def check(task, attempt):
    """调用任务的检查器，返回 (判定, 检查器是否出错)。"""
    script = task["path"] / "check.py"
    if not script.exists():
        return {"passed": None, "reason": "没有检查器", "details": {}}, True
    try:
        result = subprocess.run([sys.executable, str(script), str(attempt)], capture_output=True, text=True, timeout=900)
        return json.loads(result.stdout.strip().splitlines()[-1]), False
    except Exception as e:  # 检查器出错不记为任务失败
        return {"passed": None, "reason": f"检查器故障：{e}", "details": {}}, True


def judge(task, model, attempt, d, code, faults, timed_out, seconds):
    host_fault = code != 0 and not timed_out and is_host_fault((attempt / "run.log").read_text() if (attempt / "run.log").exists() else "")
    failed_run = code != 0
    if failed_run:
        # W0 自己以失败结束（如模型服务一直不可用）不是任务没做对：记下失败的 kind 与 message，阶段是 run_failed。
        tail = ""
        if (attempt / "run.log").exists():
            lines = [l for l in (attempt / "run.log").read_text().splitlines() if l.startswith("运行失败")]
            tail = lines[-1][:300] if lines else ""
        verdict, broken = {"passed": False, "stage": "run_failed", "reason": f"运行没有正常结束：{tail}"[:160], "details": {"tail": tail}}, False
    else:
        verdict, broken = check(task, attempt)
    direct = sum(1 for c in d["calls"] if c["name"] in ("write_file", "run"))
    # 绕开：结果对，但 W0 没有靠主体：files 类里 W0 自己写了文件，且没有成功造出主体。
    if not failed_run and not broken and verdict.get("passed") and task.get("category") == "files":
        wrote = any(c["name"] == "write_file" for c in d["calls"])
        if wrote and not any(m["ok"] for m in d["makes"]):
            verdict = {**verdict, "passed": False, "stage": "bypass", "reason": "结果对，但 W0 自己改的文件，没有造主体"}
    passed = bool(verdict.get("passed"))
    outcome = "checker_fault" if broken and not failed_run else "host_fault" if (host_fault or timed_out) else "passed" if passed else "failed"
    return {
        "task": task["name"], "set": task["set"], "category": task.get("category"), "model": model,
        "commit": {"app": commit(ROOT), "croon": commit(ROOT.parent / "croon"), "lang0": commit(ROOT.parent / "croon-lang0")},
        "outcome": outcome, "stage": stage_of(d, verdict, host_fault, timed_out, 50) if outcome != "checker_fault" else "",
        "model_calls": d["model_calls"], "tokens": d["tokens"],
        "makes": [{k: v for k, v in m.items() if k != "source"} | {"source_bytes": len(m["source"])} for m in d["makes"]],
        "w0_direct": direct, "host_faults": faults, "seconds": round(seconds, 1), "exit": code, "verdict": verdict,
    }


def finish(task, model, attempt, code, faults, timed_out, seconds):
    d = digest(attempt / "cassette.json")
    # make 的源码以 Journal 里 definition.infer 收到的为准：make(path=…) 的参数里没有源码。
    # 每次 make 调用恰好对应一次 infer（失败的 make 若在 infer 之前就失败，如没有这个文件，则没有对应项），
    # 所以按「读了源码的 make」的先后一一对应；给了 source 的 make 本来就有源码，不用对应。
    infers = [b["source"] for b in built_sources(attempt / "kernel")]
    k = 0
    for m in d["makes"]:
        if m["source"] != "":
            k += 1
            continue
        if k < len(infers):
            m["source"] = infers[k]
            k += 1
    (attempt / "digest.json").write_text(json.dumps(d, ensure_ascii=False, indent=1))
    (attempt / "makes.json").write_text(json.dumps(d["makes"], ensure_ascii=False, indent=1))
    rec = judge(task, model, attempt, d, code, faults, timed_out, seconds)
    (attempt / "record.json").write_text(json.dumps(rec, ensure_ascii=False, indent=1))
    return rec


def line(name, n, rec):
    mk = rec["makes"]
    return (f"{name}/{n} {rec['outcome']:13} 阶段={rec['stage'] or '-':9} W0调用={rec['model_calls']['w0']:3} 主体调用={rec['model_calls']['made']:2} "
            f"make={len(mk)}(败{sum(1 for m in mk if not m['ok'])}) W0写={rec['w0_direct']} 宿主故障={rec['host_faults']} {rec['seconds']}s  {rec['verdict'].get('reason', '')[:50]}")


def one(args_):
    task, provider, model, attempt = args_
    started = time.time()
    code, text, faults, timed_out = attempt_run(task, provider, attempt)
    rec = finish(task, model, attempt, code, faults, timed_out, time.time() - started)
    return task["name"], attempt.name, rec


def cmd_run(args):
    provider = pathlib.Path(args.provider).resolve()
    model = json.loads(provider.read_text())["model"]
    if args.allow_dirty is False:
        d = dirty(ROOT, ["crates", "library", "home", "docs/Library"])
        if d:
            sys.exit(f"App 有未提交的改动，批要在干净的代码上跑（--allow-dirty 跳过）：\n{d}")
    names = args.tasks.split(",") if args.tasks else sorted(p.name for p in TASKS.iterdir() if (p / "task.json").exists() and not p.name.startswith("_"))
    jobs = []
    batch = RUNS / args.batch
    for name in names:
        task = load_task(name)
        if not args.tasks and task["set"] != args.set:
            continue
        for n in range(args.repeat):
            attempt = batch / name / f"{n + 1:02d}"
            if attempt.exists():
                print(f"{name}/{n + 1:02d} 已有，跳过")
                continue
            jobs.append((task, provider, model, attempt))
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for name, n, rec in pool.map(one, jobs):
            print(line(name, n, rec), flush=True)


def cmd_check(args):
    batch = RUNS / args.batch
    for rec_path in sorted(batch.glob("*/*/record.json")):
        attempt = rec_path.parent
        task = load_task(attempt.parent.name)
        old = json.loads(rec_path.read_text())
        rec = finish(task, old["model"], attempt, old["exit"], old["host_faults"], False, old["seconds"])
        rec["commit"] = old["commit"]
        rec_path.write_text(json.dumps(rec, ensure_ascii=False, indent=1))
        print(line(attempt.parent.name, attempt.name, rec))


def cmd_report(args):
    batch = RUNS / args.batch
    by_task = {}
    for rec_path in sorted(batch.glob("*/*/record.json")):
        by_task.setdefault(rec_path.parent.parent.name, []).append(json.loads(rec_path.read_text()))
    print(f"批 {args.batch}")
    errors = {}
    for name, recs in by_task.items():
        counts, stages = {}, {}
        for r in recs:
            counts[r["outcome"]] = counts.get(r["outcome"], 0) + 1
            if r["stage"]:
                stages[r["stage"]] = stages.get(r["stage"], 0) + 1
            for m in r["makes"]:
                if not m["ok"] and m["error"]:
                    key = f"{m['error']['kind']}: {(m['error']['message'] or '')[:70]}"
                    errors[key] = errors.get(key, 0) + 1
        makes = [len(r["makes"]) for r in recs]
        w0 = [r["model_calls"]["w0"] for r in recs]
        bypass = sum(1 for r in recs if r["w0_direct"] and r["category"] == "files")
        print(f"- {name}（{recs[0]['set']}，{recs[0]['category']}）：{len(recs)} 次 {counts}；失败阶段 {stages or '无'}；"
              f"make 平均 {sum(makes) / len(makes):.1f}；W0 调用 {min(w0)}–{max(w0)}；W0 自己写文件或跑命令 {bypass} 次")
    if errors:
        print("make 的失败信息：")
        for k, v in sorted(errors.items(), key=lambda kv: -kv[1]):
            print(f"  {v:3}  {k}")
    total = {"w0": {"input": 0, "output": 0}, "made": {"input": 0, "output": 0}}
    for rs in by_task.values():
        for r in rs:
            for who in total:
                for k in total[who]:
                    total[who][k] += r["tokens"][who][k]
    print(f"token：W0 输入 {total['w0']['input']} 输出 {total['w0']['output']}；主体 输入 {total['made']['input']} 输出 {total['made']['output']}")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("run", "check", "report"):
        p = sub.add_parser(name)
        p.add_argument("--batch", required=True)
        if name == "run":
            p.add_argument("--provider", required=True)
            p.add_argument("--tasks")
            p.add_argument("--set", default="tune", choices=["tune", "holdout"])
            p.add_argument("--repeat", type=int, default=1)
            p.add_argument("--jobs", type=int, default=1)
            p.add_argument("--allow-dirty", action="store_true")
    args = parser.parse_args()
    {"run": cmd_run, "check": cmd_check, "report": cmd_report}[args.cmd](args)


if __name__ == "__main__":
    main()
