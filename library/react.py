"""ReAct：Library 的一个函数，不是 Definition。"""
from typing import Any, Callable
import croon
from croon import Inbox, receive


def react(generate: Callable, tools: list[Callable], messages: list[Any], system: str,
          inbox: Inbox | None = None, max_turns: int = 50) -> str:
    """循环：生成一次，按顺序执行工具调用，没有调用就以这一轮的文本结束。"""
    pending = []
    names = {}

    def read_messages() -> list[Any]:
        """读交来的新消息，读过就清空。"""
        out = [{"from": who, "text": v} for who, v in pending]
        pending.clear()
        return out

    fns = tools + ([read_messages] if inbox is not None else [])
    table = {t.name: t for t in [croon.tool(f) for f in fns]}
    specs = [t.spec for t in table.values()]

    for turn in range(max_turns):
        r = croon.send(generate, {"system": system, "messages": messages, "tools": specs}).value
        if r["finish"] not in ["stop", "tool_calls"]:
            raise RuntimeError(f"生成没有正常结束：{r['finish']}")
        messages.append({"role": "assistant", "text": r["text"],
                         "reasoning": r["reasoning"], "calls": r["calls"]})
        if not r["calls"]:
            return r["text"]
        results = []
        for call in r["calls"]:
            t = table.get(call["name"])
            try:
                if t is None:
                    raise KeyError(f"没有名为 {call['name']} 的工具")
                out, bad = t(call["input"]), False
            except Exception as e:
                out, bad = {"error": croon.kind(e), "message": str(e)}, True
            results.append({"id": call["id"], "name": call["name"], "output": out, "is_error": bad})
        if inbox is not None:
            a = receive(inbox, timeout=0)
            if a is not None:
                who = names.setdefault(a.via, f"来信{len(names) + 1}")
                pending.append((who, a.value))
                results[-1]["output"] = {"result": results[-1]["output"],
                                         "note": f"有 {len(pending)} 条新消息"}
        messages.append({"role": "tool", "results": results})
    raise RuntimeError(f"{max_turns} 轮之内没有结束")
