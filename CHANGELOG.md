# Changelog

本文档记录 Rils 的用户可见变化和升级注意事项。尚未正式发布的内容始终放在最上方的
`Unreleased`；正式版本按 SemVer 从新到旧排列。

## Unreleased

## 0.3.0 - 2026-08-22

### Breaking Changes

- Host Manifest 二进制与 JSON 格式提升为 v2，新增命名宿主类型、单继承和逻辑类型到 ABI transport
  的映射。Runtime、CLI 和 Analyzer 仍可读取 v1，重新导出或链接时会写为 v2。
- C ABI 从 version 2 提升为 version 4。version 3 引入 trait implementation 枚举和 opaque script
  value 生命周期接口；version 4 新增 `rils_runtime_register_host_types` 和
  `rils_runtime_register_host_functions_v2`。使用命名宿主类型的调用方必须先注册类型，再以逻辑类型
  与 transport 分离的参数结构注册函数；`rils_script_value_call_trait` 同步增加逐参数 transport/逻辑
  类型描述数组。仅使用 primitive/`HostHandle` 的宿主函数旧注册入口继续兼容。
- `.rilbc` 格式由 v4 提升为 v5，新增经过 verifier 校验的 trait implementation 表；已有 `.rilbc`、
  Unity `.bytes` 和 `.rilslib` 内嵌模块需要从源码重新编译。

### Migration

- 从 0.2.x 升级后，使用源码重新生成所有 `.rilbc`、Unity `.bytes` 和内嵌字节码的 `.rilslib`；v5
  loader 会明确拒绝旧格式文件。
- 重新导出或链接 Host Manifest，使其写为 v2。v1 Manifest 仍可被 Runtime、CLI 和 Analyzer 读取，
  但不再应作为新发布产物。
- C/C# 宿主必须将 native DLL、`rils.h` 生成的绑定和 `Rils.CSharp` facade 成套更新至 C ABI v4。
  使用命名宿主类型时，先注册类型表，再通过 v2 函数注册接口提供逻辑类型与 transport 描述；持久
  script value 的 trait 调用也要传入逐参数描述数组。

### Added

- 增加解释器与字节码 VM 共享的 `ExecutionLimits`，默认将脚本调用限制为 1024 层。AST 解释器会
  按需增长栈段，字节码 VM 继续使用显式帧；两者超限时均返回可诊断错误，不再依赖宿主线程栈崩溃。
- Host Contract、编译器、Runtime、Analyzer 和 C# facade 现已贯通命名宿主对象类型与单继承；派生
  类型可传给基类参数并调用继承的 receiver 方法，当前在 C ABI 上统一使用 `HostHandle` transport。
- C# trait 调用可为参数携带逻辑宿主类型；RilsForUnity 生命周期回调现以
  `unity_engine::GameObject` 暴露宿主对象，而不是丢失类型信息的裸 `HostHandle`。
- `Rils.CSharp` 增加与 handler 分离的宿主模块/函数 descriptor、确定性稳定 ID 和单模块
  `.rilhm` builder；Unity 集成可由同一份 Binding IR 生成编译期契约并注册 Player 静态绑定。
- 增加内建 `Default` trait 和 `#[derive(Default)]`。基础标量、tuple、数组、`Option` 与空集合具有统一默认值；Struct 派生会检查每个字段的 `Default` 约束，并由解释器、字节码编译器和 Analyzer 共享同一展开结果。
- Trait 支持声明 supertrait；解释器、编译器和 Analyzer 会统一要求 impl target 满足全部 supertrait。RilsForUnity 的 `RilsBehaviour` 现在继承 `Default`，新建模板会自动添加 `#[derive(Default)]`。
- impl 内的 `Self` 现在统一解析为当前具体类型，支持在解释器和字节码中使用 `Self { ... }` 与
  `Self::associated(...)`；Analyzer 会显示具体类型声明，并为类型及关联方法提供定义跳转。
- 字节码、C API 与 C# facade 支持发现具体 trait 实现、通过 `Default` 构造持久 opaque script value，
  并以 trait 方法身份连续调用；RilsForUnity 生命周期不再依赖源码正则或同名模块函数。

- 增加实验性 `.rilslib` 库容器、`rils library compile/verify` 命令和严格加载校验；库项目自身的
  `[lib].prelude` 现在会参与独立库编译。当前阶段先支持显式导出与验证，二进制依赖链接仍在后续实现。
- RilsForUnity 在 Editor 启动时自动校验内置 Host Manifest；缺失、损坏或与当前绑定不一致时会
  原子重建并重新导入 `.rils` 资产，不再要求用户先执行生成菜单。
- `compile_file` 允许把库自身 `[lib].prelude` 作为资产入口编译，保证只注入一次并同时加载库项目；
  RilsForUnity 同步增加空脚本和完整 `RilsBehaviour` 模板的右键创建菜单。
- Struct 现在支持 `struct Name;` 单位声明和 `struct Name {}` 零字段声明；RilsForUnity 的
  `RilsBehaviour` 创建模板使用 prelude 自动导入的 trait，不再生成显式 `use`，并会把重名或含空格的
  文件名规范化为合法 Rils 模块标识符。

- `use` 增加公开成员通配导入和递归分组导入，支持 `use path::*;`、别名、嵌套
  `use path::{item, child::{nested, other}};`。
- Analyzer 增加项目级跨文件公开导出索引，编辑器可以在工作区范围内解析和跳转符号。
- Unity 运行时 facade 和导出流程迁移到独立的 `RilsForUnity` 项目，Rils 主仓库保持宿主无关。
- Host interop 增加 session-bound `HostHandle`、C# 宿主注册桥接，以及 Unity 生命周期测试支持。
- Host Manifest 支持 `.rils/manifest/**/*.rilhm` 多 fragment 自动发现和兼容片段合并；冲突声明
  仍然报告错误。

### Fixed

- Analyzer Hover 现在会标注类型的定义模块，以及 field/enum variant 所属的类型；struct 和 enum
  声明最多展示前 8 个成员，并明确提示剩余数量，避免大型类型产生过长的 Hover。
- Analyzer 的项目索引现在会跨模块保留公开函数的完整签名与展示声明；通过显式、分组或通配
  `use` 导入的函数可参与调用结果推导，Hover 不再显示未知类型，未标注的局部变量也会恢复
  `: Type` inlay hint。编辑导出文件时会先分析最新文本再刷新项目索引，避免类型信息短暂丢失。
- Struct 字段声明、成员访问与 `Type { field: value }` 实例化字段现在会显示 `field name: Type`；
  Analyzer 会根据 receiver 或构造类型关联字段定义，支持同文件及跨模块定义跳转，并为 impl 内的
  `self`、`&self` 与 `&mut self` 引用保留实际 receiver 类型。Windows 上来自 VS Code 的等价文件
  URI 会先统一到项目索引使用的形式，避免同一打开文件被重复分析后丢失字段类型和跳转信息；
  对引用 receiver 的字段借用也会保留完整类型，例如推导为 `&mut Vec<Task>` 而非 `&mut _`。
- Analyzer 现在会从数组、集合、Range 以及自定义 `Iterator` / `IntoIterator` 的关联类型推导
  `for item in iterable` 的循环绑定类型，并显示 `item: Type` inlay hint。内建方法的嵌套泛型返回
  类型会保留具体实参，因此 `values.into_iter()` 可显示完整签名并继续推导循环项类型；VS Code
  也会按普通类型相同的 scope 高亮 `Vec`、`HashMap` 等内置类型，并在源码及 Hover 代码块中
  用独立的 enum member scope 高亮 enum variant；方法 receiver `self` 在声明和方法体中统一按关键字高亮。
- Analyzer 现在会在补全列表和 Hover 中显示用户函数、泛型类型、trait、固有方法及 enum variant 的完整
  声明；泛型 receiver 的方法返回值会按实际类型实参推导。跨文件 `use` 导入保留真实符号类别，
  record/unit/tuple enum variant 在构造与 match 模式中均可正确识别并跳转。
- 宿主类型现在会在 frontend 名称解析阶段统一规范化完整身份；`use module::*`、显式类型导入和
  `as` 别名可用于函数参数、字段及嵌套类型，编译器与 Analyzer 不再因短名丢失继承 receiver 方法。
  未导入的宿主短名和多个通配导入产生的同名类型会在 lowering 前给出明确诊断。
- Trait implementation 元数据现在保留声明 `SourceId`；RilsForUnity 导入项目级 module 时仅为当前
  `.rils` 源文件创建 entry 子资产，不再把其他脚本的 `RilsBehaviour` 实现重复挂到每个主资产下。

## 0.2.0 - 2026-08-16

### Breaking Changes

- `.rilbc` 格式由 v1 提升为 v4：除显式整数转换和稳定 ID 的 intrinsic 调用指令外，v4
  增加来源文件表，并让所有持久化 Span 携带 `SourceId`。已有 `.rilbc`/Unity `.bytes` 需要从
  源码重新生成。

- 移除原有的 `int` 和 `float` 类型名。整数与浮点数现在使用明确的 Rust 风格类型：
  `i8`、`i16`、`i32`、`i64`、`i128`、`isize`、`u8`、`u16`、`u32`、`u64`、`u128`、
  `usize`、`f32` 和 `f64`。
- 无约束整数与浮点字面量分别默认推断为 `i32` 和 `f64`。数组、Vec、tuple 字段和集合长度相关
  的索引统一使用 `usize`。
- Rust 宿主侧原有的通用整数/浮点 Value 表达已替换为具体数值 variant；依赖旧 `Value::Integer`
  或 `Value::Float` 的宿主代码需要迁移到对应的具体类型。

### Migration

- 将脚本中的 `int` 替换为实际需要的整数类型；一般业务整数使用 `i32`，索引和集合长度使用
  `usize`。
- 将脚本中的 `float` 替换为 `f32` 或 `f64`；未显式约束的小数字面量仍默认使用 `f64`。
- 更新宿主函数签名、Analyzer 断言和序列化代码，使其保留具体数值类型，不再执行隐式整数/浮点
  扩宽。
- Unity 发布流程应在 Editor 或构建阶段生成 `.rilbc`/`.bytes`，Player 通过
  `RilsRuntime.LoadBytecode(byte[])` 加载。`.rilbc` v4 包含 `usize/isize` 时与目标指针宽度相关，
  32 位和 64 位产物不能混用。

### Added

- SourceId 已贯通 lexer/parser、静态分析、HIR/MIR、字节码格式与 verifier、解释器、Analyzer、
  CLI 和 C API。项目及兼容模块加载会为每个脚本分配确定的来源标识，跨文件编译和运行错误可报告
  实际依赖文件；Analyzer 的符号 ID、定义与引用也保留文件身份。
- 增加 `HashMap<K, V>` 与 `HashSet<T>` 的拥有型运行时实现、共享内建声明和字节码 core imports。
  首版支持标量 `Eq + Hash` 键、CRUD、Map 的拥有型查询/键值迭代、Set 集合代数以及消费式 `for`；
  Analyzer 同步补全 `std::collections`、构造函数和实例方法。
- string 增加 `len/is_empty/contains/starts_with/ends_with/find/trim/replace`；数组和 Vec 增加
  `is_empty`，Vec 增加 `clear/truncate`。这些方法共享内建声明，并已贯通解释器与字节码 VM。
- string 增加 Unicode 大小写、首尾裁剪、重复、反向查找、前后缀剥离，以及
  `chars/bytes/lines/split` 拥有型迭代；内建迭代器增加 `count/last/nth/collect_vec/take/skip/rev`，
  可链式使用并直接进入 `for`。Analyzer 补全和解释器/字节码 VM 使用同一声明。
- `Iterator` 增加通用默认方法 `map/filter/filter_map/fold/for_each/any/all/find/position/enumerate`；
  现有消费与适配方法也可由自定义 `Iterator` 实现复用。谓词查询支持短路，`filter/find` 通过
  `&Item` 检查非 Copy 元素。Analyzer 同步提供内建和自定义迭代器的方法补全。
- 数组和 Vec 增加不移动查询值的 `contains(&T)`；Vec 增加拥有型 `insert/remove/swap_remove/extend`，
  并在元素仍被引用时拒绝会使索引失效的重排操作。
- Option 增加 `expect/take`，Result 增加 `expect/ok/err`，以便在不手写 match 的情况下提供上下文错误、
  取出可选值或在 Result 与 Option 之间转换。
- Option 增加 `or/xor/replace`，覆盖备用值选择、互斥存在判断和原地替换；编译器 lowering 现在保留
  receiver 的静态类型，用于可靠区分不同内建类型上的同名方法。
- Result 增加 `unwrap_err/expect_err`，使错误侧提取与现有成功侧 `unwrap/expect` 保持对称。
- Option 增加惰性的 `map/and_then/or_else`，Result 增加 `map/map_err/and_then/or_else`；回调签名支持
  方法级泛型推断，解释器与字节码 VM 行为一致，Analyzer 可直接从共享内建目录提供补全和签名。
- 增加共享 `rils_builtins` crate，通过声明宏和递归类型表达式集中描述内建 module、primitive、
  struct、enum、trait、function、成员、稳定 intrinsic ID、实现后端和文档。首批整数 intrinsic、
  标准模块签名和 Analyzer 符号开始复用编译期静态表，不依赖外部标准库文件。
- 增加 `Target::try_from(integer)`、`to_f32/to_f64`，以及整数的 `checked_*`、`wrapping_*`、
  `saturating_*` 和 `overflowing_*` 加减乘方法；checked 除法和余数也已提供。
- 整数增加位计数、前导/尾随零、旋转、`pow`、`div_euclid/rem_euclid` 和 `abs`；同时补齐
  neg/abs/pow 的 checked、wrapping、saturating、overflowing 模式，并覆盖 signed MIN、除零和
  幂溢出边界。
- 整数增加 `MIN/MAX/BITS` 关联常量、`swap_bytes/reverse_bits`，以及 shift 的 checked、wrapping、
  overflowing 模式；共享声明同步驱动类型推断、解释器、字节码 VM 和 Analyzer 补全。
- f32/f64 增加分类与符号判断、取整、`abs/signum/copysign`、`sqrt/recip`、`min/max/clamp` 和
  `mul_add`，并提供 `MIN/MAX/EPSILON/MIN_POSITIVE/NAN/INFINITY/NEG_INFINITY` 关联常量；解释器、
  字节码 VM、类型检查和 Analyzer 复用同一浮点 intrinsic 目录。
- 增加 Rust 风格整数 `as` 表达式以及 `1_i32` 形式的分隔后缀。静态检查只接受不会缩小类型范围
  的转换；`i32 as usize` 对负值运行时报错，`usize as i32` 等潜在有损转换直接拒绝。解释器、
  HIR/MIR、字节码 VM 和 Analyzer 使用同一规则。

- 增加共享 `rils_project` 项目模型和 `rils.toml`：支持项目名、多个 `script_paths` 以及可选
  `[host].manifest`，并按脚本相对路径自动建立模块目录。项目入口必须提供零参数 `fn main()`；
  项目模式不再使用外部 `mod name;`，无项目文件时保留旧加载行为。
- 增加 `crate::`、`self::`、`super::` 路径锚点，并同步解释器、静态分析、字节码符号解析和 VS Code
  高亮。Analyzer 可按项目目录及 `use` 别名补全子模块和目标文件的公开声明。
- Host Manifest 支持 `.rils/manifests/**/*.rilhm` 多 fragment 自动发现，以及 `rils.toml` 的
  `manifests` / `manifest_dirs`。Analyzer、源码编译与 CLI 复用确定性严格合并规则；新增
  `rils host-manifest link` 将开发期 fragments 链接为 Player 使用的单一 `.rilhm`。

- 增加编译期 `HostContract`、`compile_with_host`/`compile_file_with_host` 和
  `BytecodeModule::validate_host`。自定义宿主函数现在参与名称解析、类型检查、数值字面量约束和
  bytecode import 生成，不再局限于硬编码标准库导入。
- 增加严格、版本化的 Host Manifest v1 `.rilhm` 二进制格式，包括独立格式头、去重字符串表、固定
  函数记录、紧凑类型表、资源上限和覆盖规范 payload 的 FNV-1a-128 契约哈希。JSON 保留为显式
  Editor/工具交换格式，并可通过 `rils host-manifest compile/export-json` 转换。
- 增加 C ABI 的批量宿主函数注册、独立 capability 授权、统一 dispatcher、注册表冻结和显式
  module 宿主校验，以及二进制 Manifest 注册/导出。第一阶段反向调用支持 `()`、`bool`、
  `i32/u32/i64/u64` 和 `f32/f64`，并拒绝 dispatcher 重入 C API。
- 增加全部定宽整数、`isize`/`usize`、`f32`/`f64` 和 `char`，并支持根据标注、参数、返回值、
  运算及索引用法约束无后缀字面量。
- 增加实验性 `.rilbc` 显式磁盘格式，当前为 v4，包含独立的格式版本、语言版本、宿主 ABI、目标指针宽度、
  来源文件表、section 目录、CRC32、严格资源上限和加载后 verifier。
- 增加 `BytecodeModule::to_bytes/from_bytes/write_file/read_file`，以及 CLI 的 `compile`、`verify` 和
  `run` 子命令。
- 增加 `compile_file` 的递归外部模块编译，以及按名称调用公开字节码函数的宿主入口。
- 增加宿主无关的 `rils_capi` Windows x86_64 动态库、线程绑定 generation 句柄、panic 边界、
  标量值协议、源码编译、字节码加载、字节码内存/文件导出和实例调用接口。
- 增加 `.NET Standard 2.1` 的 `Rils.CSharp` facade，包括 `Compile/CompileFile`、
  `GetBytecode/WriteBytecodeFile`、`LoadBytecode/LoadBytecodeFile` 和标量转换。
- 增加 Python C# 绑定生成器、Windows C API 构建脚本，以及 Unity drop-in 包导出脚本。Unity 包将
  C# 源码放在 `Rils.CSharp/`，原生库放在 `Internal/x86_64/`。
- 增加 C API/C# 真实 DLL 冒烟测试、损坏字节码拒绝测试和编译—导出—加载往返测试。
- Analyzer 和 VS Code 插件可加载经过 verifier 的 `.rilhm`，宿主函数参与诊断、类型推断、hover 与
  语义符号；在模块路径的 `::` 后提供子模块/函数补全，并展示签名和 capability，支持模块 `use` 别名。

### Fixed

- 修复宏表达式片段对未闭合 `[]` 前缀执行解析时可能栈溢出的问题；`assert!(values[index] == text)`
  现在会稳定展开，并由所有权检查报告非 Copy 索引移出错误。

### Changed

- 重组 `examples/`：相关的基础语法合并为可验证单文件场景，并新增任务看板与遥测流水线两个标准
  多模块 Rils 项目；确定性示例现在同时校验解释器、字节码 VM 和固定返回值。
- Analyzer 为每个工作区文件分配稳定 `SourceId`，并以 `SymbolId` 关联定义和引用；定义跳转与查找引用
  不再通过名称猜测，能够区分局部遮蔽、不同文件的同名声明，并追踪项目模块中的公开函数引用。
- Analyzer 的 `.` 补全现在基于 receiver 表达式推断类型，并直接枚举共享内建目录中的 string、数组、
  Vec、Option、Result 等成员；LSP 同时声明 `.` 为补全触发字符，并能在成员名尚未输入完整时恢复分析。
- Analyzer 增加 signature help：输入 `(` 或 `,` 时可显示用户函数、Host Manifest 函数和内建方法的
  参数签名与当前参数位置，并能处理尚未闭合的调用、嵌套调用及集合字面量中的逗号。
- Parser 现在会在宏片段匹配前报告未闭合或错配的 `()`、`[]`、`{}`，避免编辑未完成调用时出现
  递归栈溢出。
- 内建目录现在统一描述现有 Option、Result、数组、Vec、Range、Iterator 与 Clone 成员的签名、receiver
  所有权和稳定运行时 ID；frontend 的成员类型、所有权分析、内建 arity、Analyzer 全局符号与解释器
  分派开始直接读取同一目录。编译器和 VM 的 core import 签名也由该目录生成，Option/Result 的
  `is_some/is_none/is_ok/is_err/unwrap/unwrap_or` 方法现已与解释器保持一致。
- `Vec::len()` 和相关集合长度接口返回 `usize`。
- 解释器、静态分析、HIR、MIR、字节码 VM、Analyzer、VS Code 语法高亮、示例和语言文档同步使用
  具体数值类型。
- C# 绑定代码统一命名为 `Rils.CSharp`；`rils_capi` 保持宿主无关，Unity 特化留在托管层与项目侧。

## 0.1.0

### Added

- 初始 Rils 语言、树遍历解释器、静态分析、HIR/MIR/字节码 VM 和 Analyzer。
- Rust 风格的显式所有权、词法局部引用、函数与闭包、struct/enum、trait/impl、泛型、模式匹配、
  模块、宏、数组、Vec、Option、Result 和迭代器基础能力。
- VS Code 语法高亮与语言服务器支持，以及用于验证解释器和字节码一致性的示例与测试。
