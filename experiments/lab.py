"""实验的运行器与记录抽取。机制与记录格式见 docs/Experiments.md。

用法（在仓库根）：
  python3 experiments/lab.py run --batch 名字 --provider 模型配置 [--tasks a,b] [--repeat N]
  python3 experiments/lab.py report --batch 名字
  python3 experiments/lab.py check  --batch 名字        重新检查已有的尝试，不再调模型
"""
import argparse
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
FAULT_MARKERS = ("宿主故障", "被宿主停下", "有 Instance 被宿主停下")
MAX_RESUME = 3

# 容器与镜像的装配：与 crates/host/tests/support 里的一致，实验用同样的限额。
DIRECTORY = {
    "docker": "/usr/bin/docker", "python": "/usr/bin/python3",
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
    return task


def commit(repo):
    out = subprocess.run(["git", "-C", str(repo), "rev-parse", "--short", "HEAD"], capture_output=True, text=True)
    return out.stdout.strip() or "?"


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
    """跑一次 croon-app，返回 (退出码, 输出)。输出同时追加进日志。"""
    result = subprocess.run([str(APP)] + args, capture_output=True, text=True, timeout=1800)
    text = result.stdout + result.stderr
    with open(log, "a") as f:
        f.write(f"$ croon-app {' '.join(args[:1])}\n{text}\n")
    return result.returncode, text


def is_host_fault(text):
    return any(m in text for m in FAULT_MARKERS)


def attempt_run(task, provider, attempt):
    """一次尝试：run，遇宿主故障就 resume，最多 MAX_RESUME 次。返回 (进程输出, 宿主故障次数, 是否宿主故障收场)。"""
    attempt.mkdir(parents=True)
    system = f"lab-{task['name']}-{attempt.name}"
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
    code, text = run_app(["run", "--config", str(config), "--repo", str(repo), "--out", str(out),
                          "--tools", str(tools), task["task"]], log)
    faults = 0
    while code != 0 and is_host_fault(text) and faults < MAX_RESUME:
        faults += 1
        if out.exists() and not any(out.iterdir()):
            out.rmdir()
        code, text = run_app(["resume", "--config", str(config), "--out", str(out)], log)
    return code, text, faults


def parse_tape(cassette):
    """从录像取出：模型调用数、token、每次 make 的尝试、各工具调用的序列。"""
    if not cassette.exists():
        return {"model_calls": 0, "tokens": {"input": 0, "output": 0}, "makes": [], "calls": []}
    entries = json.loads(cassette.read_text())["entries"]
    tokens = {"input": 0, "output": 0}
    for e in entries:
        usage = e["outcome"].get("value", {}).get("usage", {})
        tokens["input"] += usage.get("input") or 0
        tokens["output"] += usage.get("output") or 0
    # 录像里 W0 的调用最多、最后一条请求的 messages 包含整段历史。取消息最长的 W0 请求。
    w0 = [e for e in entries if "W0" in e["request"]["system"]]
    last = max(w0, key=lambda e: len(e["request"]["messages"]))["request"]["messages"] if w0 else []
    final = max(w0, key=lambda e: len(e["request"]["messages"]))["outcome"].get("value", {}) if w0 else {}
    pending = {}
    calls, makes = [], []
    for m in last + ([{"role": "assistant", "calls": final.get("calls", []), "text": final.get("text", "")}] if final else []):
        if m["role"] == "assistant":
            for c in m["calls"]:
                pending[c["id"]] = c
                calls.append({"name": c["name"], "input": c["input"]})
        elif m["role"] == "tool":
            for r in m["results"]:
                c = pending.get(r["id"])
                if c and c["name"] == "make":
                    err = r["output"] if r["is_error"] else None
                    makes.append({"name": c["input"].get("name"), "source": c["input"].get("source", ""),
                                  "ok": not r["is_error"],
                                  "error": {"kind": err.get("error"), "message": err.get("message")} if isinstance(err, dict) else None})
                    calls[-1:] = calls[-1:]
                if c:
                    for k in calls:
                        if k["name"] == c["name"] and "result" not in k and k["input"] == c["input"]:
                            k["result"] = {"is_error": r["is_error"], "output": r["output"]}
                            break
    return {"model_calls": len(entries), "tokens": tokens, "makes": makes, "calls": calls}


def stage_of(tape, passed, host_fault):
    """失败发生在哪个阶段：看工具调用的序列。"""
    if passed:
        return ""
    if host_fault:
        return "host"
    calls = tape["calls"]
    if not any(c["name"] == "make" for c in calls):
        return "look" if any(c["name"] == "look" for c in calls) else "none"
    if tape["makes"] and not any(m["ok"] for m in tape["makes"]):
        return "construct"
    made = [m["name"] for m in tape["makes"] if m["ok"]]
    ran = [c for c in calls if c["name"] in made]
    if not ran:
        return "run"
    if any(c.get("result", {}).get("is_error") for c in ran):
        return "run"
    return "answer"


def check(task, attempt, tape):
    """调用任务的检查器，返回判定；检查器自己出错记为 checker_fault。"""
    script = task["path"] / "check.py"
    if not script.exists():
        return {"passed": None, "reason": "没有检查器", "details": {}}, False
    try:
        result = subprocess.run([sys.executable, str(script), str(attempt)], capture_output=True, text=True, timeout=900)
        verdict = json.loads(result.stdout.strip().splitlines()[-1])
        return verdict, False
    except Exception as e:  # 检查器出错不记为任务失败
        return {"passed": None, "reason": f"检查器故障：{e}", "details": {}}, True


def record_of(task, provider_model, attempt, tape, code, text, faults, seconds):
    host_fault = code != 0 and is_host_fault(text)
    verdict, checker_broken = ({"passed": False, "reason": "宿主故障", "details": {}}, False)
    if code == 0:
        verdict, checker_broken = check(task, attempt, tape)
    passed = bool(verdict.get("passed"))
    if checker_broken:
        outcome = "checker_fault"
    elif host_fault:
        outcome = "host_fault"
    elif passed:
        outcome = "passed"
    else:
        outcome = "failed"
    return {
        "task": task["name"], "model": provider_model, "commit": {"app": commit(ROOT), "croon": commit(ROOT.parent / "croon"), "lang0": commit(ROOT.parent / "croon-lang0")},
        "outcome": outcome, "stage": stage_of(tape, passed, host_fault),
        "model_calls": tape["model_calls"], "tokens": tape["tokens"],
        "makes": [{k: v for k, v in m.items() if k != "source"} | {"source_bytes": len(m["source"])} for m in tape["makes"]],
        "host_faults": faults, "seconds": round(seconds, 1),
        "exit": code, "verdict": verdict,
    }


def cmd_run(args):
    provider = pathlib.Path(args.provider).resolve()
    model = json.loads(provider.read_text())["model"]
    names = args.tasks.split(",") if args.tasks else sorted(p.name for p in TASKS.iterdir() if (p / "task.json").exists())
    batch = RUNS / args.batch
    for name in names:
        task = load_task(name)
        for n in range(args.repeat):
            attempt = batch / name / f"{n + 1:02d}"
            if attempt.exists():
                print(f"{name}/{n + 1:02d} 已有，跳过")
                continue
            started = time.time()
            code, text, faults = attempt_run(task, provider, attempt)
            tape = parse_tape(attempt / "cassette.json")
            rec = record_of(task, model, attempt, tape, code, text, faults, time.time() - started)
            (attempt / "record.json").write_text(json.dumps(rec, ensure_ascii=False, indent=1))
            (attempt / "makes.json").write_text(json.dumps(tape["makes"], ensure_ascii=False, indent=1))
            print(f"{name}/{n + 1:02d} {rec['outcome']:13} 阶段={rec['stage'] or '-':9} 调用={rec['model_calls']:3} make={len(rec['makes'])} 宿主故障={faults} {rec['seconds']}s  {rec['verdict'].get('reason', '')[:60]}")


def cmd_check(args):
    batch = RUNS / args.batch
    for rec_path in sorted(batch.glob("*/*/record.json")):
        attempt = rec_path.parent
        task = load_task(attempt.parent.name)
        tape = parse_tape(attempt / "cassette.json")
        old = json.loads(rec_path.read_text())
        rec = record_of(task, old["model"], attempt, tape, old["exit"], "", old["host_faults"], old["seconds"])
        rec["commit"] = old["commit"]
        rec_path.write_text(json.dumps(rec, ensure_ascii=False, indent=1))
        print(f"{attempt.parent.name}/{attempt.name} {rec['outcome']:13} {rec['verdict'].get('reason', '')[:70]}")


def cmd_report(args):
    batch = RUNS / args.batch
    by_task = {}
    for rec_path in sorted(batch.glob("*/*/record.json")):
        by_task.setdefault(rec_path.parent.parent.name, []).append(json.loads(rec_path.read_text()))
    print(f"批 {args.batch}")
    for name, recs in by_task.items():
        counts = {}
        for r in recs:
            counts[r["outcome"]] = counts.get(r["outcome"], 0) + 1
        stages = {}
        for r in recs:
            if r["stage"]:
                stages[r["stage"]] = stages.get(r["stage"], 0) + 1
        makes = [len(r["makes"]) for r in recs]
        calls = [r["model_calls"] for r in recs]
        print(f"- {name}：{len(recs)} 次 {counts}；失败阶段 {stages or '无'}；make 平均 {sum(makes) / len(makes):.1f}；模型调用 {min(calls)}–{max(calls)}")
    total_in = sum(r["tokens"]["input"] for rs in by_task.values() for r in rs)
    total_out = sum(r["tokens"]["output"] for rs in by_task.values() for r in rs)
    print(f"token 合计：输入 {total_in}，输出 {total_out}")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("run", "check", "report"):
        p = sub.add_parser(name)
        p.add_argument("--batch", required=True)
        if name == "run":
            p.add_argument("--provider", required=True)
            p.add_argument("--tasks")
            p.add_argument("--repeat", type=int, default=1)
    args = parser.parse_args()
    {"run": cmd_run, "check": cmd_check, "report": cmd_report}[args.cmd](args)


if __name__ == "__main__":
    main()
