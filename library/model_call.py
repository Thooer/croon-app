"""模型调用：把 Model 交还的偶发失败重试掉，其余原样向外。见 croon-app:docs/Library/Runtime.md「模型调用」。"""
from typing import Any
import croon
from library.model import Generate

RETRIES = 4


def transient(e: Exception) -> bool:
    """这个失败是偶发的：HTTP 429、5xx，或请求自己的时限到了（status 为空）。"""
    if croon.kind(e) != "use":
        return False
    data = croon.data(e)
    if not isinstance(data, dict) or not isinstance(data.get("error"), dict) or "status" not in data["error"]:
        return False
    status = data["error"]["status"]
    return status is None or status == 429 or status >= 500


def generate(engine: Generate, request: Any) -> Any:
    """请求一次模型生成，返回结果值；偶发的失败最多重试 RETRIES 次，每次重试是一个新键的 Atom。

    Args:
        engine: 模型的 Port。
        request: model.generate 的输入：system、messages、tools。
    """
    for attempt in range(RETRIES):
        try:
            return croon.send(engine, request).value
        except Exception as e:
            if not transient(e):
                raise
    return croon.send(engine, request).value
