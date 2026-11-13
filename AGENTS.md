# AGENTS

croon-app 仓库的规则。协作原则、认识、方法见仓库族根目录的 AGENTS.md。文中的「我」是用户，「你」是 AI agent。

croon-app 是用 croon 与 croon-lang0 搭出的完整应用：模型适配、Docker 执行、Library 与 Application。它不承诺 API 稳定——预置与组合随需要演进。

## 位置

| 位置 | 收什么 |
|---|---|
| README.md | croon-app 是什么、定位、当前状态 |
| AGENTS.md | 本文 |
| docs/Library/README.md | Library 总览：原则、地图 |
| docs/Library/Runtime.md | 运行环境、工具调用、交办 |
| docs/Library/Organization.md | 组织与历史 |
| docs/Library/Presets.md | 预置 Charter 与 Application |
| docs/Directory/README.md | Tree 形状、Linux/Docker 文件执行 |
| docs/Model/README.md | 模型服务 Port 形状 |

- 置信度由位置决定：正文是已确认的，「待定」是未决的；没确认的留在讨论里。
- 写代码之前，先把机制写进文档，并让我看懂。
- App 的文档只写 App 自己的决定。Kernel 与 lang0 的语义只引名字，不复述理由：引用来源在 croon/docs/ 与 croon-lang0/docs/，croon-app 不重新解释它们。

## 文档与代码

### 唯一出处

- 一个事实只有一处权威。别处要用，引用它的名字，不复写。
- 形状以 `crates/*/schema/*.json` 为权威，代码用 `include_str!` 取用，不手写第二份。
- 跨仓库的形状经 crate 依赖取用（`croon_eval`、`kernel`、`lang0` 的公开 API），不读别的仓库的文档。
- `library/` 下的 Python 文件由构建生成（`scenarios` crate 的 `library_modules` example），不手改。

### 载体

| 载体 | 承担什么 | 代码能否取用 |
|---|---|---|
| 表格 | 枚举、映射、清单 | 否 |
| JSON | schema、接口、注册表 | 能 |
| 散文 | 判断、理由、权衡 | 否 |

- 自己定义的 JSON 字段与枚举字符串用 snake_case；JSON Schema 关键字照标准。

### 检查

提交时自动检查，没通过就不准提交。检查由 croon/crates/spec/ 的 check-spec 执行（开发期 path 依赖）。

- **测试存在**：不变量后标 `[test:文件:用例]`，每一个都要在测试里找得到。
- **名字唯一**：fence block 的名字只出现一次。
- **参考可解析**：`参考：` 行的路径要解析得到。Rust 文件开头的 `//!` 模块注释也用这一格式，路径相对仓库根。
- **篇幅**：按 `file.limits` 判定。

```json file.limits
{
  "min": 15,
  "healthy": 250,
  "danger": 400
}
```

### 上下文上限

```json context.limits
{
  "window": 1000000,
  "ratio": 0.2,
  "tokenizer": "o200k_base"
}
```

```json context.usage
{
  "tokens": 0,
  "percent": 0,
  "over_healthy": {}
}
```

### 文件边界

- 在非耦合的位置拆分。每部分先写成一个文件，最小 `min` 行，健康值 `healthy` 行以内，超过 `danger` 行拆成文件夹。
- 正文不插链接，末尾标「参考：」：只列仓库内的路径，路径相对本文件。

### 历史

历史在 Git 里。文档直接改写，不留版本标记或废弃段落。关键决策克制地写为什么。
