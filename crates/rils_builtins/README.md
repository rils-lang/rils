# rils_builtins

该 crate 是 Rils 发行版内建 API 的唯一静态描述层，不读取外部文件，也不包含解释器对象、AST 或
宿主回调。CLI、frontend、编译器、Analyzer、C API 和嵌入式 runtime 可以直接共享这些声明。

声明分为两部分：

- `BUILTINS` 描述 module、primitive、struct、enum、trait、function 及其成员；
- `INTEGER_INTRINSICS` 和 `FLOAT_INTRINSICS` 描述由稳定 `BuiltinId` 执行的数值方法；
- `builtin_ids.toml` 为 runtime 成员和 intrinsic 分配同一套稳定 ID。

`TypePattern` 是独立于 frontend `Type` 的递归类型表达式，可表示泛型、嵌套名义类型、
Option/Result、tuple、函数和引用。`BuiltinBackend` 明确区分 runtime、intrinsic、host-backed 和纯
metadata 项，因此“编译器认识一个符号”不等同于“runtime 自己实现该符号”。

现有公开目录中的内建 API 由 `stdlib/**/*.rils` 源码声明，类型模式使用 `type_pattern!`，ID 使用
`builtin_id!("core::...")` 在编译期解析。执行逻辑留在对应的 runtime、intrinsic 或宿主层；稳定
`BuiltinId` 不能复用。

构建期宏使用共享 `rils_syntax` lexer/parser 解析 `stdlib/**/*.rils`，
并把 enum variant、struct/primitive 成员、完整数值 primitive 矩阵、intrinsic、常量、receiver、泛型和文档转换为声明表；
TOML 只保留稳定 ID，运行时仍按 ID 绑定实现。

Rust 定义迁移已开始：`rils_stdlib/src/stdlib/option.rs` 使用 `decl_rils!` 同时定义
`Option::is_some` 的签名和 `#rils` 原生实现。宏分别为本 crate 生成
`native_definitions::DECLARATIONS`，并为 `rils_execution` 生成共享运行时 handler。
迁移期间，`BUILTINS` 及语言包仍从旧 `.rils` 声明生成；一致性测试核对两个目录的重叠成员。
