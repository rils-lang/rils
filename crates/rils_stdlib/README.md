# rils_stdlib

`rils_stdlib_macros` owns the declaration procedural macros; `rils_builtins_macros` generates the remaining module and prelude catalog. Native layout and element-read registrations are defined beside their Rust types as public `NATIVE_LAYOUT[_NAME]` and `NATIVE_ELEMENT[_NAME]` constants. `native_registry!("src/stdlib")` collects them into `src/native.rs` through the `rils_native` protocol.

## Free functions

Mark public Rust functions inside `#[decl_rils(std::fs)]` with `#[rils_fn]` to export their
signatures and documentation to Rils. Unmarked functions remain Rust helpers. The Rust body is
kept as the implementation; the execution adapter converts Rils values at the boundary. For
example, `src/stdlib/fs.rs` defines the filesystem functions and their native bodies together.
Exported parameter types are preserved, including `[T; N]` with `const N: usize` and
formatting bounds such as `T: std::fmt::Display`. Unsupported type forms are compile errors;
`rils_any` and `rils_ref_any` are removed. `#[rils_variadic]` remains the explicit variadic
contract used by formatting macros. `write` and `write_line` require `Display`; their native
argument conversion invokes Rils trait dispatch before passing rendered text to Rust.

## 显式导出声明

`src/stdlib/ops.rs` 导出泛型 `FnOnce<Args, Output>`、`FnMut<Args, Output>` 和
`Fn<Args, Output>` trait；`Args` 使用 tuple 表示参数列表。解释器按签名和捕获行为
检查回调约束。Rust 侧用于绑定的 marker trait 封闭在私有模块中，外部 crate 不能
为自己的类型实现这些导出 trait。Rust 回调继续使用原生的 `std::ops::Fn*` 约束；
导出自由函数的桥接器按具体函数签名生成适配闭包，不对参数个数预设固定上限。
Option/Result 的回调方法已直接在带 `FnOnce` 约束、返回普通 Rust 值的方法上使用
`#[export_rils]`；宏生成隐藏的可失败实现供运行时桥接调用。导出方法默认生成原生符号，
无需额外的后端标记。Vec 的 receiver 代理把元素槽位临时移入 Rust 包装类型并在调用结束后归还；
生成器从方法签名生成调用，Rust 检查参数与返回值是否满足转换接口。
涉及词法引用的借用迭代，以及结构修改时的引用保护，仍由对应 receiver 适配器处理。
其他复杂 receiver 类型仍需扩展原生桥接。
`#[export_rils]` 只可标在固有 impl 的方法上。Rust trait impl 必须在整个 impl 块上
标记 `#[rils_impl]`，由宏一并导出 trait 身份、方法和关联类型；trait 方法不能单独标记
`#[export_rils]`。集合的 `IntoIterator` 和迭代器的 `Iterator` 使用这种形式。
原生方法按导出的符号路径注册。宿主提供的方法显式写 `#[rils_import(...)]`。
未提供所需转换或适配器时，
桥接生成会在编译期间报错。例如 `Vec::is_empty` 由 `len()` 计算，并通过原生符号调用。
Rils 的 `IntoIterator` 与 Rust 一样声明 `Item` 和 `IntoIter`，标记后的 impl 会导出
这两个关联类型。`Vec::from` 由固有方法导出，其 Rust `From<[T; N]>` 实现保持内部使用。
trait impl 的导出方法签名从 Rust 方法签名和 impl 中的关联类型推导；例如
`std::option::Option<Self::Item>` 会根据 `type Item = T` 导出为 Rils 的 `Option<T>`。
trait 方法不使用 `#[rils_return]` 覆盖返回类型。`Range<T>` 已经实现
Rust `Iterator`，其 `IntoIterator` 来自 blanket impl；Rils 也在标准库声明中登记
`Iterator` 到 `IntoIterator` 的通用关系。
拥有型标准库迭代器以 `VecDeque` 保存剩余元素，并实现 Rust 的 `Iterator` trait。
`#[rils_trait]` 的方法体会导出为 Rils 默认方法，缺少重写的 impl 自动获得该实现。
`Iterator` 的默认方法在 trait 定义处编写，由解释器和字节码从同一份生成源码执行；
`next` 仍是推进迭代器的底层原语。旧数字调用操作码已移除，旧字节码需重新编译。

一个 `#[decl_rils(core::collections)]` 模块可以定义一个或多个导出项。类型和 trait
必须分别标记 `#[rils_struct]`、`#[rils_enum]`、`#[rils_trait]`；未标记的项仅供
Rust 实现使用。公开结构体字段进入 Rils 声明，固有方法仍需 `#[export_rils]`。
即使 trait 与类型都在同一模块，trait 实现也只有在 impl 块上标记
`#[rils_impl]` 后才登记到 Rils。

```rust
#[decl_rils(core::collections)]
mod native {
    #[rils_struct]
    pub struct BinaryHeap<T>(std::collections::BinaryHeap<T>);

    impl<T: Ord> BinaryHeap<T> {
        #[export_rils]
        pub fn new() -> Self { Self(std::collections::BinaryHeap::new()) }
    }
}
```

方法符号前缀及生成的 `.rils` 资源路径由 `decl_rils` 的模块路径和类型的
snake_case 名称组成，例如 `core::collections::binary_heap` 与
`core/collections/binary_heap.rils`。
derive 函数写作 `#[rils_derive(TraitName)]`，明确关联模块内标记导出的 trait。
无论模块中有多少定义，都必须显式标记导出的类型和 trait。数值家族使用
`primitive_integer_family!` / `primitive_float_family!` 预留宏声明内建原始类型。
`std` 模块下导出的宿主类型使用完整的 `std::...::Type` 路径登记，并以所属模块作为宿主能力；
`src/stdlib/io.rs` 的 `Error` 与 `ErrorKind` 因而继续由宿主提供运行时值。

此 crate 存放可信的 Rust 标准库定义源。`src/stdlib/option.rs` 和
`src/stdlib/result.rs` 使用 `#[decl_rils(core::...)]` 标注普通 Rust 模块，以类型、方法签名和 `#[export_rils]`
方法体定义原生实现。定义宏把同一份声明交给：

- `rils_builtins` 的静态元信息生成器；
- `rils_execution` 的共享原生 handler 生成器。解释器和 VM 均通过该共享 handler 执行。

`Option` 和 `Result` 的所有成员方法均在这里定义。方法体是普通 Rust 代码，编辑器可按 Rust
语法分析；`#[export_rils]` 仅标记需要生成绑定的方法，宏展开时会移除此标记。
未标记的方法保留为普通 Rust 辅助方法，不进入 Rils 的元信息或运行时绑定。
内部定义可通过 `stdlib::prelude` 引用其他已定义的 Rust 标准库类型；这个模块只服务于 Rust 实现，不改变 Rils 侧的 prelude。
运行时适配器把 Rils 值转换为该 Rust 类型，然后调用真实的方法；解释器的回调适配器使用同一方法的可失败辅助实现。
`rils_builtins` 直接使用这些 Rust 定义生成
Option、Result、整数、浮点数和 string API 的静态元信息，不再从对应的 `.rils` 文件重新解析元信息。
Analyzer 目前仍加载未迁移的 `.rils` 语言包；已迁移的类型不再生成并提交重复源码。
导出宏可按需从 Rust 定义生成声明文本供解析测试使用，但不保存 `RILS_SOURCE` 常量。

整数类型使用 `primitive_integer_family!(i8, i16, i32, ..., usize)`
声明全部内建类型，并在 `impl<TNum> Number<TNum>` 中编写 `#[export_rils]` 方法。
宏在 Rust 侧生成一个 `Number<T>` 包装类型及各具体整数的实现；Rils 侧只生成
`impl i8`、`impl i32` 等原有类型的方法声明，不新增包装类型。有符号与无符号数的
`abs`、`saturating_neg` 等差异由内部适配 trait 处理。整数方法和常量的 `.rils`
声明从这份 Rust 定义生成；调用按声明中的规范符号路径分派。

`f32` 和 `f64` 复用数值族模板；`string` 使用普通 Rust 包装类型定义方法。
`string` 的拥有型迭代器结果通过原生绑定映射到 Rils 的 `Iterator<T>`。
`Rc<T>` 和 `Weak<T>` 的声明及 Rust 方法体集中在 `src/stdlib/rc.rs`，Rils 公开路径为
`core::rc::Rc` 和 `core::rc::Weak`；当前执行链按导出的原生符号调用适配器。
`Cell<T>` 的声明与 Rust 方法体在 `src/stdlib/cell.rs`，Rils 公开路径为 `core::cell::Cell`。
Rust 包装类型通过 `Deref` / `DerefMut` 访问底层容器或句柄；拥有型转换使用 `From` / `Into`。

原生类型的 trait 映射目前支持 `Clone`、`Copy`、`Default`、`Eq`、`Hash` 和 `BitFlags`。无条件实现标在类型或数值族声明上，
例如 `#[rils_impl(Clone)]`；带约束的实现标在对应的 Rust trait `impl` 上，
例如 `#[rils_impl] impl<T: Clone> Clone for Option<T> { ... }`。
`Copy` 的实现同理使用 `impl<T: Copy> Copy for Option<T> {}`，且必须同时登记 `Clone`。
宏从 impl 的泛型 bound 生成条件元信息，Rust 编译器检查实际 trait 实现；
未标记的辅助 trait impl 仍只在 Rust 内部使用。条件元信息支持泛型参数上的
`Clone`、`Copy`、`Default`、`Eq`、`Hash` bound，可写在参数或 `where` 子句中；
脚本侧条件 impl 的执行仍未开放。

基础 trait 的 Rils 声明集中位于 `src/stdlib/traits.rs`，
各自使用 `#[decl_rils(core::...)] mod` 定义。trait 的限定父 trait 路径绑定对应 Rust trait；
其他父 trait 写入 Rils 的继承关系。模块内未标记的辅助函数保留为普通 Rust 函数。
可选的 `#[rils_derive(TraitName)]` 函数也定义在该模块内，接收类型声明 AST 并返回生成的 impl。
生成器可使用 `rils_syntax::rils_quote! { ... }` 写 Rils 语法，使用
`rils_quote_tokens!` 组装片段，支持 `#name` 插值与 `#(#items),*` 列表展开；
派生展开时自动将解析错误定位到被派生的声明。当前 `Clone` 支持泛型 struct 和 enum，
`Copy`、`Eq`、`Hash` 支持非泛型 struct 和 enum，`Default` 支持 struct；泛型条件 impl 尚待支持。
`BitFlags` 只由宿主 flags enum 自动实现，不提供脚本侧 derive。
`rils_syntax_macros` 在构建时从标准库定义模块收集带 `#[rils_derive(TraitName)]` 的函数，生成静态注册表；
新增 trait 派生不需要再维护一份独立的名称列表。
同一声明生成内建 trait 元信息；已迁移的 trait 不再保留对应的 `core/*.rils` 文件。

导出带回调的自由函数时，只需在返回普通 Rust 值的函数上标记 `#[rils_fn]`，
并用 `F: FnMut(A) -> B` 等约束描述回调。生成器由这一份实现推导 Rils 的
`fn(A) -> B` 参数及返回值，为任意参数位置和多个回调生成 Rils 函数值到 Rust 闭包的桥接；
宏还生成隐藏的可失败实现，在直接调用回调的地方传播运行错误。当前只改写直接回调调用；
`return`、嵌套闭包和表达式宏会被明确拒绝。普通参数和结果可使用类型泛型、标量与 unit；
引用和容器的通用转换仍需扩展。`core::ops` 中的 `apply_twice`、`combine`、`chain`
分别示范状态回调、双参数回调及两个回调。
