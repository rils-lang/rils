# rils_builtins

该 crate 是 Rils 发行版内建 API 的唯一静态描述层，不读取外部文件，也不包含解释器对象、AST 或
宿主回调。CLI、frontend、编译器、Analyzer、C API 和嵌入式 runtime 可以直接共享这些声明。

声明分为两部分：

- `BUILTINS` 描述 module、primitive、struct、enum、trait、function 及其成员；
- `INTEGER_INTRINSICS` 描述由稳定 `IntrinsicId` 执行的整数方法。

`TypePattern` 是独立于 frontend `Type` 的递归类型表达式，可表示泛型、嵌套名义类型、
Option/Result、tuple、函数和引用。`BuiltinBackend` 明确区分 runtime、intrinsic、host-backed 和纯
metadata 项，因此“编译器认识一个符号”不等同于“runtime 自己实现该符号”。

新增声明应优先使用本 crate 中的 `builtin!`、`member!` 和 `intrinsic!` 宏，执行逻辑留在对应的
runtime 或宿主层。稳定 intrinsic ID 不能复用。
