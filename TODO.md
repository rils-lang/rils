# Rils TODO

本文档记录尚未完成的优化、新特性和生态工作。条目按主题归类，不绑定具体版本；实际排期
会根据使用场景、兼容性和测试结果调整。已完成的能力应从这里移除，并同步到正式文档。

## Rils 原生布局与泛型存储

Vec、VecDeque、BinaryHeap、Map、Set 的现有构造、查询、修改和迭代已统一执行标准库方法体；
多参数条目布局和借用字段投影来自存储声明。存储协议与调用边界见
[标准库文档](crates/rils_stdlib/README.md)和[声明宏文档](crates/rils_stdlib_macros/README.md)。
后续布局工作沿用这些访问边界，无需先补另一套集合专用桥接。

- 已确定方向：Rils 自行定义标量、struct、enum、数组和泛型容器的进程内布局，不要求等价于 Rust 对应类型；Rust 互操作在边界显式转换。进程内布局不是字节码或 C ABI 的持久化格式。
- `Vec<T>` 的目标是按具体 T 的大小和对齐连续保存元素，容器共享元素描述；不以 `Vec<DynamicValue>` 或逐元素引用作为最终表示。类型实例化只要求生成布局与操作绑定，不要求生成每个实例的机器码。
- 待实施的协议：分开表示不可变类型布局、拥有型存储、初始化/部分 move 状态与借用凭据。record 的字段存活标记不再占据用户字段负载；数组以元素步长描述，不为每个索引建立一份字段描述。enum 判别标记属于值布局，初始化标记属于运行时状态。
- 通用内存层提供按布局分配、原位构造、移动、析构和受控投影；stdlib 定义容器头布局、增长策略、集合算法及迭代行为。execution 仅组织 Value 类别、词法引用和脚本调用；不增加 Vec、Map 等具体类型分支。
- 首批验证连续 Vec：同时修改元素投影、借用检查、构造/取出和消费式迭代。`into_iter` 接管原缓冲区与剩余初始化区间，不预先展开成 `Vec<DynamicValue>`。边界临时值可以拥有独立存储，但不能成为容器内逐元素的常驻包装。
- 当前物理布局仍需重构：`SequenceStorage.items` 是 `Vec<DynamicValue>`；record 的负载带字段初始化字节；独立 `None` 可能只分配标记字节。只读索引和消费式迭代已收拢到 `IndexedView` / `OwnedElements`，后续替换其内部存储；新增按布局的移动入口必须按有效分支和初始化状态操作，不能假定所有旧值都已分配完整布局并直接整块读取。
- 验证矩阵覆盖零大小/高对齐元素、容量溢出、部分构造失败、非 Copy 精确析构、嵌套 struct/enum/Vec、引用阻止地址失效及 Rust 边界转换。增加可重复 release benchmark，分别测元素区大小、分配次数和操作耗时；不能仅以功能测试证明已实现紧凑存储。

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

- Rust 执行入口已返回 `RilsValue`，原生组合值和集合元素引用可沿布局借用读取，内建集合查询与方法桥接已统一使用受控存储视图；继续检查脚本内部 `ReferenceValue::read`、其他方法分发及宿主边界，消除不必要的拥有型 `Value` 转换。用户结构体映射的 derive 暂缓，后续放入已确定的 `rils_host_macros`，生成临时 Rust 对象的移出、异常恢复和写回代码。`Vec<T>` 的布局提升、索引引用及迭代器创建已移除 `can_read_element` 门槛；迁移内部调用后删除仅供兼容的原始 `Value` 返回入口。
- `Value` 迁移以删除旧表示为阶段终点，交叉影响的构造、存储、调用、引用、返回和宿主接口一起修改；不为维持旧测试增设过渡兼容层。允许中间态测试未迁完，发布前必须恢复完整验证。所有标量、Option/Result、tuple/数组及全部内建集合的旧表示、两种旧 Map/Set 借用迭代器已删除；累计删除 49 项，旧枚举由本轮迁移开始的 52 项降至 3 项。IndexedStorage、FieldSlot、HashKey/KeyIdentity、旧集合 receiver、引用投影及比较路径也已删除。
- OwnedIterator、BorrowedIndexedIterator、BytecodeIterator 已接入 Native 注册，删除对应旧 Value 分支及拥有型迭代器的 Value 缓存；空迭代器和耗尽后的 None 保留元素布局与声明，自定义 Native 描述符、非 Copy 析构和引用来源已有测试。Reference 已接入原生引用叶子布局并删除旧枚举分支；独立引用、容器内引用、解释器/VM 投影、借用模式、Copy 和宿主读取共用原生句柄。十种函数、绑定方法和构造器分支已从 Value 删除，容器编码中的 Callable 枚举也已删除；统一使用 Dynamic callable 句柄，由具体 CallTarget 提供签名、显示及捕获值遍历，后端按具体目标执行。声明绑定已与值槽位分离：TypeDeclarations 拥有声明，名称解析返回 Binding，模块导出保留声明身份；StructType/EnumType/TraitType/TypeAlias/HostType/BuiltinType/Module 已移出 Value，声明无法进入 StorageSlot。Value 当前仅保留 Native/Dynamic/HostObject 三类物理存储。构造器使用 callable 目标，实例只保留布局与声明身份；保留 Value 作为三类存储的统一载体，清理其中残留的旧表示和兼容转换。
- 继续检查局部槽位、VM 寄存器、原生调用和 `RilsValue` 的剩余转换入口；当前统一载体已只有 Native、Dynamic、HostObject 三类，不再依赖旧类型大枚举。
- Vec/Option/Result 快照入口及按标量类型生成的 Option 快照桥接已删除，观察统一使用 with_native_view；继续清理 `ReferenceValue::read` 等拥有型读取入口，并保留声明身份、显式 Copy/Clone、词法来源及失败前状态检查。未能从实参确定的泛型实例、剩余赋值与方法分发边界仍需完整类型见证。
- 已建立原生布局的 Option/Result 在局部槽位赋值、初始化字段和元组/数组元素替换及可变引用写回时保留布局；局部槽位 move 后的重新赋值也保留操作表。声明类型和透明别名已由共享前端解析，跨模块字段及直接 `None` 保留具体类型身份。类型化元组/数组初始化已递归提升内部用户类型的 Option/Result，并保留具体数组元素声明。未标注的局部绑定现使用前端推导类型提升存储；解释器的模块类型已保留完整声明路径，未导入类型、私有字段及前向声明可参与内部布局解析。整体元组/数组替换保留原生描述符并复用消费式转换；旧递归槽位声明已删除；构造及 if/match/block 的直接表达式结果也已按推导类型提升存储。独立用户 struct/enum 已接入原生存储与字段/变体 place；普通 struct/enum 构造已直接写入原生布局；继续迁移剩余方法与词法引用中的旧值适配。
- `rils_value` 已支持内联 Copy、动态 Option/Result、记录、数组及拥有型序列布局。普通独立用户 struct 已按具体类型存入原生布局；嵌套字段 move/恢复、用户方法 receiver 与拥有型/借用 record 模式复用原对象及完整路径，借用模式绑定能观察原字段后续写入。原生集合方法及借用迭代器通过受限可变视图访问字段，避免克隆容器后修改副本。独立 enum 的变体投影、拥有型/借用模式、receiver、Debug 与哈希键已接入原生路径；原生 Option/Result 借用模式已复用活动分支投影，保留原负载与词法来源，Copy 容器按需转移共享所有者；普通 struct/enum、IO 错误和宿主 enum 已移除临时兼容构造；旧 `Value::Struct` / `Value::Enum`、实例包装与记录字段槽位已删除；原生 tuple/数组解码和字段移出已保留原生布局，索引、迭代、数组转 Vec 与嵌套投影均复用原生存储；空数组保留元素声明与 Copy 策略。tuple/数组字面量与默认值也已直接构造原生布局，整数方法的 tuple/Option/Result 输出已统一使用类型化原生构造器；旧 tuple/数组与 Vec 变体及 IndexedStorage 已删除；全部集合旧存储已删除，继续清理快照适配。自定义 Native 负载已由 NativeType 自动注册组合叶子布局及双向拥有型转换，覆盖非 Clone 移动、析构与显式 Copy；原生集合输出已保留执行层描述符上下文，字段移出后保留方法和子值访问注册，声明冲突或未知类型明确报错；后续执行对象迁移不得退回 Unknown 或内嵌 Value。
- 原生字段的消耗式编解码已覆盖基础值、Option、Result、tuple、数组、用户 struct/enum，以及由同一标准库声明生成布局的 Vec、VecDeque、BinaryHeap、HashMap、HashSet、BTreeMap 和 BTreeSet 泛型实例，并验证嵌套用户结构可以直接置于 bytes 中。复合路径已支持 Copy 读取、字段 move 和依照具体布局赋值，执行层可直接在原生记录上编码新值并取回旧负载。引用与函数值现已由执行层注册叶子布局和消费式转换，Copy 保留受管理句柄，原生存储内的引用来源检查遍历存活叶子；宿主布局接口已从 HostType 声明读取 Copy 策略并验证析构。宿主声明上下文已自动贯通解释器、VM、原生构造与 C API 契约加载，保留 Copy 策略、宿主对象身份及枚举 flags。共享执行层已提供声明驱动的原生构造器和 `NativeInstancePlace`，覆盖原生 struct/enum 的字段 move、递归部分 move 检查、恢复、Copy 独立所有者、引用租约与析构；拥有型转换拒绝共享非 Copy 输入。`RilsValue` 的字段与声明查询已直接使用原生路径。普通独立 struct/enum 已贯通这套共享 place；enum 解码保留原生负载与声明元数据，构造器已按完整类型见证直接移入原生字段，泛型调用和返回闭包保留类型绑定；旧名义类型兼容转换已删除，后续迁移其他执行适配器。用户 struct/enum 的 Copy 已统一为显式声明，并检查所有变体字段与嵌套类型；能力随 trait 表保留至 VM 和加载后的字节码，未声明的类型不自动 Copy。后续清理兼容转换与快照适配器沿用这套规则，不能静默回退到 `Value` 负载。
- 补齐字节码编译的两处集合/引用边界：借用 Map 迭代条目直接以 `entry.1.method()` 调用用户方法仍可能错误地保留外层引用，先绑定 `let item = entry.1` 后调用可用；通过 tuple 内 `&mut T` 借用子字段时，不应仅因 tuple 绑定不可变而拒绝，需按实际引用路径判断可变性，并补充与解释器的对照测试。
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

- Add Rils `Deref` / `DerefMut` support for `Box<T>` so scripts can borrow its child with `*box` and mutate it through a lexical reference. `Box::new` and `into_inner` already move the child through the owned native call bridge.

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
