# Struct、Enum 与集合

[← 返回语言手册目录](README.md)

## Struct

Struct 使用命名字段，并且至少包含一个字段：

```rust
struct Point {
    x: f64,
    y: f64,
}

let mut point: Point = Point {
    x: 3.0,
    y: 4.0,
};

println!(point.x);
point.x = 10.0;
```

构造时必须提供所有字段，不能提供未知字段，并且字段值必须符合声明类型。Struct 是名义类型：字段相同但名字不同的两个 struct 不是同一种类型。

字段是 place，可以直接赋值或局部借用。可写性来自最外层变量或引用：

```rust
let field = &mut point.x;
*field = 42.0;
```

多层字段同样适用，例如 `outer.inner.value = 42`。字段类型在赋值时检查；字段或其内部值
存在活动引用时，不能直接替换该字段。数组和 Vec 元素也使用同一套 place 规则。

### Tuple、数组与 Vec

Tuple 使用 Rust 风格的语法和数字字段；单元素 tuple 必须保留尾逗号：

```rust
let mut pair: (i32, string) = (42, "answer");
pair.0 = 43;
let text = pair.1;
```

固定数组类型写作 `[T; N]`。数组字面量既支持元素列表，也支持要求元素为 `Copy` 的重复形式：

```rust
let mut values: [i32; 3] = [10, 20, 30];
values[1] = 21;
let item = &mut values[2];
*item = 31;

let zeroes = [0; 8];
```

数组元素必须同型，索引必须是 `usize`。无后缀整数字面量及由它初始化的绑定可从索引用法推导为 `usize`。索引表达式只复制 `Copy` 元素；非 Copy 元素不能
通过索引移出，但可以通过 `&values[index]` 或 `&mut values[index]` 局部借用。

`Vec<T>` 当前提供最小核心 API：

```rust
let mut values: Vec<i32> = Vec::new();
values.push(20);
values.push(22);
let length = values.len();
let last = values.pop();

let copied = Vec::from([1, 2, 3]);
```

`pop()` 返回 `Option<T>`。数组和 Vec 实现拥有型 `IntoIterator`，所以 `for value in values`
会消费容器。共享引用和可写引用的迭代器尚未实现，相关类型空间已保留。

## Enum

Enum 支持 unit、tuple 和 record 三类 variant：

```rust
enum Message {
    Quit,
    Move(i32, i32),
    Write { text: string },
}

let quit = Message::Quit;
let movement = Message::Move(10, 20);
let text = Message::Write { text: "hello" };
```

空 variant 应写成 unit variant；空 record variant 暂不支持。Tuple 和 record variant 的内容会按照声明类型检查。
