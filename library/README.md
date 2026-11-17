# Library

这里是可以 import 的模块。用 `look` 看：不给名字是这张表，给模块名（如 `library.files`）是它的公开面，
`source=True` 才是源码。写 Agent 之前先看一两个相关的。

一个 Agent 是一份有顶层 `def main` 的 Lang0 源码。`main` 的参数与返回值都要标类型：值类型进输入与结果，
`Port` 类型（如 `Generate`、`Exec`）是它要用的能力，由交办的一方按名字接上；现在能给的只有 `engine`（模型）与 `exec`（文件执行）。
要改工作区的，输入里写 `snapshot: str`、结果里带 `snapshot`，工作区由工具填，不用你传。

- `library.model`：`Generate`，模型。`croon.send(engine, {"system": …, "messages": […], "tools": […]}).value` 得到 `{text, calls, finish, …}`。
- `library.directory`：`Exec`，在一份目录快照里跑命令。
- `library.workspace`：工作区，`workspace(snapshot)` 开一个，`advance` 在当前位置跑命令并采用结果。
- `library.files`：`file_tools(exec, ws)` 得到 read_file、write_file、run 三个工具。
- `library.react`：`react(engine, tools, messages, system)`，模型与工具来回，没有调用时返回最后的文本。
- 例子：`library.s` 是一个改代码的 Agent，`look(name="library.s", source=True)` 看它的全文，可以照抄。

造好（`make`）就成了你的一个工具，直接调用；它交回目录的，结果里有 `name` 与 `diff`，满意再 `merge`。
