# experiments

我们怎样测「真实模型能不能在运行中造出一个主体并用起来」。**这是一份可读、可复现的记录，不是开箱即用的评测套件**：它要真实的模型服务、Docker 与密钥，会产生费用。做法与判据的权威在 `docs/Experiments.md`；这里只说怎么跑。

## 里面有什么

| 位置 | 内容 |
|---|---|
| `lab.py` | 运行器：每次尝试一个新的 Kernel 目录，遇到宿主故障自动 `resume`，从录像整理出经过，判定，写记录 |
| `checking.py` | 检查器共用的函数：读整理结果，用 `croon-app exec` 在检验方自己的输入上把造出来的源码再跑一遍 |
| `tasks/<名字>/` | 一个任务：`task.json`（任务文字、类别、所属的集）、可选的 `repo/`、`check.py`（检查器） |
| `runs/` | 每批的结果。不进 Git |

14 个任务，分两个集：

- **调参集（10 个）**：做基线、改了一项之后看的。纯计算 `even_sum`、`word_count`、`top_word`、`dedupe`；改文件 `fix_add`、`fix_count`、`fix_two`；调用模型 `slogan`、`classify`、`translate`。
- **留出集（4 个）**：`product`、`palindrome`、`fix_neg`、`keywords`。调整的时候不跑、不看，只在最后检查「改了之后没有退步」。

## 怎么跑

需要先按仓库根 README 的「上手」建好镜像、写好 `config/agent-provider.json`，并 `cargo build`。

```
# 一批：调参集每个任务跑一次，2 路并行
python3 experiments/lab.py run --batch 我的批 --provider config/agent-provider.json --repeat 1 --jobs 2

# 留出集
python3 experiments/lab.py run --batch 留出 --provider config/agent-provider.json --set holdout

# 只跑几个
python3 experiments/lab.py run --batch x --provider config/agent-provider.json --tasks even_sum,fix_add

python3 experiments/lab.py report --batch 我的批      # 汇总：每个任务的结果、失败阶段、make 的失败信息、token
python3 experiments/lab.py check  --batch 我的批      # 只改了检查器时：对已有的录像重新判定，不再调模型
```

一批开始前，`lab.py` 要求 App 的代码没有未提交的改动（`--allow-dirty` 可以跳过）：批中途改了代码，结果就不能混着统计。

## 会花多少

一批调参集 10 个任务、各 1 次，在我们用的模型上大约 5 分钟，输入 token 约 60 万，输出约 1 万。任务的快慢差得很大，改文件的几十秒到几分钟。

## 它能说明什么，不能说明什么

- **能说明**：路径走得通；失败落在哪个阶段；改了一项（诊断文本、工具说明）之后，`make` 的失败次数与种类有没有变。
- **不能说明**：通过率。每个任务一两次的样本只能当线索。任务都比较小，在调参集上已经全部通过，所以它现在没有能力继续区分好坏；要继续找系统的缺口，得先加更难的任务。
- 检查器由我们写，被测主体改不了它；它检查实际结果，不看主体的自述。但检查器也会错：我们遇到过参考答案数错、只认一种写法而误判，这些都在记录里留着。
