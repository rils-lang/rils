# 字节码与预编译设计

Rils 的执行后端分为两条路径：树遍历解释器负责快速验证完整语言语义；字节码后端把已经稳定的
语义逐步固化为更紧凑、可验证、可重复执行的表示。两条路径在迁移期间并存，并用相同源码的结果
对照测试防止语义漂移。

## 当前实现

VM 的局部槽位在初始化原生 Option/Result 时保留具体类型和操作表；`StoreLocal` 及引用写回复用目标声明，移出负载不会清除局部槽位的声明见证。原生字段及元组/数组元素的槽位同样独立保留声明见证，移出后重新赋值与引用写回复用共享的消费式转换。此迁移不改变指令或 v8 编码。

类型声明在进入 HIR 前由共享前端解析定义模块、导入和透明别名；struct/enum 字段及函数签名保存解析后的具体类型路径，`None` 的子类型也保留推断结果中的声明身份。多文件编译复用整项目声明解析器，v8 类型编码复用现有 `Type` 编解码，无新增字段或指令。

第一阶段已经贯通以下流水线：

```text
source -> lexer/parser -> static analysis -> HIR -> MIR -> bytecode -> verifier -> VM
```

当前 crate 边界中，`rils_frontend` 负责解析和静态分析，`rils_compiler` 负责 HIR/MIR lowering；
`rils_bytecode` 消费 MIR 并完成字节码编码、磁盘格式、验证、VM 和 bytecode host，单向依赖
`rils_runtime` 提供的值模型与运行时操作；根 `rils` 仅作公共转发。磁盘类型表仍使用可独立验证和
链接的静态描述，加载后才构造运行时 `StructType`/`EnumType`。

- HIR 完成词法作用域名称解析，把局部变量转换为稳定槽位，并保留源码范围。
- MIR 使用寄存器和基本块显式表示值流与控制流。
- 编码器把基本块展平为指令流，并解析跳转目标。
- 验证器在执行前检查常量、局部槽位、寄存器和跳转目标。
- VM 每次执行创建独立的寄存器、局部槽位和显式调用帧，因此同一模块可以安全地重复执行。
- `execute_with_limit` 保留只配置指令步数的便捷入口；`execute_with_limits` 和
  `execute_with_host_and_limits` 使用共享 `ExecutionLimits` 同时配置指令步数与调用深度。
- 调用栈默认限制为 1024 帧。字节码 VM 使用显式帧；AST 解释器按需增长独立栈段。两者都会在
  超限时返回带源码位置的错误，而不是继续递归直到宿主线程栈溢出。
- 模块导入按稳定名称、签名、宿主 ABI 版本和 capability 链接；校验在 VM 启动前完成。

当前支持常量、局部 `let`/`let mut`、局部赋值、基础一元/二元运算、短路逻辑、块表达式、
`if`、`while`、`loop`、`break value`、`continue`、函数、参数、直接命名调用、递归、函数值、
间接调用、嵌套函数、词法闭包和 `return`。迭代控制流支持 Range、拥有型数组、`Vec`、`HashMap`、
`HashSet` 及脚本自定义 `Iterator` / `IntoIterator` 的 `for`，包括 `break value` 与 `continue`。
`BinaryHeap<T>` 的构造、插入、弹出、堆顶克隆和清空通过稳定 core builtin ID 调用共享运行时。
类型化局部绑定中的 `VecDeque<T>` 和 `BinaryHeap<T>` 使用动态原生序列负载；`InitLocal` 指令新增可选声明类型，让 VM 在初始化时取得具体元素布局。未冻结的 v8 编码因此改变，旧 `.rilbc` 文件需从源码重新编译，格式号仍为 v8。
同一类型信息还用于补齐 `Result<T, E>` 局部值未出现分支的类型见证，使其能继续嵌入 `Option<Result<T, E>>` 的动态布局。
`Option<T>`、`Result<T, E>` 和可能需要布局见证的命名类型参数，会在 HIR/MIR 降低时生成函数入口的 `TakeLocal`/`InitLocal`，直接使用已有指令完成参数负载转换；执行时可从实参推断其中的具体泛型实参，标量参数不增加这些指令。VM 构造 tuple enum 字段时也使用声明类型与推断出的泛型实参组合布局。此变化不新增 v8 操作码或字段。

functions 表现保存与参数数量一致的可选类型表及可选返回类型。HIR 保留解析后的用户类型路径；VM 从实参推断返回声明中的泛型参数，在普通返回和 `?` 提前返回时按具体声明组合原生负载。参数类型表受局部槽位数量上限约束，verifier 校验表长及递归类型中的 SourceId。`?`、拥有型 `match` 和用户迭代器消费原生分支，不要求用户负载实现 Clone。此调整改变尚未冻结的 v8 functions 编码；旧 `.rilbc` 需重新编译，格式号不递增。
`BTreeMap<K, V>` 同样经共享 core builtin ID 执行；`for` 的有序迭代复用现有拥有型迭代器值。
`BTreeSet<T>` 的集合运算和升序迭代也沿用共享 core builtin 与同一迭代器表示。
复合值已覆盖 tuple、数组、重复数组、Range、Option 和 Result，以及局部 tuple/数组的索引读取
与写入、tuple 数字字段读写。函数内的 `?` 会在 VM 调用帧上直接传播 Err。局部和参数读取沿用
Rils 的显式所有权语义：Copy 值复制，其他值 move；索引读取也会对非 Copy 元素执行部分 move。
`match` 已支持字面量、通配、变量绑定、Some/None、Ok/Err，以及 struct/enum 模式编码。
类型别名在静态阶段展开，泛型函数当前采用静态检查后的类型擦除字节码。

VM 局部槽位已经改为带可变性、移动状态和活动引用计数的共享存储。字节码支持局部变量与数组
元素的 `&T`/`&mut T`、引用参数、Copy 值解引用读取和 `*reference = value`，并在词法块退出时
发出显式局部清理指令，避免不可见引用继续占用所有者。

模块内的 struct/enum 定义会进入独立类型表。当前支持 struct record、enum unit/tuple/record variant
构造，局部 struct 字段的 move/Copy 读取、写入和借用，以及 struct/enum 的 record、tuple 和
unit 模式解构。泛型字段暂以运行时实际值补全字段槽位类型，完整类型参数表留待方法链接阶段。
已解析的用户结构体字段 place 现在保存类型表索引与声明顺序的字段索引；verifier 检查两级索引，VM 执行时再次校验实际接收对象的名义类型与字段声明。解释器和 VM 的结构体字段槽位按声明顺序存放，VM 字段 place 和词法字段引用按索引直接定位槽位。类型擦除的泛型 receiver 和不在本模块类型表中的外部类型保留按名称访问。字段索引是稳定的声明位置，平台相关的字节偏移不写入磁盘。未冻结的 v8 投影编码增加索引标签；旧文件的按名投影仍可读取，重新编译后才会生成索引投影。

impl 方法会登记为带 receiver 元数据的模块函数。关联函数、struct/enum 成员方法、trait impl 和
`<Type as Trait>::method(...)` UFCS 均进入普通调用帧；`self`/`mut self` 执行 move，`&self`/
`&mut self` 自动创建相应引用。泛型方法与泛型函数一样采用静态检查后的类型擦除执行。

内联模块中的函数和用户类型会扁平化为稳定的 `module::symbol` 链接名，多段路径调用、嵌套模块、
模块限定的 struct/enum 构造和 `use path [as alias]` 已进入字节码后端。模块函数解析未限定名称时
优先查找自身命名空间，因此不同模块的同名私有辅助函数不会错误互链。模块声明和导入本身不生成
运行时指令。模块内 impl、关联函数、成员调用和模块内 UFCS 使用同一限定符号表。项目模式下
`compile_file` 按 `rils.toml` 的 `src` 建立完整模块目录，归一化 `crate/self/super` 路径，
并调用所选文件的零参数 `fn main()`；无项目文件时保留 `name.rils` / `name/mod.rils` 规则。
跨模块 struct/enum pattern 会在 HIR 中规范化为声明模块的完整类型路径，VM 使用精确名义路径匹配，
不会把不同模块中短名称相同的类型视为同一类型。

函数值由函数表索引和捕获槽位组成。HIR 提升嵌套函数并形成显式捕获列表，MIR/字节码分别使用
创建闭包和按值调用指令；捕获槽位与外层 frame 共享，因此可变捕获在多次调用间保持状态，返回
闭包后仍然有效。验证器检查函数索引、捕获布局和寄存器，间接调用在执行时检查 arity。引用不能
进入闭包环境。无须函数值的具名调用仍保留直接调用指令。任意返回函数值的表达式都可直接作为
callee；UFCS 方法可作为未绑定函数值，Copy 的按值 receiver 可形成绑定方法值。非 Copy 的按值
receiver 和引用 receiver 只允许立即调用，不能进入可复制的绑定方法值。

方法 receiver 不再要求是单独的局部变量：临时值、嵌套字段和索引 place 都可调用方法。字段/索引
投影也可从引用根开始，`&*reference` / `&mut *reference` 通过显式 reborrow 指令保留原目标及
可变性约束。

确定性的 core/prelude 函数和 Vec 基础操作通过导入表调用。默认 `BytecodeHost` 提供 core；
`std::io` 与 `std::fs` 的实现可分别启用，并受同名 capability 控制。编译期 `HostContract` 可声明
自定义宿主函数的稳定 ID、完整名称、固定签名和 capability；`compile_with_host` /
`compile_file_with_host` 会让这些声明参与静态分析并生成普通 import，运行前由
`BytecodeModule::validate_host` 或正常执行路径链接到 `BytecodeHost`。自定义
`Iterator`/`IntoIterator` 的脚本方法登记在模块迭代表中，`for` 会通过普通 VM 调用帧驱动它们。
标准库 `Iterator` 默认方法从 trait 声明的方法体编译为普通函数，不为新字节码写入这些方法的旧数字 ID；
`next` 仍由运行时原语推进。Iterator 默认方法的旧 ID 只用于读取历史字节码。
旧 Sequence 的六个数字 ID（`0x0100` 至 `0x0105`）已移除；读取包含这些调用的旧
`.rilbc` v8 文件会报无效内建 ID。当前 v8 尚未定型，因此格式号保持 v8，旧文件应从
源码重新编译；Unity `.bytes` 和嵌入 `.rilslib` 的字节码模块也应重新生成。
跨工具交换使用严格、确定性排序的 [Host Manifest v5](capi/host-manifest.md)，不直接序列化 Rust 结构。

当前覆盖边界汇总如下：

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 表达式与控制流 | 已支持 | 运算、块、if、while、loop、for、break/continue、return、match |
| 函数 | 已支持 | 直接/任意表达式间接调用、递归、函数值、绑定方法值、嵌套闭包、可变捕获、泛型类型擦除 |
| 复合类型 | 已支持 | tuple、数组、Range、Option/Result、struct/enum、类型别名 |
| 所有权与引用 | 已支持基础层 | move/Copy、解引用、reborrow，以及引用根和 struct/tuple/数组/Vec 混合投影链的读取、赋值和局部借用 |
| Trait 与方法 | 已支持 | 关联函数、四种 self、任意 receiver、trait impl、UFCS/UFCS 函数值、模块内 impl |
| 模块 | 已支持 | 内联模块、use/as、多段路径及 `compile_file` 外部模块链接 |
| 迭代器 | 部分支持 | Range、数组、Vec、借用集合迭代器和自定义 Iterator/IntoIterator；适配器目前会预先收集结果 |
| 标准库/宿主 | 部分支持 | core/Vec、内置宏、显式授权的 std::io/std::fs，以及编译期自定义 HostContract 已链接；解释器 Engine 与同一契约的整合待完成 |
| 磁盘预编译 | 实验可用 | `.rilbc` v8、bytes/file API、CLI compile/verify/run；尚未承诺跨版本稳定 |

Rust 宿主入口如下：

```rust
let module = rils::compile("let mut n = 1; while n < 5 { n = n + 1; } n")?;
println!("instructions: {}", module.instruction_count());
let value = module.execute()?;

let game_scripts = rils::compile_file("scripts/main.rils")?;
let image = game_scripts.to_bytes()?;
let loaded = rils::BytecodeModule::from_bytes(&image)?;
game_scripts.write_file("scripts/main.rilbc")?;
let loaded_file = rils::BytecodeModule::read_file("scripts/main.rilbc")?;

let io_script = rils::compile("std::fs::try_exists(\"save.dat\")")?;
let mut host = rils::BytecodeHost::standard();
host.enable_standard_fs()?;
let result = io_script.execute_with_host(&host)?;

let mut contract = rils::HostContract::new();
contract.register_function(
    100,
    "unity_engine::time::frame_count",
    rils::FunctionSignature::fixed(Vec::new(), rils::Type::Integer(rils::IntegerType::U64)),
    "unity.time",
)?;
let game = rils::compile_with_host("unity_engine::time::frame_count()", &contract)?;
let mut game_host = rils::BytecodeHost::new(rils::BYTECODE_HOST_ABI_VERSION);
game_host.allow_capability("unity.time");
game_host.register_function(
    "unity_engine::time::frame_count",
    rils::FunctionSignature::fixed(Vec::new(), rils::Type::Integer(rils::IntegerType::U64)),
    "unity.time",
    |_| Ok(rils::Value::U64(42)),
)?;
game.validate_host(&game_host)?;
```

不在当前子集中的合法语义会返回带源码范围的 `CompileError`，不会静默退回解释执行。当前 AST
中的表达式、控制流和模块级声明语法均已有字节码路径；剩余边界是借用迭代器、尚未实现的
借用迭代器，以及解释器 Engine 注册内容与 `HostContract` 的统一描述。运行时错误使用
`BytecodeError`，同样可以通过 `render`
生成带源码位置的诊断。

## 磁盘格式

当前已实现实验性 `.rilbc` v8。它采用带版本的显式小端容器，不直接序列化任何 Rust enum、地址或
内存布局：
`Range<T>` 等原生负载在加载后由运行时构造，字节码只保留构造指令与类型信息；所有整数宽度及 `f32`、`f64` 常量加载时构造成原生内联值。原生类型描述和 Rust 分配器所有权不进入磁盘格式。
`BuildOptionNone` 指令携带可选的元素类型；存在具体类型时，VM 按标准库登记的子类型布局构造原生 `Option<T>`。该指令编码已在未冻结的 v8 内调整，此前生成的 v8 文件必须重新编译。

```text
magic | format version | language version | host ABI | pointer width | flags | section directory | CRC32
```

v8 包含 module、imports、native imports、types、iterators、functions、sources 和 trait implementations 八个必需
section。trait implementations 表以受 verifier 校验的类型名、trait 名、声明 SourceId、方法名和函数索引保留实现身份，
宿主无需扫描源码或猜测函数名即可发现入口并精确分发 trait 方法。`Eq`、`Hash` 的 marker 实现沿用此表，
方法列表为空；VM 加载后据此恢复类型的哈希键资格，不增加磁盘格式字段。sources 表只
保存确定性 `SourceId -> 来源名称` 映射，不嵌入源码正文；常量、指令和源码 Span 使用各自的显式
tag/字段编码，每个 Span 都携带 SourceId。加载器限制文件为 64 MiB、单个字符串为 1 MiB、通用集合为一百万项、
函数/类型/导入表各 65,536 项、总指令两百万条、单函数寄存器和局部槽位各 262,144 个、类型/模式
嵌套为 128 层，并验证目录边界、重叠、UTF-8、标量范围和 CRC32。集合还必须与 section 剩余字节数
相符，避免小文件用伪造 count 触发不成比例的预分配。解码完成后仍必须通过
现有 verifier，函数、常量、类型、导入、寄存器、局部槽位和跳转索引都不会被信任。未知必需
section 拒绝加载，未知可选 section 在完成边界验证后跳过。

由于语言当前存在 `usize`/`isize`，格式会记录目标指针宽度，32 位和 64 位产物不允许交叉加载，避免
发生静默截断。`format version`、`language version` 和 `host ABI` 分别检查。当前格式仍处于 0.4.0
实验期，标准库迁移期间的内部布局仍会调整并沿用 v8；待迁移完成后再固定 v8 布局，当前尚未承诺长期跨版本兼容。
类型表使用标签 `18` 编码切片类型 `[T]`，供 `&[T]` 参数使用；旧 v8 文件应从源码重新编译。
泛型参数的 trait bound 现按结构化类型编码，包含 `Fn<(T,), U>` 等类型实参；此前生成的实验性 v8 文件需重新编译。
`FnOnce`、`FnMut`、`Fn` bound 当前会在字节码编译阶段以带范围的错误拒绝，等待共享前端补齐捕获行为检查后再允许编译。

格式 v8 为已迁移的标准库原生方法使用独立的 native imports 表，表项保存规范符号路径和签名；
`CallNative` 按表索引调用生成的 Rust 桥接实现。verifier 检查符号、签名、实参数和索引。
Option/Result 的回调方法及带原生适配器的标准库自由函数也使用此指令；VM 把 Rils 函数值交给原生桥接时，在同一模块和调用预算内执行回调，并保留回调中的错误位置。原生导入表与 verifier 从导出声明校验自由函数的擦除签名。
标准库 runtime 成员、数值 intrinsic 和 `Clone` 调用会以规范路径写入 native imports，并编码为 `CallNative`；带目标类型的整数关联函数将目标宽度写入符号路径。尚未实现独立原生桥接的成员由符号分发层转接现有运行时实现。旧实验字节码的 `CallRuntime` / `CallIntrinsic` 操作码已移除，包含这两种指令的文件需要重新编译。
loader 仍拒绝未知或不支持对应调用方式的 ID。`std` 和宿主 Manifest 导入使用独立的 host imports 表。

格式 v6 在 v5 的 trait implementation 表之外增加带显式 `IntegerType` 的 `IntegerBinary` 指令。
静态分析无法证明类型的运算、浮点运算和字符串拼接仍使用通用 `Binary` 指令。旧 loader 不会误读
这些内容；升级后的 loader 也会明确拒绝旧文件，项目需要从源码重新生成 `.rilbc`/`.bytes`。

CLI 入口为：

```console
rils compile scripts/main.rils -o scripts/main.rilbc
rils verify scripts/main.rilbc
rils run scripts/main.rilbc
rils run scripts/main.rils
```

`run` 可以直接执行 `.rils` 源文件，也可以执行 `.rilbc` 字节码；传入项目目录时会加载其 `rils.toml`。
`compile` 仍不访问文件；CLI 和 `compile_file` 采用统一的项目/兼容模块加载规则，将入口和模块
链接为一个 `.rilbc`。C# 的 `RilsRuntime.LoadBytecode(byte[])` 可直接消费 AssetBundle 或
Addressables 中的 bytes，不要求发布包保留 `.rils` 文件布局。
Unity Editor 也可通过 `RilsModule.GetBytecode()` 或 `WriteBytecodeFile(path)` 将 C API 编译得到的内存
module 导出为构建产物。

格式版本和语言版本分开维护：前者描述二进制编码兼容性，后者描述脚本语义。首个磁盘格式只有在
函数调用、复合值、模块依赖和宿主 ABI 进入字节码后端后才冻结，避免早期实现细节成为长期包袱。

## 运行边界

游戏嵌入场景还需要独立配置最大指令数、调用深度、堆内存、容器长度和宿主 IO 能力。预编译文件
只消除前端解析与降低成本，并不天然意味着可信；加载器验证和运行时资源预算都必须保留。

未来的格式、性能和运行时增强统一记录在仓库根目录的 [TODO.md](../TODO.md)，不作为当前字节码
格式承诺。

导出签名的类型编码还保留符号数组长度（`[T; N]`）、推导出的 `usize` const 实参及泛型
trait 约束。泛型参数记录区分类型参数与 `const usize` 参数。IO 的 `Display` 约束经过
序列化和加载仍保留，由通用参数转换入口调用格式化 trait。该调整属于尚未冻结的 v8；
此前生成的 v8 文件应重新编译，不递增到 v9。
