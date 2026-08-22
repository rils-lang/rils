# Rils 示例

示例分为两类：单文件示例用于聚焦一组紧密相关的语言能力；项目示例使用 `rils.toml` 和多文件
模块布局，展示更接近真实工程的领域建模与数据处理。除明确涉及文件系统的示例外，仓库测试会
同时在 AST 解释器和字节码 VM 中运行示例，并检查固定返回值。

## 单文件示例

| 示例 | 目标 | 预期返回值 |
| --- | --- | ---: |
| `hello.rils` | 函数、循环、断言和基础输出 | `720` |
| `collections_and_closures.rils` | 数组、Vec、索引引用、闭包捕获和拥有型迭代 | `42` |
| `domain_model.rils` | 泛型 struct、trait/UFCS、enum pattern 和类型别名 | `42` |
| `fallible_pipeline.rils` | `Option`、`Result`、`?` 和错误分支匹配 | `42` |
| `iterators.rils` | 自定义 `Iterator`/`IntoIterator` 与 Range | `20` |
| `macros.rils` | expr/ident fragment 和宏重复展开 | `42` |
| `references.rils` | 字段 place、多个可变引用和显式 Clone | `7` |
| `standard_fs.rils` | `std::fs` 能力授权和结构化 IO 错误 | 文件往返 |

运行单文件示例：

```console
cargo run -p rils_cli -- examples/domain_model.rils
```

`standard_fs.rils` 会在当前目录短暂创建并删除 `rils-standard-library-example.txt`，因此自动测试只
验证其能够编译，不执行文件系统副作用。

## 项目示例

### task_board

从任务看板服务提取的领域逻辑，覆盖跨模块名义类型、trait 评分策略、消费式汇总、可变 facade、
Default 派生和报告输出：

```text
task_board/
├── rils.toml
└── src/
    ├── domain.rils   # Task、Priority、TaskState 与 Scored
    ├── kanban.rils   # Board 聚合与 Summary
    ├── limits.rils   # 容量策略
    ├── logbook.rils  # 输出与稳定校验值
    └── main.rils     # 组合入口
```

```console
cargo run -p rils_cli -- run examples/task_board
```

入口返回 `1222`。

### telemetry_pipeline

从遥测采集流水线提取的批处理逻辑，覆盖自定义迭代器、事件 enum、Vec、`Result` 传播、Default
聚合状态和成功/失败批次：

```text
telemetry_pipeline/
├── rils.toml
└── src/
    ├── event.rils      # 事件模型与构造 facade
    ├── generator.rils  # Window 与 SampleRange
    ├── ingest.rils     # Metrics 和可失败聚合
    └── main.rils       # 批次生成与校验入口
```

```console
cargo run -p rils_cli -- run examples/telemetry_pipeline
```

入口返回 `7703`。

## 维护原则

- 只展示一个局部语法点时使用单文件；需要多个职责协作时创建标准 Rils 项目。
- 项目示例按领域职责拆分模块，`main.rils` 只负责组合和最终验证。
- 确定性示例必须给出稳定返回值，并同时经过解释器和 VM；有外部副作用的示例至少通过编译验证。
- 新示例优先提取真实工程中的数据流、状态转换或领域规则，不把互不相关的语法机械堆在一个文件里。
