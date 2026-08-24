# 表达式与控制流

[← 返回语言手册目录](README.md)

## 表达式与分号

代码块最后一个没有分号的表达式是该块的值：

```rust
let value: i32 = {
    let left = 20;
    let right = 22;
    left + right
};
```

分号丢弃表达式的值，并让该语句产生 `()`。

运算符从高到低为：

1. 函数调用 `f(x)`、成员访问 `value.field` 和索引 `value[index]`
2. 后缀错误传播 `?`
3. 一元运算 `!`、`-`、`&`、`&mut`、`*`
4. `*`、`/`、`%`
5. `+`、`-`
6. `<`、`<=`、`>`、`>=`
7. `==`、`!=`
8. `&&`
9. `||`
10. 赋值 `=`

不同具体数值类型之间不会隐式转换；整数与浮点数混合计算前必须使用明确的转换入口。字符串使用 `+`
连接。整数运算检查溢出，除数为零产生运行时错误。

## 控制流

`if` 是表达式：

```rust
let label = if score >= 60 {
    "pass"
} else {
    "fail"
};
```

没有 `else` 且条件为假时结果为 `()`，而不是 `None`。

```rust
let mut index = 0;
while index < 3 {
    println!("{}", index);
    index = index + 1;
}
```

`for` 通过内置的 `Iterator` / `IntoIterator` trait 工作。可迭代值会先调用
`into_iter(self)`（如果实现了 `IntoIterator`），随后循环调用迭代器的
`next(&mut self)`，直到得到 `None`：

```rust
let mut total = 0;
for value in Range { current: 1, end: 4 } {
    total = total + value;
}
```

循环变量在每次迭代的新局部作用域中创建。

两个相同具体整数类型的值之间可以使用半开区间语法 `start..end`。结果是内置 `Range<T>`，实现了
`Iterator<Item = T>`，包含起点、不包含终点：

```rust
let mut total = 0;
for value in 0..5 {
    total = total + value;
}
// total == 10
```

区间两端必须是同一种整数类型；起点大于或等于终点时产生空区间。

`if` 与 `while` 的条件必须为 `bool`。数字、字符串、函数、`()`、`Option` 等值不会
进行隐式真值转换，以避免把“没有结果”或“可选值”静默解释为条件。

`loop` 创建无限循环，使用 `break` 退出、`continue` 开始下一次迭代。`break value` 会成为
循环语句的结果，因此可以通过块表达式接收：

```rust
let answer = {
    loop {
        break 42;
    }
};
```

`while` 和 `for` 同样支持 `break value` 与 `continue`。循环控制采用词法作用域，不能从循环中
声明的嵌套函数跨越函数边界。
