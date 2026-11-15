"""W0：根的第一个 Agent。改代码，或交办给 worker，审过交回的再并入。"""
from typing import TypedDict
import croon
from croon import Inbox, Port, receive
from library.kernel import Instantiate, Outcome
from library.model import Generate
from library.directory import Exec
from library.files import file_tools
from library.react import react
from library.workspace import workspace, execute, fork, offer, adopt

Out = TypedDict("Out", {"summary": str, "snapshot": str})
Report = TypedDict("Report", {"name": str, "summary": str, "diff": str})
Done = Port(Outcome, doc="收结束状态")

SYSTEM = "你是 W0。能自己改，也能把一处修改交办出去；交回的先看 diff，满意再并入。"


def main(task: str, worker: str, snapshot: str, engine: Generate, exec: Exec, instantiate: Instantiate,
         back: Done, done: Inbox(Done)) -> Out:
    """改仓库里的代码，必要时交办给 worker。

    Args:
        task: 要做的事。
        worker: 交办时实例化的 Definition 的 ID。
    """
    ws = workspace(snapshot)

    def delegate(name: str, task: str) -> Report:
        """交办一处修改给子 Agent，等它结束，交回总结与 diff；不并入。

        Args:
            name: 给这次交办起的名字，并入时用。
            task: 交给子 Agent 的任务。
        """
        base = ws["current"]
        name = fork(ws, name)
        r = croon.send(instantiate, {"definition": worker, "input": {"value": {"task": task, "snapshot": base}, "dependencies": [base]}, "seats": {}, "self": {}},
                       dependencies=sorted([worker, base]), attach={"engine": engine, "exec": exec})
        reg = croon.send(r.attach["notify"], {}, attach={"to": back})
        a = receive(done, via=reg.derived["to"].handle)
        if not a.value["ok"]:
            raise RuntimeError(f"{name} 失败：{a.value['error']['message']}")
        got = a.value["value"]
        offer(ws, name, got["snapshot"])
        d = execute(exec, ws["current"], ["git", "diff", "--no-index", "--stat", "/mnt/a", "/mnt/b"],
                    mounts={"a": base, "b": got["snapshot"]})
        return {"name": name, "summary": got["summary"], "diff": d["stdout"]}

    def merge(name: str) -> str:
        """把一次交办交回的产物 Bundle 采用或合并进自己的状态。

        Args:
            name: 交办时返回的名字。
        """
        adopt(ws, exec, name)
        return f"已并入 {name}"

    tools = file_tools(exec, ws) + [delegate, merge]
    summary = react(engine, tools, [{"role": "user", "text": task}], SYSTEM)
    return croon.deliver({"summary": summary, "snapshot": ws["current"]}, [ws["current"]])
