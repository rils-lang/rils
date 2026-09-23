# rils_stdlib

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
Analyzer 目前仍加载 `.rils` 语言包以提供源码位置；运行生成脚本时，导出宏按需从
Rust 定义生成这些语言包声明，不在 `rils_stdlib` 中保存 `RILS_SOURCE` 常量。
提交前用 `python tools/generate-stdlib-sources.py --check` 校验语言包资源同步。

整数类型使用 `primitive_integer_family!(i8, i16, i32, ..., usize)`
声明全部内建类型，并在 `impl<TNum> Number<TNum>` 中编写 `#[export_rils]` 方法。
宏在 Rust 侧生成一个 `Number<T>` 包装类型及各具体整数的实现；Rils 侧只生成
`impl i8`、`impl i32` 等原有类型的方法声明，不新增包装类型。有符号与无符号数的
`abs`、`saturating_neg` 等差异由内部适配 trait 处理。整数方法和常量的 `.rils`
声明从这份 Rust 定义生成；稳定 ID 继续取自 `builtin_ids.toml`。

`f32` 和 `f64` 复用数值族模板；`string` 使用普通 Rust 包装类型定义方法。
`string` 的拥有型迭代器结果通过原生绑定映射到 Rils 的 `Iterator<T>`。
