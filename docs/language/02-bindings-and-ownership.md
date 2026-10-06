# 变量、作用域与所有权

[← 返回语言手册目录](README.md)

已使用原生布局的 `Option<T>` / `Result<T, E>` 在赋值和可变引用写回时按目标布局消费新值，不要求非 Copy 内容实现 Clone。局部变量、字段及元组/数组元素在 move 后重新赋值仍保留其声明见证和注册操作；转换失败不会替换槽位内原有值。

透明类型别名不创建新的类型身份：参数、返回值、局部声明和字段中的别名按定义处展开，包括链式泛型别名。跨模块的同名类型保留各自的完整声明路径；为原生 `Option<T>` 字段或局部变量赋值 `None` 时，布局继续由目标的具体 `T` 决定。

局部绑定没有显式类型标注时，原生存储也使用前端推导结果；例如 `let value = Some(Item { value: 37 });` 在 Item 布局可解析时采用原生 Option。类型化元组和数组在初始化、函数参数及返回时递归应用具体声明；内部的 `Option<用户类型>` / `Result<用户类型, E>` 在布局可解析时采用原生存储。转换按所有权移动内容，不需要用户类型实现 Clone；嵌套元组和数组也适用。

## 变量与作用域

变量默认不可变，并且声明时必须初始化：

```rust
let name = "Rils";
let mut count: i32 = 0;
count = count + 1;
```

内部作用域可以遮蔽外部变量。函数采用词法作用域，嵌套函数可以捕获外部绑定。

### 所有权、移动与局部引用

`()`、`bool`、`i32`、`f64` 以及只包含 Copy 值的 Option、Result、struct 和 enum 是
Copy 值。其他值默认拥有唯一所有者，赋值、传参和返回会移动所有权：

```rust
let text = "hello";
let moved = text;
println!("{}", text); // 只借用 `text`，不会 move
```

需要独立副本时必须显式克隆。`clone` 接受引用，因此不会移动原值：

```rust
let text = "hello";
let copied = clone(&text);
```

`&T` 是局部只读引用，`&mut T` 是局部可写引用。Rils 的 `&mut` 不具有 Rust
的独占含义：同一个存储位置可以同时存在多个可写引用，并且所有引用都能观察到修改。

```rust
fn set(value: &mut i32, next: i32) {
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

原生容器中的用户记录同样支持多层字段与索引引用，例如 `&mut item.inner.count`、
`&mut item.items[0].count`；RefCell 借用、参数以及 `&self` / `&mut self` 方法 receiver
保留原对象与路径，写入会反映到容器原值。通过引用读取拥有型字段仍要求 Copy；借用非 Copy
字段应显式使用 `&` 或 `&mut`。存在后代字段引用时，不能替换其父字段。

Rust 宿主的 `RilsValue::field()` 可直接借用带声明上下文的原生用户记录，并保留原对象与词法路径；字段名和类型查询不会复制非 Copy 负载。`with_ref::<i32>()` 等读取按实际叶子布局处理组合字段与独立包装值，借用字段 handle 不能通过 `into_owned` 移出。执行层已经支持原生实例的字段 move、部分 move 检查和恢复；普通脚本 struct/enum 构造器仍在迁移中。

引用当前受以下限制：

- 不能成为全局绑定，不能被闭包捕获。
- 可以存入 `Option<&T>`、`Result<&T, E>`、`Vec<&T>`、tuple、array、HashMap value
  以及泛型 struct/enum 实例，但这些值仍不能跨越引用本身的词法作用域。
- 来自函数参数的引用可以作为函数返回值；来自函数局部值的引用不能返回。
- 直接声明为 `&T` / `&mut T` 的 struct/enum 字段仍被禁止；泛型字段实例可以携带引用。
- 可以引用局部变量、多层 struct 字段，以及通过引用访问的字段。
- 引用存在期间，所有者不能被移动；字段引用存在期间也不能整体替换所有者。
- `&mut` 只能从 `let mut` 绑定或其字段创建。

引用不会延长目标生命周期，也不需要显式生命周期参数；生命周期由词法作用域和引用来源自动推导。

这些规则也适用于原生组合存储中的引用。`Option<&mut T>`、`Vec<&mut T>` 和泛型字段实例
保存引用句柄及其词法租约；复制引用继续指向同一个来源，不会复制被引用的值。作用域检查沿实际
存活的布局字段和分支追踪来源，因此把局部引用放入 Option、Result 或用户记录后仍不能返回。
