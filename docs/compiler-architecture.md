# 编译器架构与遗留模块迁移

本文记录 Rils 当前编译器与运行时架构、与 Rust 编译器分层的对照，以及仍需迁移或收缩的过渡模块。本文描述的是架构方向，不代表其中所有目标均已实现；具体任务状态仍以根目录 `TODO.md` 为准。

## 当前架构

当前主要依赖链为：

```text
rils_syntax
    ↓
rils_builtins + rils_host
    ↓
rils_frontend
    ↓
rils_compiler
    ↓
rils
```

各层当前主要职责如下：

- `rils_syntax`：lexer、parser、AST、Span、基础类型表示和语义 ID 类型。
- `rils_builtins`：标准库声明、文档、稳定 intrinsic/runtime ID 和后端类别的唯一事实来源。
- `rils_host`：Host Contract、Host 类型与函数声明、ABI 常量，以及 Manifest 编解码和验证。
- `rils_frontend`：源码数据库、模块索引、静态分析、名称与调用解析、类型推断、所有权和引用逃逸检查。
- `rils_compiler`：HIR lowering 和 MIR lowering，并为现有调用方兼容转发 Host API。
- 根 `rils` crate：公共 `Engine`、项目加载、AST 解释器、运行时 `Value`、字节码编码与格式、verifier、VM、宿主执行和标准库实际行为。

目前已经建立了 `CompilationSession`、`ProjectId`、`SourceDatabase`、`ModuleGraph`、`DefMap`、`TypeckResults`、HIR 和 MIR。项目加载器、compiler 输入和 Analyzer 已开始共享这一会话模型，整体方向正确。但 compiler 仍会把项目模块拼成 synthetic AST，部分旧流程也仍会改写 AST，或在解释器、VM 和 Analyzer 中重复解析语义。

## 与 Rust 编译器的对照

Rust 编译器大致按以下职责分层：

```text
rustc_span
rustc_ast / rustc_parse
rustc_resolve
rustc_hir / rustc_ast_lowering
rustc_hir_typeck / rustc_hir_analysis
rustc_middle（TyCtxt、query、MIR）
rustc_mir_build / rustc_mir_transform
rustc_codegen_*
rustc_interface / rustc_driver
```

Rils 不需要复制 rustc 的 crate 数量和复杂 query system，但应借鉴以下边界：

1. AST 保留源码原貌，名称解析和类型检查结果写入 side table，不原地修改 AST。
2. `DefId`、`ExprId`、`ModuleId` 等身份是后续阶段的主键，`Span` 只用于源码定位和诊断。
3. 项目编译由统一 compilation session 管理源码、模块图、定义和阶段产物。
4. driver/interface 只负责组织阶段，不承担项目 AST 拼装、语义猜测或后端行为实现。
5. Analyzer、解释器和 VM 消费共享语义结果，不各自维护名称、类型和标准库目录。
6. Host Manifest、target 和 ABI 等共享元数据不应归某个后端私有。

目标流水线仍保持：

```text
lexer/parser
    ↓
static analysis
    ↓
HIR
    ↓
MIR
    ↓
bytecode
    ↓
verifier
    ↓
VM
```

AST 解释器在字节码迁移期间继续作为完整语义参考后端，但应逐步改为消费共享 frontend 结果。

## 必须迁移的过渡设计

### 数值字面量 AST 改写

`crates/rils_frontend/src/resolution.rs` 当前先推断数值类型，再将未定型整数和浮点字面量原地改写成具体 AST literal。编译和分析入口随后还会再次执行类型推断。

目标设计为：

```text
未定型 AST literal
    ↓
TypeckResults[ExprId] = i32/u64/f32/...
    ↓
HIR 或解释器根据类型结果构造实际值
```

完成后删除：

- `resolution.rs`
- `NumericResolutionError`
- `resolve_numeric_literals*`

### Host 类型名称 AST 改写

`crates/rils_frontend/src/host_type_resolution.rs` 当前把源码中的 Host type alias 或 glob 路径改写为 canonical manifest string。

AST 应保留用户写下的路径；解析后的 canonical Host identity 应进入统一 definition/type namespace，并由 `DefMap`、类型检查结果或 HIR 保存。完成后删除 `host_type_resolution.rs`。

### Host enum synthetic AST 注入

`rils_compiler` 当前会把 Host Contract 中的 enum、flags 和相关能力转换成 synthetic `Stmt::Enum`、`Stmt::Impl`，这些节点没有真实源码 Span。

该设计会把宿主元数据伪装成用户源码，并影响诊断、源码身份和 Analyzer 依赖。Host 声明应直接注册到共享 semantic declaration table，不再生成 AST。

### Host Contract 所属层级（共享层已完成）

Host Contract 和 Manifest 已从 `rils_compiler` 迁入独立的 `rils_host`。Host-aware analysis 也已下沉到 `rils_frontend`，Analyzer 不再依赖 compiler crate。`rils_compiler` 暂时兼容转发原有 Host API，避免内部重构立即破坏现有调用方。

`rils_host` 负责：

- `HostContract`
- Host function/type/enum/module 声明
- ABI 常量
- Manifest 编解码与 verifier
- frontend 可消费的 declaration view

当前依赖方向为：

```text
rils_frontend → rils_host
rils_compiler → rils_frontend + rils_host
rils_analyzer → rils_frontend + rils_host
rils runtime  → rils_host
```

后续仍需移除 Host enum synthetic AST 注入；该事项属于语义声明迁移，不再是 crate 所属层级问题。新增 crate 时必须遵守仓库命名和版本规范，不因内部重构自动修改版本号。

### 多文件项目 synthetic AST 拼装（会话基础已完成）

根 `src/lib.rs` 当前仍通过 `ProjectModuleNode`、`project_module_statements` 和 `prepare_project_entry` 把多个文件包装成 inline module，并插入 synthetic 入口调用。

统一的 `CompilationSession`、稳定 `ProjectId` 以及根项目加载器的 `ProjectCompilation` 已经建立，Analyzer 也不再分别持有源码数据库和项目语义索引。当前会话结构为：

```text
CompilationSession
├── SourceDatabase
├── ModuleGraph
├── 每模块 AST/HIR
├── 跨模块 DefMap
├── TypeckResults
├── Host declarations
└── entry DefId
```

compiler 的项目入口已经接收该 session，但 lowering 仍接收扁平化后的单个 `Program`。下一阶段需要让每个模块成为独立分析和 lowering 单位，并删除 `ProjectModuleNode`、`project_module_statements` 和 synthetic `main()` 调用。无 manifest 的 legacy entry loader 可以作为兼容入口保留，但不应继续作为主项目编译模型。

### 语义 ID 仍由 Span 反推

目前部分 `DefId`、`ExprId` 和 `ImplId` 是在分析结束后，对以 `Span` 为键的结果排序并分配得到的。因此 ID 虽已存在，但仍依赖 Span 唯一性；宏展开、synthetic AST 或共享 Span 节点可能发生身份碰撞。

目标是：

- parser 或 HIR lowering 阶段为表达式分配稳定身份。
- definition collection 阶段直接分配 `DefId`。
- `TypeckResults`、调用解析和 ownership 结果全部以 ID 为键。
- `Span` 只作为 ID 对应的诊断位置，不再充当语义主键。

## 应逐步收缩的旧模块

### AST 解释器中的静态语义

AST 解释器不能立即删除，它仍是字节码迁移期间的语义参考，并承担动态宿主注册能力。但以下职责应逐步迁回共享 frontend：

- `src/interpreter/type_check.rs` 中的类型兼容检查
- trait 方法身份和签名检查
- 声明、模块和方法名称查找
- builtin 方法签名和注册目录
- 与 frontend 重复的所有权或引用约束判断

目标结构为：

```text
共享 frontend analysis
    ↓
DefMap + TypeckResults
    ├── AST interpreter
    └── HIR → MIR → bytecode → VM
```

解释器最终只负责 AST 求值、动态作用域状态和宿主交互，不再维护另一套静态分析器。

### Interpreter 与 VM 的 runtime builtin 重复实现

当前主要分发表位于：

- `src/interpreter/builtin_methods.rs`
- `src/bytecode/runtime_builtins.rs`

Vec、String、Option、Result 和 Iterator 的大量行为、类型约束及错误信息分别实现。数值 intrinsic 已经复用 `numeric.rs`，HashMap/HashSet 已经复用 `hash_collections.rs`，可以沿用这一方向。

纯运行时操作应进入共享的 ID dispatcher。需要调用 Rils 函数值的 Iterator combinator 可以通过后端 callback adapter 保留执行差异：

```text
共享 builtin operation
    ├── Interpreter callback adapter
    └── VM callback adapter
```

应增加解释器与 VM 运行同一源码的矩阵测试，防止收敛过程中改变语义。

### Bytecode core import 字符串分发

`src/bytecode/core_imports.rs` 的 import 声明列表已经从 `rils_builtins` 生成，但执行仍通过函数名字符串 `match` 映射到 `BuiltinId`。

bytecode 链接阶段应把导入解析为明确类别：

```text
Builtin(BuiltinId)
Host(HostImportId)
External(...)
```

VM 热路径直接按 ID 分发。字符串只保留在源码解析、诊断和磁盘导入描述中，不应成为内部 builtin 调用主键。

### Analyzer 的文本解析兜底

Analyzer 中有两类文本处理，应区别对待。

合理保留：

- 未完成源码的补全恢复
- 在光标位置插入临时标识符后重新分析
- 括号、成员访问等局部语法恢复

这些能力在 parser 尚未完整支持错误节点时仍有必要。

应迁移：

- Analyzer 自行扫描源码解析 path alias
- navigation、completion 和 signature help 分别识别 qualified path
- Analyzer 直接遍历 builtins 和 Host Contract 重新拼装成员目录
- 通过简单变量名重新猜测 receiver 类型

frontend 最终应提供类似以下的语义查询接口：

```text
completion_at(SourceId, offset)
signature_at(ExprId)
definition_at(SourceId, offset)
members_of(TypeId)
```

Analyzer 只负责容错输入和 LSP 数据转换。

## 需要拆分但不应删除的模块

以下文件职责合理，但体积或职责组合已超过薄入口和单一职责要求：

- `crates/rils_frontend/src/analysis.rs`
- `crates/rils_frontend/src/type_inference.rs`
- `crates/rils_compiler/src/hir.rs`
- `crates/rils_compiler/src/host.rs`
- `src/bytecode/vm.rs`
- `src/value.rs`
- 根 `src/lib.rs`

建议拆分方向：

- `analysis`：definition collection、scope/import resolution、member enrichment、diagnostic orchestration。
- `type_inference`：constraint collection、call inference、expression visitor、constraint solving。
- `hir`：program/function lowering、expression lowering、call lowering、type lowering。
- `host`：contract model、validation、inheritance、manifest codec/API。
- `vm`：frame/call、instruction execution、imports、value operations。
- `value`：primitive、aggregate、callable、host、reference value。
- 根 `lib.rs`：拆出 `engine`、`project_compilation`、`compile_facade` 和 `source_diagnostics`。

`src/runtime_type.rs` 是静态 `Type` 与动态 `Value` 之间的运行时桥接，职责本身合理。它可以按集合、函数和聚合值拆分，但不属于应删除的重复类型系统。

## 明确保留的兼容层

以下内容具有明确兼容目的，不应在内部重构中顺手删除：

- `Project::for_legacy_entry` 和无 manifest 的 `name.rils` / `name/mod.rils` 入口
- Host Manifest 的 legacy v2/v3/v4 编码兼容
- legacy function-like macro grammar
- `SymbolId` 到 `DefId` 的编辑器 API 兼容别名
- Host layout 的旧拼写兼容

删除这些能力属于破坏性更新，需要独立迁移周期、changelog 说明和明确授权。

## 长期 crate 边界

在共享语义和项目 session 完成后，可以再评估以下拆分：

```text
rils_compiler
    frontend orchestration + HIR/MIR

rils_bytecode
    bytecode IR + encoder + disk format + verifier

rils_runtime
    Value + runtime builtins + VM + host execution

rils
    public facade + Engine
```

这不是当前第一优先级。`Value`、bytecode type table、Host ABI 和宿主调用仍有较深耦合，过早拆 crate 只会把内部耦合变成跨 crate 耦合。

## 推荐迁移顺序

迁移应按依赖方向分批进行，每批保持解释器和 VM 对照测试通过：

1. 已完成：抽离 Host Contract/Manifest 共享层，解除 Analyzer 对 compiler 的依赖。
2. 进行中：已建立 `CompilationSession` 并迁移项目加载器、compiler 输入和 Analyzer；仍需取消扁平 AST，使 `ModuleGraph` 成为 lowering 主模型。
3. 在 syntax/HIR 构造阶段分配真实 `DefId`、`ExprId`，停止从 Span 反推。
4. 将 numeric literal 和 Host type AST rewrite 改为 semantic side table，并删除旧模块。
5. 让 AST 解释器消费共享 `DefMap`、`TypeckResults`，收缩旧静态检查逻辑。
6. 合并解释器和 VM 的 runtime builtin dispatcher。
7. 将 bytecode core import 从字符串分发改为 ID 分发。
8. 按职责拆分大文件和根 facade。
9. 再评估 `rils_bytecode`、`rils_runtime` 的 crate 拆分。
10. 最后单独决定 legacy project、macro 和 manifest 兼容层的废弃周期。

## 每阶段验证要求

架构迁移不得只验证编译成功。每个阶段至少应覆盖：

- `cargo fmt --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- 同一源码在 AST 解释器与 VM 下的结果一致性
- 多文件、跨模块、Host Manifest 和错误 Span 场景
- builtin 声明、类型检查、Analyzer 可见性与 runtime handler 的自动一致性

涉及磁盘字节码、Host ABI 或兼容层删除时，还必须按对应规范增加格式、verifier、C API 和迁移测试。
