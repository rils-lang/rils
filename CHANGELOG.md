# Changelog

本文档记录 Rils 的用户可见变化和升级注意事项。尚未正式发布的内容始终放在最上方的
`Unreleased`；正式版本按 SemVer 从新到旧排列。

## Unreleased

### Breaking Changes

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
  `RilsRuntime.LoadBytecode(byte[])` 加载。`.rilbc` v1 包含 `usize/isize` 时与目标指针宽度相关，
  32 位和 64 位产物不能混用。

### Added

- 增加全部定宽整数、`isize`/`usize`、`f32`/`f64` 和 `char`，并支持根据标注、参数、返回值、
  运算及索引用法约束无后缀字面量。
- 增加实验性 `.rilbc` v1 显式磁盘格式，包含独立的格式版本、语言版本、宿主 ABI、目标指针宽度、
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

### Changed

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
