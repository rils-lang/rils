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

目前已经建立了 `CompilationSession`、`ProjectId`、`SourceDatabase`、`ModuleGraph`、`DefMap`、`TypeckResults`、HIR 和 MIR。项目加载器、compiler 输入和 Analyzer 都共享这一会话模型及其项目分析缓存；compiler 和配置项目解释器都直接消费独立模块 AST，不再拼装 synthetic project AST。AST 保持解析后的原貌，数值具体化与 Host 名称解析通过语义 side table 完成。当前剩余重复主要位于解释器静态检查和名称查找、Analyzer 查询适配，以及后续非标准或外部 bytecode import 的链接分类。

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

### 数值字面量 AST 改写（已移除）

compiler 和 AST 解释器均通过表达式的 `ExprId` 查询 `TypeckResults`，据此生成具体宽度的整数或浮点值，并在 literal Span 上报告越界。原有 `resolution.rs`、`NumericResolutionError` 和 `resolve_numeric_literals*` API 已删除；共享的 `numeric_literals` 模块只负责根据目标类型具体化单个 literal，不修改 AST。

目标设计为：

```text
未定型 AST literal
    ↓
TypeckResults[ExprId] = i32/u64/f32/...
    ↓
HIR 或解释器根据类型结果构造实际值
```

### Host 类型名称 AST 改写（已移除）

AST 保留用户写下的 Host alias 或 glob 路径。类型节点使用 source-scoped `TypeRefId`，pattern 节点使用 `PatternId`，两者均按 AST preorder 分配并保留同一 Span 对应多个节点的关系。不可变的 `HostTypeResolutionResults` 按 `TypeRefId`、`ExprId` 和 `PatternId` 记录 canonical type/path，并作为 `DocumentAnalysis` 的阶段产物参与项目分析合并。`HostTypeResolutionView` 为类型推断、静态检查、Analyzer 和 HIR 提供只读查询。`host_type_resolution.rs` 仍作为该 side table 的窄接口保留，不再包含 AST rewrite API。

### Host enum synthetic AST 注入（已移除）

Host Contract 中的 enum、flags 和相关能力现在直接进入 frontend 类型检查、控制流分析、Analyzer 元数据和 HIR 类型声明，不再转换成没有真实 Span 的 `Stmt::Enum`、`Stmt::Impl`。Host 声明不会再伪装成用户源码。

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

新增 crate 时必须遵守仓库命名和版本规范，不因内部重构自动修改版本号。

### 多文件项目 synthetic AST 拼装（结构化语法已进入会话）

根项目加载器不再通过 `ProjectModuleNode` 和 `project_module_statements` 自行维护模块树。每个文件解析出的 `Program` 现在按 `ModuleId` 保存在 `ProjectSyntax` 中，prelude 则作为明确的根语法单元保存；compiler 的 session 入口也不再接收调用方拼装好的单个 `Program`。

统一的 `CompilationSession`、稳定 `ProjectId` 以及根项目加载器的 `ProjectCompilation` 已经建立，Analyzer 也不再分别持有源码数据库和项目语义索引。session 会按项目和 Host Contract hash 缓存合并后的 `DocumentAnalysis`；源码、模块图或项目语法发生可变访问时缓存失效。准确的当前结构为：

```text
CompilationSession
├── SourceDatabase
├── ModuleGraph
├── ProjectSyntax（根语法单元 + ModuleId -> Program）
├── ProjectSemanticIndex（模块、源码与已索引 definition 的项目视图）
└── ProjectAnalysis（Host hash + DefMap + TypeckResults + 诊断等）
```

compiler、根项目解释器和 Analyzer 都消费 session 中缓存的项目分析。Analyzer 在工作区重分析时按 `ProjectSyntax` 与 `ModuleGraph` 生成完整项目结果，写回 `CompilationSession`，并以该缓存建立跨文件定义索引和导出信息；单文档分析仍只用于该文档的 LSP 诊断与容错展示。entry `DefId` 和每模块 HIR 尚未成为显式缓存项。当前失效策略是保守的整项目失效，后续可引入源码 revision 做细粒度查询。

compiler 的项目入口通过入口源码身份解析稳定的 `DefId` 后在 HIR 中调用 `main`，不再向编译 AST 插入 synthetic `main()` 调用。frontend 的项目分析按模块路径对独立 `Program` 做导出收集、跨文件复析，并合并项目级 `DefMap` 和 `TypeckResults`；HIR 的声明收集和 lowering 也直接遍历 `ModuleGraph` 与独立 `Program`。配置项目的 AST 解释器同样按 `ModuleGraph` 建立运行时模块环境，以入口 `DefId` 调用 `main`，并在执行前拒绝 frontend 的首个 error diagnostic；原 inline-module compatibility program 已删除。无 manifest 的 legacy entry loader 作为明确兼容入口保留。

配置项目的解释器会从顶层 `use` 建立模块初始化依赖顺序，而不是按模块路径或文件发现顺序执行；
导入循环会在执行入口前明确报错。HIR lowering 将 struct/enum pattern 中的类型路径规范化为声明模块
的完整名义路径，解释器则直接比较运行时类型定义身份，因此跨模块同名类型不会因短名称相同而误
匹配。该规范化只改变内存 HIR 和既有 pattern 字符串内容，不增加磁盘格式字段。

### 表达式 ID 已按 AST 访问顺序分配

definition collection 已在访问声明时直接分配 `DefId`；函数和方法同时登记 `BodyId`，impl 也在访问节点时直接分配 `ImplId`，不再于分析结束后通过定义 Span 反查 owner。`ExprId` 现在也按每个 `Program` 的 AST preorder 直接分配，调用和值解析按该 ID 写入 side table；共享同一 Span 的表达式会获得不同身份，Span 索引保留完整的一对多关系。

类型推断使用同一个 AST 身份分配器直接产生 `ExprId -> Type`，并由该表构造 `TypeckResults`；即使两个表达式共享 Span，也可以保留不同的推断类型。HIR lowering（包括数值 literal lowering）、调用解析、控制流、所有权、静态类型、格式检查和 Analyzer 成员补全均通过节点身份索引查询 `ExprId`。旧的表达式 Span 类型主表和有歧义的单值 `*_at(Span)` 查询已经移除；Span 只保留为 `ExprId -> Span` 的诊断位置，以及明确的一对多或 offset 源码查询索引。解释器按 `ExprId` 读取类型结果并具体化 numeric literal，不修改 AST。

数值约束中的临时 integer/float inference variable 也以 `ExprId` 标识，不再复用 literal Span。它们属于 frontend 内存态，字节码编码器会明确拒绝未求解的 inference type；原有磁盘格式中的 legacy Span numeric variable tag 仍只为旧字节码解码兼容保留，本次迁移不改变格式版本。

目标是：

- parser 或 HIR lowering 阶段为表达式分配稳定身份。
- 已完成：definition collection 阶段直接分配 `DefId`、`BodyId` 和 `ImplId`。
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

### Interpreter 与 VM 的 runtime builtin 重复实现（纯运行时分发已收敛）

纯运行时 builtin 已集中到后端中立的 `src/runtime_builtins.rs` 及其子模块。解释器、bytecode core
import 和 VM 通过稳定 `BuiltinId` 进入同一 dispatcher；数值、Range、HashMap/HashSet、String、
Vec、Option/Result 和无 callback 的内建 Iterator 操作不再各自维护行为实现。

需要调用 Rils 函数值的 Option/Result combinator 和自定义 Iterator 操作仍通过解释器或 VM 各自的
callback adapter 执行。这是用户函数调用机制的后端差异，不属于重复的纯 builtin 分发：

```text
共享 builtin operation
    ├── Interpreter callback adapter
    └── VM callback adapter
```

后续增加带 callback 的能力时仍应使用解释器/VM 同源码矩阵测试，不应重新建立按名称分发的第二套
runtime builtin 目录。

### Bytecode core import 字符串分发（标准 core import 已完成）

`src/bytecode/core_imports.rs` 的 import 声明列表从 `rils_builtins` 生成。`BytecodeHost::standard` 在初始化时将每个标准 core import 名称解析为 `CoreImport`（其中 runtime member 是稳定 `BuiltinId`），handler 闭包只保存该 ID；VM 热路径不再按函数名字符串匹配。字符串仍保留在源码解析、链接、诊断和磁盘导入描述中。

宿主链接阶段仍可继续把其他内建或外部导入解析为明确类别：

```text
Builtin(BuiltinId)
Host(HostImportId)
External(...)
```

VM 热路径应继续按 ID 或已解析 handler 分发。字符串只保留在源码解析、诊断和磁盘导入描述中，不应成为内部 builtin 调用主键。

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
- `crates/rils_host/src/lib.rs`
- `src/bytecode/vm.rs`
- `src/value.rs`
- 根 `src/lib.rs`

建议拆分方向：

- `analysis`：definition collection、scope/import resolution、member enrichment、diagnostic orchestration。
- `type_inference`：constraint collection、call inference、expression visitor、constraint solving。
- `hir`：program/function lowering、expression lowering、call lowering、type lowering。
- `rils_host`：contract model、validation、inheritance、manifest codec/API。
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

已于当前架构阶段评估：暂不创建 `rils_bytecode` 或 `rils_runtime` crate。`Value`、bytecode type table、Host ABI、解释器与 VM 的宿主调用仍在根 crate 内共享具体表示；此时拆分只会形成跨 crate 的高扇入依赖，不能降低耦合。

只有在以下条件同时满足时才重新开启拆分：

- `Value` 与 Host ABI 的运行时表示可由一个不依赖解释器或 bytecode 的稳定 crate 提供；
- bytecode 编码、格式、verifier 与 VM 仅依赖该运行时 crate 和 frontend/host 的公共模型；
- AST 解释器和 VM 的宿主调用边界不再需要访问彼此的私有实现；
- 新 crate 能以独立测试证明解释器/VM 对照、磁盘格式 verifier 与 C API 路径均不退化。

## 推荐迁移顺序

迁移应按依赖方向分批进行，每批保持解释器和 VM 对照测试通过：

1. 已完成：抽离 Host Contract/Manifest 共享层，解除 Analyzer 对 compiler 的依赖。
2. 已完成：`CompilationSession` 以 `ProjectSyntax` 保存独立模块 AST，项目 analysis、跨文件调用解析、HIR lowering 和配置项目解释执行均直接消费模块集合；compiler 与解释器入口不再依赖 synthetic project AST。
3. 已完成：`DefId`、`BodyId`、`ImplId` 和 `ExprId` 均在 AST/definition 访问时直接分配；类型推断、调用解析、静态检查器、Analyzer 与 HIR lowering 均按 `ExprId` 查询，表达式 Span 兼容主表已移除。
4. 已完成：compiler 与 AST 解释器直接按 semantic type 具体化 numeric literal；Host type side table 已由 compiler、Analyzer、静态检查和 HIR 消费；numeric、Host type rewrite 和 Host enum synthetic injection 均已删除。
5. 本轮边界已完成：AST 解释器消费共享 `TypeckResults`，配置项目入口使用项目 `DefMap` 并以 frontend error diagnostic 作为执行前 gate；frontend 在项目级按模块路径解析 trait 声明与 `use` 导入，将完成 supertrait 和方法契约检查的 impl 记录为稳定 `ImplId`，项目解释器对这些 impl 复用验证结果。无法由项目语义解析的路径继续保留运行时防御。剩余静态检查和名称查找收缩属于后续独立迭代，不阻塞本轮语义身份迁移收口。
6. 已完成：数值、Range、HashMap/HashSet、String、Vec、Option/Result，以及内建 `SequenceIterator` 的无 callback 操作（含 `enumerate`）已共享根 runtime dispatcher；解释器、bytecode core import 和 VM 均直接调用该后端中立层。需要调用 Rils 函数值的 Option/Result 和自定义 Iterator callback adapter 刻意保留在解释器中，属于执行用户回调而非纯 runtime builtin 分发。
7. 已完成：标准 bytecode core import 在 host 初始化时解析为稳定 ID 或专用操作，执行热路径不再按字符串分发。
8. 后续独立重构：按职责拆分大文件和根 facade；机械拆分不与新的语言语义混在同一分支。
9. 已完成评估：当前不拆分 `rils_bytecode`、`rils_runtime`；待运行时表示、bytecode 与宿主 ABI 达到上述独立边界后再重新立项。
10. 已完成决定：legacy project、macro 与 manifest 兼容层维持“读取旧格式、只写当前格式”的策略；删除必须作为单独破坏性版本的提案，包含实际使用者审计、迁移指南、`CHANGELOG.md` 说明与明确授权，当前不设删除日期。

## 本轮语义身份迁移的收口边界

本轮迁移以“源码节点和定义拥有稳定语义身份，项目、compiler、解释器与 Analyzer 能共享对应分析
结果”为完成条件。Host Contract 独立层、结构化项目 session、`DefId`/`BodyId`/`ImplId`/`ExprId`、
类型与 Host 解析 side table、项目 frontend gate、trait impl 契约复用，以及纯 runtime builtin 的
稳定 ID dispatcher 均已进入这一边界。

以下事项建立在该基础上，但具有各自独立的验收条件，不属于本轮退出条件：

- 彻底移除解释器剩余静态检查和名称查找；
- 按 source revision 缓存 entry `DefId`、项目分析和每模块 HIR；
- 拆分 frontend、HIR、Host、VM、Value 与根 facade 的大文件；
- 建立 Analyzer 统一语义查询、增量分析和完整 `pub use` 支持；
- 实现 `.rilslib` 声明表、外部链接和 workspace/lockfile。

这些工作应记录在 `TODO.md` 并从最新版本分支分别拉出短分支。除非发现由本轮迁移直接引入的回归，
否则不再扩张本轮 feature 的实现范围。

## 每阶段验证要求

架构迁移不得只验证编译成功。每个阶段至少应覆盖：

- `cargo fmt --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- 同一源码在 AST 解释器与 VM 下的结果一致性
- 多文件、跨模块、Host Manifest 和错误 Span 场景
- builtin 声明、类型检查、Analyzer 可见性与 runtime handler 的自动一致性

涉及磁盘字节码、Host ABI 或兼容层删除时，还必须按对应规范增加格式、verifier、C API 和迁移测试。
