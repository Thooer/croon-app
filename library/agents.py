"""运行中构造 Agent：模型写 Lang0 源码，宿主推出 Interface，Kernel 检查并记下构造事实。"""
from typing import Callable
import croon
from library.kernel import Construct, Pack
from library.infer import Infer


def construct_tools(pack: Pack, construct: Construct, infer: Infer, modules: str,
                    agents: dict[str, str]) -> list[Callable]:
    """构造工具：造出的 Definition 登记在 agents 里，名字到 ID；模型只见名字。"""

    def construct_agent(name: str, source: str) -> str:
        """构造一个 Agent，交办时按名字用它。

        Args:
            name: 给它起的名字。
            source: Lang0 源码，须有顶层 def main，参数与返回值都标类型；可以 import library 里的模块。
        """
        body = {"source": source, "modules": modules}
        inferred = croon.send(infer, {"language": "lang0@1", "body": body}, dependencies=[modules]).value
        definition = {"language": "lang0@1", "interface": inferred["interface"], "body": body}
        packed = croon.send(pack, {"content": {"json": definition}, "dependencies": [modules]},
                            dependencies=[modules]).value
        built = croon.send(construct, {"definition": packed["id"]}, dependencies=[packed["id"]]).value
        unique = name
        n = 2
        while unique in agents:
            unique = f"{name}{n}"
            n += 1
        agents[unique] = built["id"]
        return f"已构造 {unique}"

    return [construct_agent]
