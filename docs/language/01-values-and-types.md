# 值与类型

[← 返回语言手册目录](README.md)

## 值和类型

| 类型 | 示例 |
| --- | --- |
| `()` | `()` |
| `bool` | `true`、`false` |
| `i8`～`i128`、`isize` | `42i8`、`-7i64` |
| `u8`～`u128`、`usize` | `42u8`、`1usize` |
| `f32`、`f64` | `3.14f32`、`3.14` |
| `char` | `'a'`、`'你'`、`'\n'` |
| `string` | `"hello\n"` |
| `&T` | 局部只读引用 |
| `&mut T` | 局部可写引用，可同时存在多个 |
| `Option<T>` | `Some(42)`、`None` |
| `fn(A, B) -> R` | 保留参数与返回值签名的函数 |
| `function` | 签名未知的兼容函数类型 |
| 名义类型 | 用户声明的 `struct` 和 `enum` |

Rils 没有 `nil` 或隐式空引用。`()` 表示计算完成但没有产生有意义的值；值缺失必须显式使用 `Option<T>`。

## 数值字面量与推导

整数类型完整支持 `i8`、`i16`、`i32`、`i64`、`i128`、`isize`、`u8`、`u16`、`u32`、`u64`、`u128` 和 `usize`；浮点类型为 `f32` 与 `f64`。无后缀整数默认 `i32`，无后缀浮点数默认 `f64`。不同具体数值类型之间不会隐式转换。

无后缀字面量会先保留为待推导类型，并接受变量标注、函数参数、运算另一侧和索引用法等后续约束。例如索引要求 `usize`，因此下面的 `index` 会推导为 `usize`：

```rust
let values = [10, 20, 30];
let index = 1;
values[index]
```

旧的 `int` 与 `float` 类型名已经移除；迁移时应根据实际语义明确选择具体类型。

`nil` 是保留的迁移错误，不能作为值使用：

```rust
let value = nil;
// error: `nil` has been removed; use `None` with an `Option<T>` type
```

## 单元类型

空代码块、没有返回值的函数、带分号的表达式和 `while` 循环产生 `()`：

```rust
fn log(message: string) -> () {
    println!(message);
}

let result: () = log("hello");
```

`()` 不表示缺失值，也不能作为条件使用。

## Option

`Option<T>` 是内置的参数化类型：

```rust
let present: Option<i32> = Some(42);
let missing: Option<i32> = None;
```

`None` 本身没有足够信息推断 `T`，因此变量初始化为 `None` 时必须标注类型：

```rust
let missing = None;
// error: cannot infer the element type; declare it as Option<T>
```

普通变量不能在之后被赋值为 `Option`：

```rust
let mut count = 1;
count = None;
// error: declare the variable as Option<T>
```

`Some(value)` 可以推断元素类型：

```rust
let inferred = Some(42); // Option<i32>
```

当前内置操作如下：

| 函数 | 结果 |
| --- | --- |
| `Some(value)` | 构造包含值的 Option |
| `None` | 不包含值的 Option |
| `is_some(option)` | 是否包含值 |
| `is_none(option)` | 是否为空 |
| `unwrap(option)` | 取出值，None 时产生错误 |
| `unwrap_or(option, fallback)` | 取出值或返回同类型默认值 |

Option 不参与隐式真值转换。推荐使用模式匹配：

```rust
match user {
    Some(value) => println!(value),
    None => println!("not found"),
}
```

直接写 `if user` 会产生运行时错误。`is_some`、`is_none`、`unwrap` 和 `unwrap_or` 仍可用于简单表达式。

### Result 与错误传播

`Result<T, E>` 是内置的成功/失败类型，使用 `Ok(value)` 和 `Err(error)` 构造：

```rust
fn load(success: bool) -> Result<i32, string> {
    if success { Ok(42) } else { Err("load failed") }
}
```

可以用 `Ok(pattern)` / `Err(pattern)` 解构，也可以使用函数或方法形式的
`is_ok`、`is_err`、`unwrap`、`unwrap_or`。`is_ok()` 与 `is_err()` 共享借用变量，
不会移动 Result；`unwrap()` 与 `unwrap_or()` 消费接收者。

`?` 只能在函数内使用。操作数为 `Ok(value)` 时表达式结果是内部值；操作数为 `Err(error)`
时立即从当前函数返回错误分支，后续表达式不会执行。传播只保留错误类型，成功类型由当前函数的
返回标注决定，因此 `Result<string, E>` 可以在返回 `Result<(), E>` 的函数中使用 `?`：

```rust
fn add_two(success: bool) -> Result<i32, string> {
    let value = load(success)?;
    Ok(value + 2)
}
```

当前没有 `FromResidual` 或隐式错误转换，所以传播值必须符合当前函数声明的
`Result<_, E>` 返回类型。顶层使用 `?`、对非 Result 使用 `?`，或传播不兼容的错误类型都会报错。

## 类型标注

变量、参数和返回值支持可选类型标注：

```rust
let mut count: i32 = 0;

fn find_positive(value: i32) -> Option<i32> {
    if value > 0 {
        Some(value)
    } else {
        None
    }
}
```

标注会在以下位置执行运行时校验：

- 变量初始化
- 可变变量赋值
- 函数参数传递
- 函数返回
- `Option<T>` 和 `Result<T, E>` 内部值

没有标注的绑定会从初始化值和后续受约束用法推断类型，但 Option 与普通值之间不会隐式转换。`Some(value)` 创建的变量会保留推断出的 `Option<T>` 类型。
