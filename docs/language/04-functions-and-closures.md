# 函数与闭包

[← 返回语言手册目录](README.md)

## 函数与闭包

struct、enum、trait、impl、类型别名、模块和 use 是模块级声明；函数是唯一允许出现在函数体或
普通块中的声明，作为词法闭包使用。

函数体最后一个无分号表达式是隐式返回值，也可以使用 `return`：

```rust
fn absolute(value: int) -> int {
    if value < 0 {
        return -value;
    }
    value
}
```

嵌套函数捕获声明位置的环境：

```rust
fn make_counter() {
    let mut count: int = 0;

    fn next() -> int {
        count = count + 1;
        count
    }

    next
}
```

函数类型使用 `fn(参数类型...) -> 返回类型`，箭头右结合：

```rust
fn make_value() -> fn() -> int {
    fn value() -> int {
        42
    }
    value
}

let getter: fn() -> int = make_value();
let result: int = getter();
```

因此 `make_value` 的完整类型是 `fn() -> fn() -> int`。调用一次得到
`fn() -> int`，再次调用得到 `int`。没有显式返回标注时，分析器会从尾表达式和
`return` 推导签名；函数作为变量、参数或返回值时仍保留该信息。

旧的 `function` 类型继续表示“可以调用，但参数和返回类型未知”的函数值，主要用于
原生函数和兼容已有代码。新代码应优先使用精确的 `fn(...) -> ...`。
