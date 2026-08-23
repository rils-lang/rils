# 字节码与预编译设计

Rils 的执行后端分为两条路径：树遍历解释器负责快速验证完整语言语义；字节码后端把已经稳定的
语义逐步固化为更紧凑、可验证、可重复执行的表示。两条路径在迁移期间并存，并用相同源码的结果
对照测试防止语义漂移。

## 当前实现

第一阶段已经贯通以下流水线：

```text
source -> lexer/parser -> static analysis -> HIR -> MIR -> bytecode -> verifier -> VM
```

当前 crate 边界中，`rils_frontend` 负责解析和静态分析，`rils_compiler` 负责 HIR/MIR lowering；主
crate 消费 MIR 并完成字节码编码、验证和 VM 执行。encoder 迁移前需先让字节码类型表摆脱运行时
`StructType`/`EnumType`，改为可独立验证和链接的静态描述。

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

impl 方法会登记为带 receiver 元数据的模块函数。关联函数、struct/enum 成员方法、trait impl 和
`<Type as Trait>::method(...)` UFCS 均进入普通调用帧；`self`/`mut self` 执行 move，`&self`/
`&mut self` 自动创建相应引用。泛型方法与泛型函数一样采用静态检查后的类型擦除执行。

内联模块中的函数和用户类型会扁平化为稳定的 `module::symbol` 链接名，多段路径调用、嵌套模块、
模块限定的 struct/enum 构造和 `use path [as alias]` 已进入字节码后端。模块函数解析未限定名称时
优先查找自身命名空间，因此不同模块的同名私有辅助函数不会错误互链。模块声明和导入本身不生成
运行时指令。模块内 impl、关联函数、成员调用和模块内 UFCS 使用同一限定符号表。项目模式下
`compile_file` 按 `rils.toml` 的 `script_paths` 建立完整模块目录，归一化 `crate/self/super` 路径，
并调用所选文件的零参数 `fn main()`；无项目文件时保留 `name.rils` / `name/mod.rils` 规则。

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
跨工具交换使用严格、确定性排序的 [Host Manifest v4](capi/host-manifest.md)，不直接序列化 Rust 结构。

当前覆盖边界汇总如下：

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 表达式与控制流 | 已支持 | 运算、块、if、while、loop、for、break/continue、return、match |
| 函数 | 已支持 | 直接/任意表达式间接调用、递归、函数值、绑定方法值、嵌套闭包、可变捕获、泛型类型擦除 |
| 复合类型 | 已支持 | tuple、数组、Range、Option/Result、struct/enum、类型别名 |
| 所有权与引用 | 已支持基础层 | move/Copy、解引用、reborrow，以及引用根和 struct/tuple/数组/Vec 混合投影链的读取、赋值和局部借用 |
| Trait 与方法 | 已支持 | 关联函数、四种 self、任意 receiver、trait impl、UFCS/UFCS 函数值、模块内 impl |
| 模块 | 已支持 | 内联模块、use/as、多段路径及 `compile_file` 外部模块链接 |
| 迭代器 | 部分支持 | Range、数组、Vec 和自定义 Iterator/IntoIterator；借用迭代器待实现 |
| 标准库/宿主 | 部分支持 | core/Vec、内置宏、显式授权的 std::io/std::fs，以及编译期自定义 HostContract 已链接；解释器 Engine 与同一契约的整合待完成 |
| 磁盘预编译 | 实验可用 | `.rilbc` v6、bytes/file API、CLI compile/verify/run；尚未承诺跨版本稳定 |

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

当前已实现实验性 `.rilbc` v6。它采用带版本的显式小端容器，不直接序列化任何 Rust enum、地址或
内存布局：

```text
magic | format version | language version | host ABI | pointer width | flags | section directory | CRC32
```

v6 包含 module、imports、types、iterators、functions、sources 和 trait implementations 七个必需
section。trait implementations 表以受 verifier 校验的类型名、trait 名、声明 SourceId、方法名和函数索引保留实现身份，
宿主无需扫描源码或猜测函数名即可发现入口并精确分发 trait 方法。sources 表只
保存确定性 `SourceId -> 来源名称` 映射，不嵌入源码正文；常量、指令和源码 Span 使用各自的显式
tag/字段编码，每个 Span 都携带 SourceId。加载器限制文件为 64 MiB、单个字符串为 1 MiB、通用集合为一百万项、
函数/类型/导入表各 65,536 项、总指令两百万条、单函数寄存器和局部槽位各 262,144 个、类型/模式
嵌套为 128 层，并验证目录边界、重叠、UTF-8、标量范围和 CRC32。集合还必须与 section 剩余字节数
相符，避免小文件用伪造 count 触发不成比例的预分配。解码完成后仍必须通过
现有 verifier，函数、常量、类型、导入、寄存器、局部槽位和跳转索引都不会被信任。未知必需
section 拒绝加载，未知可选 section 在完成边界验证后跳过。

由于语言当前存在 `usize`/`isize`，v6 记录目标指针宽度，32 位和 64 位产物不允许交叉加载，避免
发生静默截断。`format version`、`language version` 和 `host ABI` 分别检查。当前格式仍处于 0.4.0
实验期，后续不兼容调整会提升格式版本；尚未承诺长期跨版本兼容。

格式 v6 在 v5 的 trait implementation 表之外增加带显式 `IntegerType` 的 `IntegerBinary` 指令。
静态分析无法证明类型的运算、浮点运算和字符串拼接仍使用通用 `Binary` 指令。旧 loader 不会误读
这些内容；升级后的 loader 也会明确拒绝旧文件，项目需要从源码重新生成 `.rilbc`/`.bytes`。

CLI 入口为：

```console
rils compile scripts/main.rils -o scripts/main.rilbc
rils verify scripts/main.rilbc
rils run scripts/main.rilbc
```

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
