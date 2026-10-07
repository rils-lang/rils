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

函数直接返回的具体 `Option<T>` / `Result<T, E>` 已接入组合原生存储，包含实参可确定泛型的 `None` / `Err`。`?`、`match`、回调组合器和用户迭代器按所有权取出非 Clone 内容；构造处注册的消费操作保留跨模块用户类型声明。字节码保存参数与返回类型；旧实验性 v8 文件需重新编译。

局部绑定的原生存储使用显式标注或前端推导类型；布局可解析时，未标注的用户类型 Option/Result 及嵌套元组/数组也会提升。模块类型未被导入时仍可解析布局，内部私有字段和前向声明也能参与；不同模块的同名类型按完整路径区分，`type_of` 与 VM 保持一致。类型化元组/数组初始化会递归提升内部用户类型的 Option/Result，涵盖具体泛型参数、返回值和透明别名。已建立原生布局的 Option/Result 在局部变量、字段及元组/数组元素替换时保留布局；可变引用写回使用同一消费式入口。局部变量、字段及元组/数组元素的声明操作表独立于负载保留，move 后重新赋值也可复用。整体元组/数组替换同样保存递归声明，涵盖嵌套元素、整个负载 move 和多个可变引用依次写回，不保留旧负载。直接表达式以及 if/match 返回的 Option/Result、嵌套元组/数组也按前端推导类型提升，不要求先绑定局部变量。

前端推断、类型检查和 HIR 使用共享的声明类型解析器，保留定义模块内的完整类型路径，并展开参数、返回值及字段中的透明类型别名。跨模块原生 Option/Result 字段替换、直接赋值 `None` 和链式泛型别名返回值在解释器、VM 与字节码加载后执行中保持一致。
独立的 [`rils_value`](crates/rils_value/README.md) crate 提供按 Rust 布局存储的原生值与类型操作注册；小型 Copy 值直接内联，其他值使用共享存储。`Range<T>`、所有标准库整数宽度、`f32`、`f64` 和 `string` 已在解释器与字节码 VM 中使用原生负载；Rust 宿主通过 `Value::from_i16` / `Value::as_i16` 等对应宽度的方法构造和读取整数，浮点与字符串分别使用 `from_f32` / `as_f32`、`from_f64` / `as_f64`、`from_string` / `as_string`。整数与浮点方法由标准库声明生成注册，并通过类型化上下文调用 Rust 方法。`type_of(1..3)` 保留泛型参数，返回 `"Range<i32>"`。

子类型有可递归解析的布局时，`Option<T>` 在解释器与 VM 中使用按实际子类型布局组合的 `Value::Dynamic`；这包括基础值、嵌套标准库容器，以及类型化局部绑定、记录字段、tuple enum 字段和可从实参推断具体类型的函数参数中的用户 struct/enum。具体类型的 `Result<T, E>` 也在这些绑定处组合原生分支布局。局部声明、函数实参、赋值、字段与嵌套表达式中可确定类型的 `None` 使用原生路径；`Some(value)` 消耗原值。Rust 宿主可用 `RilsValue::with_native_view()` 借用组合负载，用 `Value::as_option()` 读取可转换的新旧 Option 表示；基础值可通过 `Value::as_i8()`、`Value::as_i32()`、`Value::as_usize()` 和 `Value::as_string()` 读取。

词法引用和函数值也有执行层注册的原生叶子布局：`Option<&mut T>`、`Result<fn(T) -> U, E>`、`Vec<fn(T) -> U>` 及包含它们的泛型记录可以参与原生组合存储。Copy 保留引用租约或函数身份，闭包的捕获状态继续共享；原生字段内的引用仍按来源检查作用域，不能因进入组合存储而逃逸。宿主声明已接入相同上下文：Option/Result、空集合、Box、Rc 和 Cell/RefCell 构造可递归组合宿主字段，保留声明的 Copy 策略与对象身份。Rust 宿主可用 `Engine::register_host_contract_types()` 和 `BytecodeHost::register_host_contract()` 安装 Manifest 类型；C API 冻结宿主契约时自动接入。`register_native_type()` 的自定义类型使用完整模块路径，默认非 Copy。

原生值 crate 已支持运行时组合的 `Option<T>`、用户结构体、tuple、定长数组、带标签变体和拥有型序列布局；执行层可从已有 struct/enum 声明解析具体泛型实例的字段偏移和变体负载。带具体标准库元素布局的 `VecDeque<T>` 与 `BinaryHeap<T>` 在解释器和 VM 的类型化局部绑定中已使用动态原生序列存储；元素有可解析布局的类型化空 `Vec<T>` 也已接入，Rust 宿主可通过 `Value::as_vec()` 读取新旧表示。入队、出队、堆插入、堆弹出及清空等操作直接访问原生负载；支持 Clone 的组合元素端点克隆与容器克隆按原生布局递归读取负载。类型化局部绑定中的用户定义元素可在布局可解析时使用原生路径；其他构造上下文仍可能走旧容器路径。字节码中的本模块用户结构体字段 place 已使用经过验证的声明索引，解释器和 VM 的字段槽位也按声明顺序存放。独立用户 struct/enum 已按具体类型存入原生布局；容器内原生用户记录的嵌套字段与索引引用已直接沿布局路径访问 bytes，支持 RefCell、借用迭代器、tuple/array/Vec 子字段、参数与方法 receiver 写回。整数、浮点数、`string` 与 `Option<T>` 的布局工厂从标准库声明生成。整数与 `string` 方法的原生对象注册由过程宏生成；两类方法返回的 `Option<T>` 已使用原生布局，`Option<T>` 自身的方法桥接当前通过过渡适配器读取原生负载，其他类型的可执行注册及实际值迁移仍在进行中。
`VecDeque<T>` 和 `BinaryHeap<T>` 可通过 `into_iter()` 或 `for` 消费并遍历；队列保持队首到队尾的顺序，堆遍历不保证排序。集合实现 `IntoIterator`，产出的迭代器实现 `Iterator`。`Vec<T>` 的拥有型迭代接管原有元素存储，`iter()` 则借用元素并保留集合；字符串迭代按需生成下一项。

项目中的公开源码声明可通过多层 `pub use` 重导出；Analyzer 的补全、Hover、跳转和引用查找
会追踪到原声明，并隔离不同项目中的同名符号。

具备可解析分支布局的 `Result<T, E>` 已接入原生布局，包括 `Result<Option<string>, string>` 等非 Copy 组合；Rust 宿主可通过 `Value::as_result()` 统一读取新旧表示。借用读取与 Clone 按组合布局递归处理，叶子类型仍需有对应的原生 Clone 注册。

## Rust 嵌入

执行层新增 `TypedStorageContext::compose_nominal()` 和共享 `NativeInstancePlace`，可把具体用户 struct/enum 按所有权转换成原生实例，沿字段路径 move、恢复和借用；类型定义与泛型参数保存在声明元数据中。`RilsValue::field()`、`struct_name()` 与 `field_name()` 可直接读取这类实例及其引用，字段 handle 保留原 bytes 所有者；数值叶子的 `with_ref` / `into_owned` 按实际布局转换。普通脚本 struct/enum 的独立存储、泛型实例、字段 move/恢复、方法 receiver 与模式绑定已接入原生布局；enum 的 unit/tuple/record 变体保留标签、声明身份和具体类型参数。借用 enum 模式直接投影原字段，拥有型模式在整条分支匹配成功后移动绑定字段；Debug 与哈希键访问复用原生变体元数据。构造过程仍有临时兼容值，Option/Result 等剩余快照适配器继续迁移。用户类型只有显式实现 Copy 才可复制，字段全部为 Copy 并不会自动授予该能力。

`RilsValue::with_native_view` 可在回调内沿原生布局读取 `Option`、`Result`、record 和序列的嵌套子值，不需要把复合值转换成拥有型 `Value`；`with_ref` 仍用于已知 Rust 叶子类型。视图不能离开回调。用户 struct/enum receiver、变体字段投影、原生集合方法及借用迭代器使用原对象的检查路径；剩余适配器仍在逐步迁移。

类型化空 `Vec<T>` 在元素有可解析的标准库原生布局时使用原生序列，包括 `Option<string>`、`Result<string, string>`、元组及嵌套 `Vec<string>`。这些复合元素可用拥有型方法移入、移出，并能建立索引引用和借用迭代器；内部读取借用的非 Copy 复合元素仍受 `Value` 转换边界限制。无法解析布局的用户定义元素暂时沿用旧容器路径。`Vec<string>` 借用读取字符串时，现有 `Value` 调用边界会克隆文本；索引直接移出非 Copy 字符串仍被拒绝。

脚本中的常用拥有型容器包括 `Vec<T>`、`VecDeque<T>`、`BinaryHeap<T>`、`BTreeMap<K, V>`、
`BTreeSet<T>`、
`HashMap<K, V>` 和 `HashSet<T>`。`BinaryHeap` 是最大优先队列，使用
`push/pop/peek_cloned` 处理整数、字符或字符串优先级；`BTreeMap` 按键排序。完整用法见
[集合章节](docs/language/05-data-types-and-collections.md)。
`Vec::new()` 等零参数泛型集合构造需从类型标注或显式泛型参数确定类型，例如 `let values: Vec<i32> = Vec::new();`。
数组、`Vec<T>`、Map 和 Set 还提供 `iter()` 借用遍历，遍历后可继续使用原集合。数组和 `Vec<T>` 的拥有型 `into_iter()` 按 `next()` 的调用逐项移出元素。
`Iterator::next(&mut iterator)` 等显式 trait 路径调用在解释器和字节码 VM 中均可用。
Struct 和 enum 支持 `#[derive(Clone)]`、`#[derive(Copy)]`、`#[derive(Eq, Hash)]`；struct 也支持 `#[derive(Default)]`。`Clone` 逐字段调用对应 trait 实现，用户类型必须显式实现 `Copy`，并同时实现父 trait `Clone`；所有字段（enum 的所有变体）均须为 Copy，仅含 Copy 字段不会自动获得 Copy。
整数实现 `Clone`、`Copy`、`Default`、`Eq`、`Hash`；`f32`、`f64` 实现前三者；`string` 实现 `Clone`、`Default`、`Eq`、`Hash`。
泛型 trait 可声明类型参数；解释器可按函数签名和捕获行为检查标准库的 `FnOnce<Args, Output>`、`FnMut<Args, Output>`、`Fn<Args, Output>` bound。字节码编译器当前会明确拒绝这组三种 bound，直到共享前端完成相同的检查。Option/Result 的 `map`、`and_then`、`or_else` 等导出方法，以及 `core::ops::apply_twice`、`combine`、`chain` 等导出自由函数，已可在解释器和字节码中调用 Rils 函数或闭包；`Option::filter` 还可把共享引用交给谓词。回调错误保留源码位置。

实现 `Iterator` 的 Rils 类型只需声明 `Item` 并实现 `next`，可使用或重写标准库 trait 中定义的默认方法；解释器和字节码均从这些方法体执行，元素类型从关联类型 `Item` 解析。它还会自动实现 `IntoIterator`，`into_iter()` 返回自身，可直接用于 `for`。

内建 `Box<T>` 使用 `Box::new(value)` 构造；需要显式类型参数时可写 `Box::<T>::new(value)`，消费后通过 `into_inner()` 取回值。内部字段是私有的，不能用 `Box { value: ... }` 构造。

```rust
let value: i32 = rils::eval("1 + 2 * 3")?.get_cloned()?;
let module = rils::compile("let value = 40; value + 2")?;
let value: i32 = module.execute()?.get_cloned()?;
```

`eval`、`Engine::eval` 与字节码 `execute` / `call` 返回 `RilsValue` 句柄。
宿主可用 `with_ref::<T, _>(|value| ...)` 临时借用，用 `get_cloned::<T>()` 显式克隆，
或用消耗句柄的 `into_owned::<T>()` 移出拥有型结果。移出失败会把句柄连同错误返回。
目前类型化借用和移出覆盖基础标量与字符串；脚本结构体可用 `field(index)` 获取保持原对象存活的字段引用句柄。
动态组合值的类型化视图仍在迁移中。

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
