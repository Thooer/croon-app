"""外部调用方把一个目录交给 W0 写过的 fix_add_agent。需要模型服务与任务镜像。"""

import argparse
import json
from pathlib import Path
import subprocess
import sys

from call import bind


def repair_tool(binary, repo, out, config=None, env=None):
    source = Path(__file__).with_name("fix_add_agent.py")
    return bind(binary, "fix_add_agent", source, out, env, snapshot=repo, config=config)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--snapshot", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--config", type=Path, help="宿主配置文件")
    parser.add_argument("task", help="交给帮手的任务")
    args = parser.parse_args()
    try:
        tool, invoke = repair_tool(args.binary.resolve(), args.snapshot, args.out, args.config)
        print(json.dumps(tool, ensure_ascii=False), file=sys.stderr)
        print(json.dumps(invoke({"task": args.task}), ensure_ascii=False))
    except subprocess.CalledProcessError as exc:
        sys.stderr.write(exc.stderr)
        sys.exit(exc.returncode)
