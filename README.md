# Rils

Rils（Rust-Inspired Lightweight Script）是一门面向嵌入场景的轻量脚本语言，采用 Rust 风格语法、
显式所有权和可验证的字节码执行模型，并提供 Rust 宿主 API、静态分析器与 VS Code 支持。

## 快速开始

```console
rils --version
```

运行单文件脚本：

```console
rils examples/hello.rils
```

通过 `repl` 命令进入 REPL：

```console
rils repl
```

VS Code 和 CLI 工具链发行包均携带同源的 `rils_stdlib` 声明包，供 Analyzer 索引标准库签名与定义。
自定义安装可通过 `RILS_SYSROOT` 指定包含 `packages/rils_stdlib/rils.toml` 的目录；发现顺序和
工作区加载失败的处理方式见 [Analyzer 说明](docs/analyzer.md#标准库声明包)。

Rust 标准库中的固有方法用 `#[export_rils]` 导出，trait impl 用 `#[rils_impl]` 整体导出；
导出方法默认使用原生桥接，兼容旧入口时需显式声明绑定，详见 [标准库定义说明](crates/rils_stdlib/README.md)。
独立的 [`rils_value`](crates/rils_value/README.md) crate 提供按 Rust 布局存储的原生值与类型操作注册；小型 Copy 值直接内联，其他值使用共享存储。`Range<T>`、`i8`、默认整数类型 `i32`、`usize` 和 `string` 已在解释器与字节码 VM 中使用原生负载；整数方法由标准库声明生成注册，并通过类型化上下文调用 Rust 方法。`type_of(1..3)` 保留泛型参数，返回 `"Range<i32>"`。

所有标准库整数、`f32`、`f64` 和 `string` 的 `Option<T>` 在解释器与 VM 中使用按实际子类型布局组合的 `Value::Dynamic`；局部声明、函数实参、赋值、字段与嵌套表达式中可确定类型的 `None` 也使用该路径。数值类型的转换由标准库声明生成。Rust 宿主可用 `Value::as_option()` 统一读取新旧 Option 表示，读取已迁移的基础值可使用 `Value::as_i8()`、`Value::as_i32()`、`Value::as_usize()` 和 `Value::as_string()`。

原生值 crate 已支持运行时组合的 `Option<T>` 布局；整数、浮点数、`string` 与 `Option<T>` 的布局工厂从标准库声明生成。整数与 `string` 方法的原生对象注册由过程宏生成；整数方法返回的 `Option<T>` 已使用原生布局，`Option<T>` 自身的方法桥接当前通过过渡适配器读取原生负载，其他类型的可执行注册及实际值迁移仍在进行中。
项目中的公开源码声明可通过多层 `pub use` 重导出；Analyzer 的补全、Hover、跳转和引用查找
会追踪到原声明，并隔离不同项目中的同名符号。

## Rust 嵌入

脚本中的常用拥有型容器包括 `Vec<T>`、`VecDeque<T>`、`BinaryHeap<T>`、`BTreeMap<K, V>`、
`BTreeSet<T>`、
`HashMap<K, V>` 和 `HashSet<T>`。`BinaryHeap` 是最大优先队列，使用
`push/pop/peek_cloned` 处理整数、字符或字符串优先级；`BTreeMap` 按键排序。完整用法见
[集合章节](docs/language/05-data-types-and-collections.md)。
数组、`Vec<T>`、Map 和 Set 还提供 `iter()` 借用遍历，遍历后可继续使用原集合。数组和 `Vec<T>` 的拥有型 `into_iter()` 按 `next()` 的调用逐项移出元素。
Struct 和 enum 支持 `#[derive(Clone)]`、`#[derive(Copy)]`、`#[derive(Eq, Hash)]`；struct 也支持 `#[derive(Default)]`。`Clone` 逐字段调用对应 trait 实现，`Copy` 要求字段均为 Copy。
整数实现 `Clone`、`Copy`、`Default`、`Eq`、`Hash`；`f32`、`f64` 实现前三者；`string` 实现 `Clone`、`Default`、`Eq`、`Hash`。
泛型 trait 可声明类型参数；解释器可按函数签名和捕获行为检查标准库的 `FnOnce<Args, Output>`、`FnMut<Args, Output>`、`Fn<Args, Output>` bound。字节码编译器当前会明确拒绝这组三种 bound，直到共享前端完成相同的检查。Option/Result 的 `map`、`and_then`、`or_else` 等导出方法，以及 `core::ops::apply_twice`、`combine`、`chain` 等导出自由函数，已可在解释器和字节码中调用 Rils 函数或闭包；`Option::filter` 还可把共享引用交给谓词。回调错误保留源码位置。

实现 `Iterator` 的 Rils 类型只需声明 `Item` 并实现 `next`，可使用或重写标准库 trait 中定义的默认方法；解释器和字节码均从这些方法体执行，元素类型从关联类型 `Item` 解析。它还会自动实现 `IntoIterator`，`into_iter()` 返回自身，可直接用于 `for`。

```rust
let value = rils::eval("1 + 2 * 3")?;
let module = rils::compile("let value = 40; value + 2")?;
let value = module.execute()?;
```

解释器与字节码 VM 默认允许 1024 层脚本调用。嵌入方可通过
`Engine::set_max_call_depth` 或 `BytecodeModule::execute_with_limits` 配置调用深度和指令步数预算；
超过预算会返回运行时错误，不会继续递归直到宿主线程栈溢出。

字符串形式的 `eval` 和 `compile` 不会隐式访问文件。多文件加载、项目配置与预编译字节码分别参见
[项目模型](docs/project.md)和[字节码设计](docs/bytecode.md)。

## Unity 嵌入

Unity 项目通过独立的 [RilsForUnity](https://github.com/rils-lang/RilsForUnity) 包接入。Editor 会把
`.rils` 源文件导入为经过验证的字节码资产，并为其中的 `RilsBehaviour` 实现生成可挂载入口；在场景中
添加 `RilsBehaviour` 组件并指定对应入口资产，即可由 Unity 生命周期驱动脚本。Player 只加载字节码，
不需要携带 Rils 源码或 Rust 工具链。

当前集成面向 Unity 2022.3 LTS 和 Windows x86_64。Unity API 调用必须位于创建运行时的主线程；
跨边界目前支持基础标量、UTF-8 字符串、固定布局 Unity 值类型、真实 enum 与 session 绑定的 Unity
对象句柄；集合仍需通过宿主 API 或自定义绑定转换。
安装、脚本模板和生命周期示例参见
[RilsForUnity 使用说明](https://github.com/rils-lang/RilsForUnity/blob/main/Packages/com.rils-lang.rils-for-unity/README.md)，
底层对象所有权与线程边界参见 [Unity 互操作边界](docs/unity-interoperability.md)。

## 文档

- [可运行示例](examples/README.md)
- [安装与环境包](docs/installation.md)
- [语言手册](docs/language/README.md)
- [项目模型](docs/project.md)
- [项目依赖与打包](docs/project-dependencies-and-packaging.md)
- [Rils 库产物](docs/library-artifacts.md)
- [编译器架构与迁移边界](docs/compiler-architecture.md)
- [Analyzer 与编辑器能力](docs/analyzer.md)
- [字节码设计](docs/bytecode.md)
- [发布与分支流程](docs/release-process.md)
- [C API 与 Host Manifest](docs/capi/README.md)
- [Unity 互操作边界](docs/unity-interoperability.md)
- [示例程序](examples)
- [VS Code 插件](editors/vscode-rils)
- [未来规划与待办](TODO.md)
- [变更日志](CHANGELOG.md)

当前代码处于 `0.4.0` 阶段，适合语言实验、工具开发和受控宿主嵌入。Unity、UE 等引擎集成由
各自独立的插件工程维护，不属于 Rils 核心仓库的版本标准。

标准库导出保留数组的元素类型与 `const N: usize` 长度约束。IO 的 `write` / `write_line`
使用 `T: Display`，支持内建值和用户定义的 `Display` 实现；未实现该 trait 的类型会被拒绝。
参见[泛型签名](docs/language/06-impl-generics-and-traits.md)与[IO 输出](docs/language/08-modules-and-standard-library.md)。
