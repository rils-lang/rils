# rils_stdlib

此 crate 存放可信的 Rust 标准库定义源。`src/stdlib/option.rs` 中的 `decl_rils!`
定义模块路径、类型、方法签名和 `#rils` 实现。定义宏把同一组 tokens 交给：

- `rils_builtins` 的静态元信息生成器；
- `rils_execution` 的共享原生 handler 生成器。解释器和 VM 均通过该共享 handler 执行。

当前是 `Option::is_some` 的纵向样例。`#rils` 块使用受限 Rust 表达式，`#self`
和 `#Option` 由宏替换为生成的值适配对象。现阶段 handler 仅支持 Option 的
`&self -> bool` 方法；其他接收方式、返回类型和 enum 类型须扩展适配器后才能使用。
`builtin_ids.toml` 仍是稳定 ID 的来源。旧 `.rils` 声明暂用于语言包与现有公开目录，
迁移时由一致性测试检查重叠声明。
