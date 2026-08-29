# Rils TODO

本文档记录尚未完成的优化、新特性和生态工作。条目按主题归类，不绑定具体版本；实际排期
会根据使用场景、兼容性和测试结果调整。已完成的能力应从这里移除，并同步到正式文档。

## 优化项

### Analyzer 与编辑器

- 支持 `pub use` 重导出参与跨文件补全、诊断、跳转和引用查找。
- 将工作区重分析从“变更后全量重分析”优化为基于模块依赖图的增量分析。
- 为 `ModuleId`、`SymbolId` 建立跨项目稳定身份，避免同名符号在复杂项目中产生歧义。
- 增加 workspace symbol、rename、code action 等常用 LSP 能力。
- 为大型项目建立可重复的解析、索引和补全性能基准。

### 运行时与编译器

- 在 `CompilationSession` 已缓存项目级 `DefMap`、`TypeckResults` 的基础上，让 Analyzer 复用同一次
  项目分析，并评估以源码 revision 缓存 entry `DefId` 与每模块 HIR；继续收缩 AST 解释器内重复的
  静态检查和名称查找逻辑。
- 合并解释器与 VM 的 runtime builtin dispatcher；标准 bytecode core import 已在链接时解析为稳定 ID，
  后续新增内建或外部 import 也应沿用该模式。完成这些边界后再评估 `rils_bytecode`、`rils_runtime`
  crate 拆分。
- 在已有统一指令步数与调用深度预算的基础上，继续增加堆、字符串、容器和宿主调用次数预算。
- 消除项目模块的初始化顺序依赖，并修复字节码 VM 跨模块直接构造或匹配 enum variant 时丢失名义
  类型身份的问题；项目顶层导入不应要求被依赖模块按路径字典序提前初始化。
- 评估常量折叠、无效代码删除、分支简化和寄存器复用，并以基准数据决定是否启用。
- 完善字节码调试信息的可剥离 section、跨版本兼容策略和 fuzz 覆盖。
- 继续拆分职责过重的 Rust 模块，保持入口文件只包含模块声明、导出和薄入口。

### 基础类型与标准能力

- 增加结构化数值转换错误类型和更完整的浮点转换入口。
- 评估 HashMap/HashSet 的借用查询、索引 place 和借用迭代器，遵守 Rils 引用不能逃逸的规则。
- 补充常用字符串解析、格式化和 Unicode 操作，但不引入隐式深拷贝。

## 新特性

### 语言

- 模式守卫、或模式、`@` 绑定和更完整的 `..` 模式。
- tuple struct、默认 trait 方法、trait object、条件 impl、`where` 和显式类型实参。
- 带标签的循环控制以及更多宏片段类型、嵌套重复和卫生宏能力。

### 项目与依赖

- 完成 `.rilslib` 的公开声明表和动态链接：源码依赖与二进制依赖必须使用相同的库身份、类型/trait
  语义和符号导入；入口 `.rilbc` 只引用共享库，不内嵌依赖实现，并覆盖缺失库、哈希/ABI 不匹配、
  重复库和跨库 trait impl 冲突。
- 扩展 `rils.toml` 的 crate/外部依赖声明、workspace 和锁定文件模型。
- 增加项目级模块图、依赖循环诊断和可复现的构建缓存。
- 评估外部 crate 注册、版本解析和离线依赖分发方案。

### 宿主与部署

- 在现有 host value formatter 与文本输出 callback 之上增加日志级别，并允许宿主定制未知
  host type 的 fallback 策略。

- 在保持宿主无关的前提下继续扩展 C ABI 的核心生命周期、值交换和诊断能力。
- 为自动生成的 Unity direct C# handler 增加显式 override 层，使少量需要自定义语义、错误映射或
  性能特化的 Core API 可以替换自动绑定；继续补齐 enum、常量和超过 16 字节的 struct transport，
  无法表达的同签名碰撞仍要求 override 或手写 facade。
- 编译后按实际 host imports 裁剪或外置运行时契约，避免完整 Unity manifest 在每个脚本资产中重复内嵌。
- 将 Unity、UE 等引擎集成维护在各自独立仓库和插件工程中。
- 评估可选 Rust AOT 后端；AOT 不替代字节码验证、能力隔离和资源限制。

## 工具与生态

- 完善 CLI 的项目检查、模块图、Manifest 校验和诊断导出命令。
- 提供标准库 API 目录和由 `rils_builtins` 生成的文档入口。
- 已建立独立的 `tools/rils-bench` release 基准工具和 `python tools/benchmark.py` 稳定入口；继续扩展
  解释器、磁盘字节码和 Analyzer 场景，并在基线稳定后建立持续性能回归。
- 增加跨平台原生构建与发布矩阵，并明确各宿主的 ABI/字节码兼容策略。

## 记录规则

- 新条目应说明用户场景、影响范围和必要的兼容性约束。
- 设计尚未确定时记录候选方案，不把讨论稿当作语言规范。
- 能力完成后更新对应的正式文档、变更日志和测试，并从本文件移除。
