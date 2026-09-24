# rils_stdlib

## 显式导出声明

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

方法 ID 前缀及生成的 `.rils` 资源路径由 `decl_rils` 的模块路径和类型的
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
`builtin_ids.toml` 仍是稳定 ID 的来源。`rils_builtins` 直接使用这些 Rust 定义生成
Option、Result、整数、浮点数和 string API 的静态元信息，不再从对应的 `.rils` 文件重新解析元信息。
Analyzer 目前仍加载未迁移的 `.rils` 语言包；已迁移的类型不再生成并提交重复源码。
导出宏可按需从 Rust 定义生成声明文本供解析测试使用，但不保存 `RILS_SOURCE` 常量。

整数类型使用 `primitive_integer_family!(i8, i16, i32, ..., usize)`
声明全部内建类型，并在 `impl<TNum> Number<TNum>` 中编写 `#[export_rils]` 方法。
宏在 Rust 侧生成一个 `Number<T>` 包装类型及各具体整数的实现；Rils 侧只生成
`impl i8`、`impl i32` 等原有类型的方法声明，不新增包装类型。有符号与无符号数的
`abs`、`saturating_neg` 等差异由内部适配 trait 处理。整数方法和常量的 `.rils`
声明从这份 Rust 定义生成；稳定 ID 继续取自 `builtin_ids.toml`。

`f32` 和 `f64` 复用数值族模板；`string` 使用普通 Rust 包装类型定义方法。
`string` 的拥有型迭代器结果通过原生绑定映射到 Rils 的 `Iterator<T>`。
`Rc<T>` 和 `Weak<T>` 的声明及 Rust 方法体集中在 `src/stdlib/rc.rs`，Rils 公开路径为
`core::rc::Rc` 和 `core::rc::Weak`；当前执行链仍使用对应的稳定 ID 适配器。
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
