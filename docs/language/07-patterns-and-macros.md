# 模式匹配与宏

[← 返回语言手册目录](README.md)

## 模式匹配

`match` 是表达式，返回被选中分支的值：

```rust
fn value_or_zero(value: Option<i32>) -> i32 {
    match value {
        Some(number) => number,
        None => 0,
    }
}
```

当前支持：

- `Some(pattern)` 与 `None`
- `Ok(pattern)` 与 `Err(pattern)`
- `_` 通配模式
- 标识符绑定
- `()`、布尔、整数、浮点数和字符串字面量
- 括号模式
- 嵌套模式，例如 `Some(Some(value))`
- Struct 模式，例如 `Point { x, y }`
- Enum unit、tuple 和 record variant 模式

变量绑定只存在于对应分支：

```rust
match Some(42) {
    Some(number) => number,
    None => 0,
}
```

复杂类型可以直接解构：

```rust
match message {
    Message::Quit => "quit",
    Message::Move(x, y) => if x == y { "diagonal" } else { "move" },
    Message::Write { text } => text,
}
```

Record 模式当前必须列出全部字段；尚未支持 Rust 的 `..` 剩余字段语法。

普通表达式分支之间必须使用逗号。块、`if` 和嵌套 `match` 分支可以省略逗号：

```rust
match value {
    Some(number) => {
        println!(number);
        number
    }
    None => 0,
}
```

分支按照从上到下的顺序选择第一个匹配项。独立静态分析会检查 `bool`、Option、Result 和已知用户
enum 的穷尽性，并报告重复或已被完整覆盖的不可达分支；未知或开放类型仍保守处理。VM 和解释器
都保留运行时非穷尽保护，建议开放类型使用 `_` 兜底。

0.1 暂不支持模式守卫、`|` 或模式和 `@` 绑定。

## 函数式宏

宏在解析 AST 之前按 token tree 展开，不是运行时函数。使用 `macro` 在顶层声明，
通过 `name!(...)` 调用。一个宏可以包含多个匹配分支，按书写顺序选择第一个完整匹配的分支：

```rust
macro choose_larger {
    ($value:lit) => { $value }
    ($left:expr, $right:expr) => {
        if ($left) > ($right) { ($left) } else { ($right) }
    }
}

let answer = choose_larger!(21, 42);
```

捕获必须声明片段类型：

- `$value:expr`：一个能够被完整解析的表达式，包括块表达式和其他宏调用。
- `$value:lit`：整数、浮点数、字符串、布尔值或 `()` 字面量。
- `$name:ident`：单个普通标识符，不接受关键字。

匹配器中的 `$($element:expr),*` 表示以逗号分隔的零个或多个元素，`+` 表示一个或多个。
分隔符可以省略，也可以换成其他单个 token。展开模板使用相同形式：

```rust
macro bindings {
    ($($name:ident = $value:expr),*) => {
        $(let $name = $value;)*
    }
}

bindings!(left = 20, right = 22)
left + right
```

同一次重复中的多个捕获会按位置同步展开，并且必须具有相同长度。当前不支持嵌套重复。
宏展开分支的最外层 `{ ... }` 是模板边界，不会成为结果的一部分；需要产生块表达式时，
应在模板中再写一层 `{ ... }`。

当前宏具有以下规则：

- 宏声明只能位于顶层，但可以在声明之前调用。
- 分支匹配必须消费调用括号内的全部 token；没有分支匹配时产生语法诊断。
- token 替换不会自动添加括号；表达式参数通常应在模板中写成 `($value)` 以保持优先级。
- 展开支持嵌套和递归宏，最大展开深度为 64，超过限制会显示展开链。
- 未知捕获、重复捕获、不一致的重复长度和未知宏都会产生语法诊断。
- 当前是非卫生宏：模板中直接写出的名字可能捕获调用位置的绑定。
- 旧的单分支写法 `macro name($value) { ... }` 仍然兼容，其中参数按未分类 token 匹配。
- 暂不支持嵌套重复、更多片段类型和过程宏。

### Rust 宿主转发宏

`rils_forward_macro!` 可以把一个 Rust 原生函数注册为 Rils 函数式宏。辅助宏会创建一个
不可从普通 Rils 源码命名的内部调用目标，并自动生成接受零个或多个 `expr` 参数的转发宏：

```rust
fn host_log(values: &[rils::Value]) -> Result<rils::Value, String> {
    for value in values {
        eprintln!("{value}");
    }
    Ok(rils::Value::Unit)
}

let mut engine = rils::Engine::new();
rils::rils_forward_macro!(engine, log, 0, usize::MAX, host_log)?;
engine.eval(r#"log!("hello", 42)"#)?;
```

参数依次为 Engine、Rils 宏名、最小参数数量、最大参数数量和原生函数。原生函数必须是
`fn(&[Value]) -> Result<Value, String>`，也可以使用不捕获环境的闭包。Rust 宏本身不能作为
运行时函数指针传入；需要在这个原生函数中调用对应的 Rust `println!` 等宏。辅助宏生成的
Rils matcher 是 `$($argument:expr),*`，参数数量在调用原生函数前检查。
