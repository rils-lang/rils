# Rils TODO

本文档记录尚未完成的优化、新特性和生态工作。条目按主题归类，不绑定具体版本；实际排期
会根据使用场景、兼容性和测试结果调整。已完成的能力应从这里移除，并同步到正式文档。

## 优化项

### Analyzer 与编辑器

- 将 Analyzer 当前服务级合并的 Host Contract 改为按项目隔离，避免无关项目的同名宿主声明互相冲突；
  保留 Manifest 启动容错、事务热重载和错误路径诊断。
- 在已支持公开源码声明重导出的基础上，补齐枚举变体、私有中间别名和受限可见性组合。
- 将工作区重分析从“变更后全量重分析”优化为基于模块依赖图的增量分析。
- 已有会话内稳定 ProjectId、带项目归属的模块查询及跨重导出链的原声明身份；继续完善
  ModuleId/DefId 在源码结构变化后的跨修订稳定性，不把会话内身份当作跨进程持久化格式。
- 增加 workspace symbol、rename、code action 等常用 LSP 能力。
- 为大型项目建立可重复的解析、索引和补全性能基准。

### 运行时与编译器

- 评估以源码 revision 缓存 entry `DefId` 与每模块 HIR，并为项目分析建立细粒度失效边界。
- 继续收缩 AST 解释器内剩余的类型兼容检查和名称查找逻辑。已迁入共享 frontend 的部分包括 trait
  impl associated type 声明契约、暂不支持的条件 trait impl 诊断、孤儿规则与项目内重复 impl 检查。
  后续仍应每次只迁移一类检查，以解释器/VM 对照测试证明行为不变，不把这项开放式清理作为其他
  feature 分支的退出条件。
- 标准 bytecode core import 已在链接时解析为稳定 ID；后续新增内建或外部 import 也应沿用该模式。
  `rils_runtime` 与 `rils_bytecode` 已形成单向依赖，根 `rils` 只保留兼容转发层。后续应继续收窄
  `rils_runtime::support`，把 bytecode/VM 所需的共享值、环境槽位、格式化和 builtin 操作整理成稳定
  的最小接口，不允许重新形成跨 crate 的双向依赖。
- 在已有统一指令步数与调用深度预算的基础上，继续增加堆、字符串、容器和宿主调用次数预算。
- 评估常量折叠、无效代码删除、分支简化和寄存器复用，并以基准数据决定是否启用。
- 完善字节码调试信息的可剥离 section、跨版本兼容策略和 fuzz 覆盖。
- 继续拆分职责过重的 Rust 模块，保持入口文件只包含模块声明、导出和薄入口。
  `rils_runtime::value` 已拆出 hash collection、range、reference 和 display/debug 子模块；后续仅在
  稳定运行时值 crate 边界明确后继续拆分 primitive、aggregate、callable 与 host value 声明。

### 基础类型与标准能力

- 增加结构化数值转换错误类型和更完整的浮点转换入口。
- 评估 HashMap/HashSet 的借用查询、索引 place 和借用迭代器，遵守 Rils 引用不能逃逸的规则。
- 将 BTreeMap/BTreeSet、VecDeque 和 BinaryHeap 接入借用迭代器，补齐 `iter_mut()`；逐步让迭代器适配器使用通用惰性实现，并弃用内部的 `SequenceIterator` 队列。
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
- 在已有 `rils.toml` 源码路径依赖上扩展二进制依赖声明、workspace 和锁定文件模型。
- 完善已有项目模块图的跨项目身份、循环诊断覆盖和可复现的构建缓存。
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

## 建议迭代拆分

近期顺序：标准库包诊断、工作区容错及共享源码导出查询已完成首轮；先补齐剩余重导出边界和跨修订
身份，再以稳定依赖关系实现增量失效；`.rilslib` 声明表和链接独立迭代。解释器清理和
性能优化持续按需推进，不作为标准库包分支的退出条件。源码路径依赖、模块图基础结构及语言包
依赖边已存在，后续项目模型工作应聚焦跨项目身份、二进制依赖、workspace/lockfile 和缓存边界。

以下工作存在依赖关系，但不应继续堆叠在同一个长期 feature 分支中。每组应从最新版本分支拉出
独立短分支，满足本组验收条件后及时合回：

1. Analyzer 语义查询：继续扩展已有共享导出查询，补齐重导出边界与跨修订身份，再分别实现 rename
   和 workspace symbol。
2. Frontend 增量缓存：以 source revision 缓存 entry `DefId`、项目分析和每模块 HIR，明确依赖图
   失效规则。
3. 模块与 crate 职责拆分：根 facade、`rils_runtime` 和 `rils_bytecode` 已完成首轮拆分；后续以
   独立分支收窄共享支持接口和项目 driver，不在同一分支混入新语义。
4. 解释器语义收缩：逐类迁移剩余类型、trait 和名称查找，保留动态宿主注册所需的运行时防御。
5. 库链接与依赖：单独设计并实现 `.rilslib` 声明表、稳定库身份、外部符号链接以及 workspace/
   lockfile；不得混入解释器清理或 VM 性能优化。

正确性修复优先于缓存、性能和新语法；大文件机械拆分应避开同一模块正在进行语义变更的分支。

## 记录规则

- 新条目应说明用户场景、影响范围和必要的兼容性约束。
- 设计尚未确定时记录候选方案，不把讨论稿当作语言规范。
- 能力完成后更新对应的正式文档、变更日志和测试，并从本文件移除。
