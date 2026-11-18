# 发货标签

给这个工具一份含 `label.txt` 的目录和一个份数。它在输入的副本里生成 `print.txt`，每份按字节复制 `label.txt` 一次。要在标签之间换行，可以在 `label.txt` 末尾留一行换行符。份数范围是 1 到 5。工具只需要文件执行能力，CLI 可以免模型密钥运行。

## 配一批货物

完成仓库 README「上手」的 Linux 构建和两张 Docker 镜像准备。在 `croon-app` 仓库根的同一个 Bash 终端里执行下面的命令。Python 依赖只装在这个样例的虚拟环境里。

```bash
app=$(pwd -P)
pkg="$app/examples/shipping_labels"
client="$app/crates/host/clients/directory_tool.py"
bin="$app/target/debug/croon-app"
work=$(mktemp -d)
python3 -m venv "$work/venv"
py="$work/venv/bin/python"
"$py" -m pip install -r "$app/crates/host/clients/requirements.txt"
mkdir -p "$work/north/input"
printf 'SKU-A7\nNorth Dock\n' > "$work/north/input/label.txt"
cp "$pkg/binding.example.json" "$work/north/binding.json"
binding="$work/north/binding.json"
```

打开刚复制的 JSON。`input_dir` 是这批货物的输入目录，`results_dir` 存放各次调用的结果。相对路径从这份 JSON 所在的目录算起。默认配置下，上面这批货使用 `$work/north/input` 和 `$work/north/results`。将结果放在输入目录之外，给工具的目录只放准备交付的文件。换一批货时可以复制并修改绑定文件，继续使用同一个 `main.py`。服务在启动时读取绑定配置，切换批次后用新的 `--binding` 路径启动服务。

## 从命令行调用

```bash
args=(--binary "$bin" --package "$pkg" --binding "$binding")
"$py" -B "$client" "${args[@]}" describe
"$py" -B "$client" "${args[@]}" call '{"copies":2}' > "$work/receipt.json"
"$py" - "$work/receipt.json" "$work/north/input/label.txt" <<'PY'
import json, sys
from pathlib import Path
receipt = json.loads(Path(sys.argv[1]).read_text())
value = receipt['value']
printed = Path(receipt['directory']) / value['file']
assert value['copies'] == 2 and printed.read_bytes() == Path(sys.argv[2]).read_bytes() * 2
print(printed)
PY
```

`describe` 给出的输入只有 `copies`。`call` 返回工具的完整结束值 `value`，其中的 `snapshot` 标识产物快照，`directory` 是本次结果在宿主上的绝对路径。再次调用会得到另一个目录。份数越界或读不到 `label.txt` 时，程序退出非零，诊断在 stderr。

## 接入 Pi

在一个你信任的项目目录中运行下面的命令。`--local` 把启动方式写进这个项目的 `.pi/mcp.json`，其中记录上面准备好的绝对路径。保留工作目录和虚拟环境供 Pi 后续启动。`--exposure direct` 让模型可以直接调用注册的工具。

```bash
pi mcp add croon-labels --local --exposure direct -- "$py" -B "$client" "${args[@]}" serve
pi mcp list
```

在该项目启动 Pi，或在已打开的会话中用 `/reload` 载入配置。请它先生成两份标签，再生成三份，然后分别检查两次交回的 `directory/print.txt`。要检查错误反馈，可以让它试一次 `copies` 为 0。Pi 会话的模型由 Pi 配置，标签工具内部只在容器里运行命令。

## 怎样写自己的目录工具

入口和完整实现见 `main.py`。`workspace(snapshot)` 建立本次工作区。`advance(ws, exec, argv)` 执行命令并更新 `ws["current"]`，返回值含 `exit`、`stderr` 和 `timed_out`。例子先检查执行结果，再用 `croon.deliver` 把当前快照随结束值交回。宿主据此把文件物化到本次结果目录。命令、份数校验和错误说明由包作者决定，使用者绑定的宿主路径留在调用方。

用上面准备好的环境运行 `"$py" -B "$app/crates/host/clients/test_directory_tool.py" -v` 可复查两批目录的绑定、每次调用的打印内容以及失败时的产物处理。
