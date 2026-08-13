# Changelog

本文档记录 Rils 的用户可见变化和升级注意事项。尚未正式发布的内容始终放在最上方的
`Unreleased`；正式版本按 SemVer 从新到旧排列。

## Unreleased

### Breaking Changes

- `.rilbc` 格式由 v1 提升为 v3，以编码显式整数转换和稳定 ID 的 intrinsic 调用指令。已有 `.rilbc`/Unity `.bytes` 需要从
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
  `RilsRuntime.LoadBytecode(byte[])` 加载。`.rilbc` v3 包含 `usize/isize` 时与目标指针宽度相关，
  32 位和 64 位产物不能混用。

### Added

- string 增加 `len/is_empty/contains/starts_with/ends_with/find/trim/replace`；数组和 Vec 增加
  `is_empty`，Vec 增加 `clear/truncate`。这些方法共享内建声明，并已贯通解释器与字节码 VM。
- 数组和 Vec 增加不移动查询值的 `contains(&T)`；Vec 增加拥有型 `insert/remove/swap_remove/extend`，
  并在元素仍被引用时拒绝会使索引失效的重排操作。
- Option 增加 `expect/take`，Result 增加 `expect/ok/err`，以便在不手写 match 的情况下提供上下文错误、
  取出可选值或在 Result 与 Option 之间转换。
- 增加共享 `rils_builtins` crate，通过声明宏和递归类型表达式集中描述内建 module、primitive、
  struct、enum、trait、function、成员、稳定 intrinsic ID、实现后端和文档。首批整数 intrinsic、
  标准模块签名和 Analyzer 符号开始复用编译期静态表，不依赖外部标准库文件。
- 增加 `Target::try_from(integer)`、`to_f32/to_f64`，以及整数的 `checked_*`、`wrapping_*`、
  `saturating_*` 和 `overflowing_*` 加减乘方法；checked 除法和余数也已提供。
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
- 增加实验性 `.rilbc` 显式磁盘格式，当前为 v3，包含独立的格式版本、语言版本、宿主 ABI、目标指针宽度、
  section 目录、CRC32、严格资源上限和加载后 verifier。
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

### Changed

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
