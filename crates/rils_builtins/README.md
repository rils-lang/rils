# rils_builtins

该 crate 是 Rils 发行版内建 API 的唯一静态描述层，不读取外部文件，也不包含解释器对象、AST 或
宿主回调。CLI、frontend、编译器、Analyzer、C API 和嵌入式 runtime 可以直接共享这些声明。

声明分为两部分：

- `BUILTINS` 描述 module、primitive、struct、enum、trait、function 及其成员；
- `INTEGER_INTRINSICS` 和 `FLOAT_INTRINSICS` 描述按规范符号路径调用的数值方法。

`TypePattern` 是独立于 frontend `Type` 的递归类型表达式，可表示泛型、嵌套名义类型、
Option/Result、tuple、函数和引用。`BuiltinBackend` 明确区分 runtime、intrinsic、host-backed 和纯
metadata 项，因此“编译器认识一个符号”不等同于“runtime 自己实现该符号”。

内建类型和方法的元信息由 `rils_stdlib` 的 Rust 定义生成；`stdlib/` 目前保留模块树与
prelude 函数声明。类型模式使用 `type_pattern!`，原生方法与数值操作以声明中的规范符号路径定位。

构建期宏使用共享 `rils_syntax` lexer/parser 解析保留的 `.rils` 声明；
`decl_rils` 从 Rust 定义生成类型、成员、数值 intrinsic、常量、receiver、泛型和文档元信息。

Rust 定义迁移已开始：`rils_stdlib/src/stdlib/option.rs` 和 `result.rs` 使用 `#[decl_rils]` 同时定义
所有 Option/Result 成员方法的签名和 `#[export_rils]` 标记的普通 Rust 原生实现。宏分别为本 crate 生成
`native_definitions::DECLARATIONS`，并为 `rils_execution` 生成共享运行时 handler。
`BUILTINS` 中的 Option/Result/string 与整数、浮点数 intrinsic 和常量直接采用 Rust 定义生成的元信息。
Analyzer 的语言包仍包含尚未迁移的 `.rils` 声明；已迁移的声明不再写回该目录。
这些类型的静态元信息直接来自 `rils_stdlib`，Analyzer 的完整源码导航待语言包迁移时接入同一元信息。
整数类型由 `rils_stdlib/src/stdlib/integer.rs` 的 `Number<TNum>` 方法模板与整数类型族宏定义；
静态声明、原生实现和 12 种宽度的运行时桥接使用同一组方法签名。
