# 函数与闭包

[← 返回语言手册目录](README.md)

## 函数与闭包

struct、enum、trait、impl、类型别名、模块和 use 是模块级声明；函数是唯一允许出现在函数体或
普通块中的声明，作为词法闭包使用。

函数体最后一个无分号表达式是隐式返回值，也可以使用 `return`：

```rust
fn absolute(value: i32) -> i32 {
    if value < 0 {
        return -value;
    }
    value
}
```

嵌套函数捕获声明位置的环境：

```rust
fn make_counter() {
    let mut count: i32 = 0;

    fn next() -> i32 {
        count = count + 1;
        count
    }

    next
}
```

函数类型使用 `fn(参数类型...) -> 返回类型`，箭头右结合：

```rust
fn make_value() -> fn() -> i32 {
    fn value() -> i32 {
        42
    }
    value
}

let getter: fn() -> i32 = make_value();
let result: i32 = getter();
```

因此 `make_value` 的完整类型是 `fn() -> fn() -> i32`。调用一次得到
`fn() -> i32`，再次调用得到 `i32`。没有显式返回标注时，分析器会从尾表达式和
`return` 推导签名；函数作为变量、参数或返回值时仍保留该信息。

旧的 `function` 类型继续表示“可以调用，但参数和返回类型未知”的函数值，主要用于
原生函数和兼容已有代码。新代码应优先使用精确的 `fn(...) -> ...`。

需要给回调加泛型约束时，可用 `FnOnce<Args, Output>`、`FnMut<Args, Output>` 或
`Fn<Args, Output>`。三者分别表示允许消费捕获值、允许修改捕获值、只读调用；
`Args` 是参数 tuple，零参数写为 `()`，单参数写为 `(T,)`。详情见
[Impl、泛型与 Trait](06-impl-generics-and-traits.md)。

标准库 Option/Result 的 `map`、`map_err`、`and_then` 和 `or_else` 可接收具有匹配
`fn(...) -> ...` 签名的 Rils 函数或闭包。`Option::filter` 接收 `fn(&T) -> bool`，
只在有值时以共享引用调用谓词。解释器和字节码 VM 都会在当前调用上下文中
执行回调，因此捕获值、运行错误的位置以及执行预算会沿用调用方的语义。

导出的自由函数也能通过标准库声明接收回调。`core::ops::apply_twice(value, callback)`
按顺序调用两次 `fn(T) -> T`，可用于带可变捕获状态的闭包；
`core::ops::combine(left, right, callback)` 接收两个参数的回调；
`core::ops::chain(first, value, second)` 依次执行两个回调。回调可以处在普通参数之前
或之后，参数类型与结果类型从调用处推断。解释器、字节码 VM 和加载后的字节码
使用同一份导出声明与原生桥接。
