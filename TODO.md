# Rils TODO

## Native boxed references

- Add Rils `Deref` / `DerefMut` support for `Box<T>` so scripts can borrow its child with `*box` and mutate it through a lexical reference. `Box::new` and `into_inner` already move the child through the owned native call bridge.

本文档记录尚未完成的优化、新特性和生态工作。条目按主题归类，不绑定具体版本；实际排期
会根据使用场景、兼容性和测试结果调整。已完成的能力应从这里移除，并同步到正式文档。

## 泛型 trait bound 一致性

- `Display` / `Debug` 的调用约束已进入共享前端；字节码编译路径仍需补齐其他泛型函数调用的 trait bound。目前解释器会拒绝将 `Option<string>` 传给 `T: Copy`，但字节码路径尚未拒绝同一调用；应在共享前端完成检查，并增加解释器与 VM 的失败路径对照测试。
- `decl_rils` 导出方法时需保留 `where` 约束并交由共享前端检查；当前 `Cell<T>::get()` 的 `T: Copy` 只能在执行时拒绝不匹配的值。

- 数组签名已支持 `const N: usize` 与长度推导；后续扩展 const 表达式、函数体内的 const 值读取与显式 const 实参。

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

- Rust 执行入口已返回 `RilsValue`，原生组合值和 `Vec<T>` 元素引用现可经 `with_native_view` 沿布局借用读取；继续把脚本内部 `ReferenceValue::read`、`contains`、方法分发及其他容器接到该视图，消除对拥有型 `Value` 的依赖。用户结构体映射的 derive 暂缓，后续放入已确定的 `rils_host_macros`，生成临时 Rust 对象的移出、异常恢复和写回代码。`Vec<T>` 的布局提升、索引引用及迭代器创建已移除 `can_read_element` 门槛；迁移内部调用后删除仅供兼容的原始 `Value` 返回入口。
- 标准库类型已有原生布局注册，下一步按数据类别删除旧 `Value` 变体及兼容构造。`StoredDataRef` 已区分 Native、Dynamic、Host 数据负载，`TypedStorageContext` 已统一类型化局部绑定、记录字段、tuple enum 字段、可从实参推断具体类型的函数参数与返回声明的布局提升，并支持用户 struct/enum 作为 Option/Result 子值。`?`、拥有型 match、回调返回和用户迭代器消费原生分支；组合对象的注册消费操作保留构造处的声明上下文。继续把无法从实参推断类型的泛型实例、剩余赋值边界、方法分发和词法引用接到同一类型见证及借用视图；消除仍会经 `Value` 快照的适配器，再清理旧集合与数值变体。
- 已建立原生布局的 Option/Result 在局部槽位赋值、初始化字段和元组/数组元素替换及可变引用写回时保留布局；局部槽位 move 后的重新赋值也保留操作表。声明类型和透明别名已由共享前端解析，跨模块字段及直接 `None` 保留具体类型身份。继续为最初使用旧表示的目标建立具体布局，包括类型化元组/数组初始化时递归提升内部的用户类型 Option/Result。
- `rils_value` 已支持内联 Copy、动态 Option/Result、记录、数组及拥有型序列布局。用户 struct/enum 声明可在类型化容器和字段构造时参与布局解析，但独立用户实例仍主要保存在旧 `Value::Struct` / `Value::Enum`；后续须贯通直接的原生字段 place、嵌套引用、部分 move、Copy/Clone 与析构，并保持解释器和 VM 一致。
- 原生字段的消耗式编解码已覆盖基础值、Option、Result、tuple、数组、用户 struct/enum，以及由同一标准库声明生成布局的 Vec、VecDeque、BinaryHeap、HashMap、HashSet、BTreeMap 和 BTreeSet 泛型实例，并验证嵌套用户结构可以直接置于 bytes 中。复合路径已支持 Copy 读取、字段 move 和依照具体布局赋值，执行层可直接在原生记录上编码新值并取回旧负载。尚未接入解释器和 VM 的 `StructFields`；接入前仍需为宿主值、引用、函数等合法字段类型建立声明驱动的布局/转换，统一共享值与引用投影的所有权规则，不能静默回退到 `Value` 负载。
- 评估以源码 revision 缓存 entry `DefId` 与每模块 HIR，并为项目分析建立细粒度失效边界。
- 继续收缩 AST 解释器内剩余的类型兼容检查和名称查找逻辑。已迁入共享 frontend 的部分包括 trait
  impl associated type 声明契约、暂不支持的条件 trait impl 诊断、孤儿规则与项目内重复 impl 检查。
  后续仍应每次只迁移一类检查，以解释器/VM 对照测试证明行为不变，不把这项开放式清理作为其他
  feature 分支的退出条件。
- 标准库原生调用已统一使用声明生成的规范符号；后续在加载时将原生导入链接为进程内调用槽位。宿主 import 继续使用独立的 ABI 契约。
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

- `FnOnce`、`FnMut`、`Fn` 已导出，解释器按签名和捕获行为检查 bound；继续把捕获能力分析移到共享前端，让字节码路径也拒绝不满足的 bound，并静态约束 `FnOnce` 泛型回调的重复调用。Option/Result 方法与导出自由函数已从普通 `Fn*` Rust 实现生成隐藏的可失败桥接；继续扩展回调调用的宏改写范围、其他方法 receiver 的原生桥接及引用、容器等值类型转换。
- 完善泛型 trait 身份：普通泛型 trait 实例可作为 bound 使用，同一类型对同一 trait 的不同类型实参可分别实现，限定关联类型与 trait UFCS 路径保留并校验实参，coherence 和方法表按实例身份区分。
- 将标准库 `Iterator -> IntoIterator` 的 blanket 关系推广为用户可声明的带条件 trait impl，统一泛型匹配、关联类型投影、跨模块 coherence 和解释器/VM 分派。
- 为标准库原生桥接补齐 `&mut self` 的 place 代理与写回，以及泛型返回值的类型见证和所有权转换；完成后移除相应旧 ID 适配。
- 增加结构化数值转换错误类型和更完整的浮点转换入口。
- 评估 HashMap/HashSet 的借用查询与索引 place，遵守 Rils 引用不能逃逸的规则。
- 将 VecDeque 和 BinaryHeap 接入借用迭代器，补齐 `iter_mut()`；让 `take/skip/enumerate/map/filter` 等适配器在调用 `next()` 时再计算元素。目前拥有型数组、Vec、VecDeque 和 BinaryHeap 已按需取值，字符串迭代也按需生成元素；默认适配器仍会预先收集结果。
- 补充常用字符串解析、格式化和 Unicode 操作，但不引入隐式深拷贝。

## 新特性

### 语言

- 继续扩展 trait 定义中的派生生成器，覆盖泛型 `Copy` 的条件 impl。
- 将现有 `Debug` 的派生逻辑从 `rils_syntax` 的固定名称分支迁入对应 trait 的注册生成器，使所有内建 derive 使用同一入口。扩展 `Eq`、`Hash` 派生的字段检查，支持已实现相应 trait 的命名字段和泛型条件 impl。

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
- 将剩余的模块树与 prelude `.rils` 声明接入 Rust 定义，设计 Analyzer 对新定义的源码导航支持，再统一移除重复语言包。
- 扩展 `decl_rils` 原生桥接，继续统一泛型值、引用、可变 receiver 和回调转换；将现有执行层适配器逐步收回到导出定义附近，并对缺少实现的导出方法报错。
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
