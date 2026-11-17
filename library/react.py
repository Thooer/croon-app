"""ReAct：Library 的一个函数，不是 Definition。"""
from typing import Any, Callable
import croon
from croon import Inbox, receive
from library.model_call import generate as ask


def react(generate: Callable, tools: list[Callable], messages: list[Any], system: str,
          inbox: Inbox | None = None, max_turns: int = 50) -> str:
    """循环：生成一次，按顺序执行工具调用，没有调用就以这一轮的文本结束。tools 是传进来的同一个列表，工具运行时往里加的，下一轮起可用。"""
    pending = []
    names = {}

    def read_messages() -> list[Any]:
        """读交来的新消息，读过就清空。"""
        out = [{"from": who, "text": v} for who, v in pending]
        pending.clear()
        return out

    if inbox is not None:
        tools.append(read_messages)
    table = {}
    specs = []

    def refresh() -> None:
        """工具表只增不减：新载入的下一轮就出现在请求里。一项是函数，或 {name, description, input, call}。"""
        while len(specs) < len(tools):
            t = tools[len(specs)]
            if isinstance(t, dict):
                table[t["name"]] = t["call"]
                specs.append({"name": t["name"], "description": t["description"], "input": t["input"]})
            else:
                tool = croon.tool(t)
                table[tool.name] = tool
                specs.append(tool.spec)

    for turn in range(max_turns):
        refresh()
        r = ask(generate, {"system": system, "messages": messages, "tools": specs})
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
