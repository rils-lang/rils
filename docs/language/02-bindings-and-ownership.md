# 变量、作用域与所有权

[← 返回语言手册目录](README.md)

## 变量与作用域

变量默认不可变，并且声明时必须初始化：

```rust
let name = "Rils";
let mut count: int = 0;
count = count + 1;
```

内部作用域可以遮蔽外部变量。函数采用词法作用域，嵌套函数可以捕获外部绑定。

### 所有权、移动与局部引用

`()`、`bool`、`int`、`float` 以及只包含 Copy 值的 Option、Result、struct 和 enum 是
Copy 值。其他值默认拥有唯一所有者，赋值、传参和返回会移动所有权：

```rust
let text = "hello";
let moved = text;
println!(text); // error: use of moved value `text`
```

需要独立副本时必须显式克隆。`clone` 接受引用，因此不会移动原值：

```rust
let text = "hello";
let copied = clone(&text);
```

`&T` 是局部只读引用，`&mut T` 是局部可写引用。Rils 的 `&mut` 不具有 Rust
的独占含义：同一个存储位置可以同时存在多个可写引用，并且所有引用都能观察到修改。

```rust
fn set(value: &mut int, next: int) {
    *value = next;
}

let mut answer = 0;
{
    let first = &mut answer;
    let second = &mut answer;
    set(first, 20);
    set(second, *second + 22);
}
```

引用当前受以下限制：

- 只能保存在局部变量和函数参数中，不能成为全局绑定。
- 不能存入 struct、enum、Option 或其他拥有型值。
- 不能作为函数返回值、块结果或 match 分支结果逃逸。
- 不能被闭包捕获。
- 可以引用局部变量、多层 struct 字段，以及通过引用访问的字段。
- 引用存在期间，所有者不能被移动；字段引用存在期间也不能整体替换所有者。
- `&mut` 只能从 `let mut` 绑定或其字段创建。

引用不会延长目标生命周期，当前没有生命周期参数、引用字段、容器元素引用或裸指针。
