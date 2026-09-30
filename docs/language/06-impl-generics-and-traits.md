# Impl、泛型与 Trait

[← 返回语言手册目录](README.md)

## Impl

Struct 和 enum 都可以拥有 `impl`：

```rust
impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self { x: x, y: y }
    }

    fn origin() -> Self {
        Self::new(0.0, 0.0)
    }

    fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }
}

let point = Point::new(3.0, 4.0);
println!("{}", point.length_squared());
```

规则如下：

- 没有 `self` 参数的函数通过 `Type::function(...)` 调用。
- 实例方法的第一个参数必须是 `self`。
- 未标注的 `self` 自动视为当前名义类型。
- `Self` 在 impl 的类型标注、构造表达式和关联路径中表示当前具体类型，例如 `Self { ... }`
  与 `Self::new(...)`。
- receiver 支持 Rust 风格的 `self`、`mut self`、`&self` 和 `&mut self`。
- `self` 和 `mut self` 接收所有权；后者允许在方法内重新赋值 receiver。
- `&self` 和 `&mut self` 由方法调用自动借用，`&mut self` 要求实例绑定可变。
- 方法不能和 struct 字段或 enum variant 同名。
- 可以存在多个 `impl Type { ... }`，但不能重复定义方法。

暂不支持关联常量和可见性修饰符。

## 泛型

函数、struct、enum、impl 和 impl 内的方法可以声明类型参数：

```rust
fn identity<T>(value: T) -> T {
    value
}

struct Pair<T, U> {
    first: T,
    second: U,
}

enum Outcome<T, E> {
    Ok(T),
    Err(E),
}

impl<T, U> Pair<T, U> {
    fn swap(self) -> Pair<U, T> {
        Pair {
            first: self.second,
            second: self.first,
        }
    }

    fn replace_first<V>(self, value: V) -> Pair<V, U> {
        Pair {
            first: value,
            second: self.second,
        }
    }
}
```

泛型参数通过函数实参、构造字段和 `self` 的实际类型推断。同一个类型变量多次出现时必须推断为兼容类型：

```rust
fn choose<T>(left: T, right: T) -> T {
    left
}

choose(1, 2);       // T = i32
choose(1, "wrong"); // 类型错误
```

无法从构造器立即推断的参数可以由外层标注补全：

```rust
struct Holder<T> {
    value: Option<T>,
}

let holder: Holder<i32> = Holder {
    value: None,
};
```

泛型类型采用运行时单态参数信息，但当前不会生成专用机器码。关联函数支持 `Box::<i32>::new(value)` 形式的显式类型参数，也可在 `Box::new(value)` 中由实参推导。默认类型参数、显式生命周期参数和 `where` 暂不支持；引用生命周期由词法作用域自动推导。

### Recursive structures and heap indirection

Recursive fields must pass through a fixed-size heap handle or container. The compiler treats `Box<T>`, `Vec<T>`, `HashMap<K, V>`, `HashSet<T>`, iterator handles, and `string` as heap-backed indirection. For example:

The built-in `Box<T>` has private storage. Use `Box::new(value)` or `Box::<T>::new(value)` to construct it, and `box.into_inner()` to consume it and recover the value. `Box { value: ... }` and `.value` cannot access its private storage.

```rust
struct Node {
    value: i32,
    next: Option<Box<Node>>,
    children: Vec<Node>,
}
```

The rule is based on the field layout, rather than requiring one specific type. Ordinary structs, tuples, fixed-size arrays, and `Option<Node>` remain inline and direct recursion reports an infinite-size error. Elements stored in heap containers still follow Rils explicit move rules.

### 类型别名

`type` 声明透明类型别名，可以带泛型参数，也可以引用另一个别名：

```rust
struct Box<T> {
    value: T,
}

type ValueBox<T> = Box<T>;
type IntBox = ValueBox<i32>;

let value: IntBox = Box { value: 42 };
```

别名不会创建新的名义类型，使用时会递归展开，并严格检查泛型实参数量。同一代码块内的
类型别名会先于其他声明注册，因此可以在声明位置之前用于函数、字段或变量类型。

## Trait

Trait 声明一组必须实现的方法签名：

```rust
trait Describe {
    fn describe(self) -> string;
}

trait Duplicate {
    fn duplicate(self) -> Self;
}
```

Trait 方法可以用分号声明为必需方法，也可以提供默认方法体。`Self` 表示正在实现该 trait 的具体类型：

```rils
trait Score {
    fn score(self) -> i32;
    fn doubled(self) -> i32 { self.score() * 2 }
}
```

实现 `Score` 时只需定义 `score`；如需不同的行为，可以在 impl 中重写 `doubled`。

以下 trait 由运行时预先声明，用户不能同名重定义：

```rust
trait Copy: Clone {}

trait Clone {
    fn clone(&self) -> Self;
}

trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
    // count、collect_vec、take 等方法由标准库 trait 定义提供默认方法体。
}

trait IntoIterator {
    type Item;
    type IntoIter;
    fn into_iter(self) -> Self::IntoIter;
}
```

Trait 可以声明一个或多个 supertrait。实现该 trait 的类型必须同时实现所有 supertrait：

```rust
trait Behaviour: Default + Clone {
}
```

Trait 可以声明必需关联类型，也可以使用 `=` 提供默认类型。关联类型及默认值均可带泛型参数：

```rust
trait Factory {
    type Item<T> = Box<T>;
    fn make(self) -> Self::Item<i32>;
}

impl Iterator for Counter {
    type Item = i32;

    fn next(&mut self) -> Option<i32> {
        // ...
    }
}
```

没有默认值的关联类型必须在 trait impl 中定义。关联类型会参与方法参数和返回类型校验。
泛型代码可写 `T::Item`；如果某个类型从多个 trait 得到同名关联类型，目前会报告歧义。
可以使用完全限定投影消除歧义：

```rust
let left: <Both as Left>::Item = 1;
let right: <Both as Right>::Item = "right";
```

基础标量、函数、引用以及仅包含 Copy 字段的 Option/struct/enum 自动满足 `Copy`。
拥有型值自动满足 `Clone` bound；命名类型若要使用 `.clone()` 方法，需显式实现 `Clone`，
也可以继续使用通用的 `clone(&value)` 函数。对含非 Copy 字段的类型声明 `impl Copy` 会报错。

Struct 和 enum 可通过 `#[derive(Clone)]` 生成逐字段调用 `Clone` 的实现；enum 支持 unit、tuple 和 record 变体。
`#[derive(Copy)]` 生成标记实现，并继续检查所有字段是否为 Copy。两种派生可以组合使用：

```rils
#[derive(Clone, Copy)]
struct Point { x: i32, y: i32 }
```

这两种派生都支持 struct 和 enum 的 unit、tuple 与 record 变体；泛型 struct 和 enum 可派生 `Clone`，泛型 `Copy` 派生需等待条件 trait impl 支持。

非泛型 struct 和 enum 还可使用 `#[derive(Eq, Hash)]` 作为 `HashMap` 的键或 `HashSet` 的元素。
字段目前支持 `()`、`bool`、整数、`char`、`string` 及由它们组成的 tuple、数组、`Option`、`Result`；
浮点数、引用和其他命名类型不能派生。`Hash` 和 `Eq` 必须同时存在才能用作哈希键。
`BitFlags` 只由宿主 manifest 中标记为 flags 的 enum 自动实现；脚本 enum 当前没有位值语义，不能派生或手写实现该 trait。
整数类型实现 `Clone`、`Copy`、`Default`、`Eq`、`Hash`；浮点类型实现 `Clone`、`Copy`、`Default`，
但不实现 `Eq`、`Hash`；`string` 实现 `Clone`、`Default`、`Eq`、`Hash`，不实现 `Copy`。

使用 Rust 风格的 `impl Trait for Type`：

```rust
struct Point {
    x: i32,
    y: i32,
}

impl Describe for Point {
    fn describe(self) -> string {
        "point"
    }
}
```

Trait impl 会验证：

- 所有必需方法均已实现
- 没有声明 trait 之外的额外方法
- 参数数量和参数类型一致
- `self` 的位置一致
- 返回类型一致，包括 `Self`
- 同一个 trait 不会对同一类型重复实现
- 遵守孤儿规则：trait 或目标类型至少一个必须声明在当前项目中

项目内不同模块共享同一套 coherence 检查。通过 `use`、alias 或完整模块路径指向同一 trait 和
目标类型时，只允许一个 impl；不同模块中仅短名称相同的声明保持不同身份。内建类型、内建 trait
和 Host 类型属于外部身份，因此不能为两个均非本地的身份新增 impl。未来外部库声明也遵循同一规则。

## Default 与派生

`Default` 是 prelude 中的内建 trait，关联函数签名为 `fn default() -> Self`。基础标量的默认值分别是数值零、`false`、`'\0'`、空字符串和 `()`；tuple 与数组逐元素取默认值，`Option<T>` 默认为 `None`，`Vec<T>`、`HashMap<K, V>` 和 `HashSet<T>` 默认为空集合。引用、函数、`Result<T, E>` 和宿主对象没有隐式默认值。

Struct 可以使用 Rust 风格派生：

```rust
#[derive(Default)]
struct Settings {
    enabled: bool,
    retries: i32,
}

let settings = <Settings as Default>::default();
```

派生生成器由 Rust 标准库的 `Default` 定义模块注册，在前端生成普通的 `impl Default`，因此解释器、字节码编译器和 Analyzer 使用同一模型。每个字段类型都必须实现 `Default`，否则诊断会指向对应字段。同一类型不能同时派生并显式实现 `Default`。内部派生模型会为泛型字段记录所需的 `Default` bound；泛型条件 impl 的执行仍受本章末尾所述的当前限制。

Trait 方法保留其 trait 身份。同一类型可以实现多个带同名方法的 trait；普通方法调用只有在
候选唯一时才会自动选择，否则必须使用 UFCS：

```rust
Left::value(&both);
<Both as Right>::value(&both);
```

`Type::method(receiver, ...)` 可调用固有方法或唯一的 trait 方法；多个 trait 同名时必须指定 trait。
固有方法始终优先于同名 trait 方法。类型路径和 UFCS 不执行接收器自动借用，因此 `&self` 和
`&mut self` 方法需要显式传入引用。
`Iterator::next(&mut iterator)`、`Clone::clone(&value)` 等 trait 路径调用可在解释器和字节码 VM 中使用；
标准库原生方法按 trait 导出符号分派，脚本类型则调用其对应的 trait impl。

泛型参数支持一个或多个 trait bound：

```rust
fn describe<T: Describe>(value: T) -> string {
    value.describe()
}

fn combine<T: Left + Right>(value: T) -> i32 {
    value.left() + value.right()
}
```

Struct、enum、函数、固有 impl 和方法的泛型参数都可以带 bound。无条件的泛型 trait impl 也受支持：

```rust
impl<T> Describe for Wrapper<T> {
    fn describe(self) -> string {
        "wrapper"
    }
}
```

标准库提供一项通用实现：任何实现 `Iterator` 的类型同时实现 `IntoIterator`，
`Item` 为其 `Iterator::Item`，`IntoIter` 为该类型自身，`into_iter(self)` 直接返回自身。因此自定义迭代器只需实现
`Iterator`，即可用于 `for`、调用 `.into_iter()` 或通过
`<MyIterator as IntoIterator>::into_iter(value)` 调用。再为同一类型显式实现
`IntoIterator` 会与这项通用实现冲突。当前用户代码仍不能声明新的带条件或 blanket trait impl。
显式实现 `IntoIterator` 时需声明 `Item` 和 `IntoIter`；`IntoIter` 必须实现 `Iterator`，
且 `Item` 必须与 `IntoIter` 的 `Iterator::Item` 一致。

Trait 本身也可以声明类型参数，impl 必须给出相同数量的类型实参，方法签名按这些实参检查：

```rils
trait Transform<T> {
    fn apply(self, value: T) -> T;
}

struct Echo { value: i32 }
impl Transform<i32> for Echo {
    fn apply(self, value: i32) -> i32 { value }
}
```

标准库从 `core::ops` 导出 `FnOnce<Args, Output>`、`FnMut<Args, Output>`、
`Fn<Args, Output>`，其中 `Args` 是参数 tuple。`Fn` 可用于需要 `FnMut` 或
`FnOnce` 的位置，`FnMut` 可用于需要 `FnOnce` 的位置。精确函数类型还必须与约束中的
参数和返回类型相同。参数 tuple 不限制为 0～4 项：

```rils
fn invoke<T, F: Fn<(T,), T>>(value: T, callback: F) -> T {
    callback(value)
}
```

解释器会在调用时核对回调的参数、返回类型和捕获行为。只读捕获可满足 `Fn`，
修改捕获值的函数满足 `FnMut` 和 `FnOnce`，消费非 `Copy` 捕获值的函数只满足
`FnOnce`。不带捕获的普通函数满足全部三种约束。当前分类对不能证明可重复调用的
情况采取保守结果；函数类型本身仍只记录签名，不能单独证明捕获行为。
这三个 trait 由标准库封闭，不能用 `impl FnOnce/FnMut/Fn for 自定义类型` 手写实现。
`FnOnce` 约束保证回调至少可以调用一次；当前泛型函数体尚不静态限制调用次数，
再次调用已经消费捕获值的回调会在执行时报 move 错误。

当前字节码编译器对带调用 trait bound 的声明返回带源码范围的 `CompileError`，
直到共享前端能够验证同样的捕获语义。其他泛型 bound 的字节码检查仍在迁移中。
普通泛型 trait 实例的 bound 匹配、原生标准库回调桥接仍在迁移中；
部分带回调的方法继续使用旧执行入口。

`Debug` 是内建格式化 trait，Struct 与 enum 可以通过派生生成结构化调试表示：

```rils
#[derive(Default, Debug)]
struct State<T> {
    value: T,
}

#[derive(Debug)]
enum Message {
    Empty,
    Text(string),
}

println!("state = {:?}", state);
println!("state = {:#?}", state);
```

派生会递归检查字段并为泛型参数补充 `Debug` bound；字段格式化会继续调用字段类型自己的
`Debug::fmt`。`Display` 表示面向用户的稳定文本形式，不会自动派生。自定义实现通过
`Formatter::write_str` 写入结果：

```rils
impl core::fmt::Display for Point {
    fn fmt(
        &self,
        formatter: &mut core::fmt::Formatter,
    ) -> Result<(), core::fmt::FormatError> {
        formatter.write_str("point")
    }
}
```

`Formatter` 由格式化宏临时提供，不能由脚本自行构造或保存。解释器和字节码 VM 都会在 `{}`、
`{:?}` 处调用实际 trait 实现；`#[derive(Debug)]` 也使用同一分派通路。

当前暂不支持：

- 同一类型对同一泛型 trait 的不同类型实参分别实现
- 泛型 trait 的限定关联类型与带类型实参的 trait UFCS 路径
- trait 对象和 `dyn Trait`
- 关联常量
- `where` 子句
- 带条件的 trait impl，例如 `impl<T: Display> Trait for Box<T>`

带条件的 trait impl 会在共享 frontend 阶段返回明确诊断；AST 解释器和字节码编译器采用相同的
执行前 gate，不会静默忽略泛型参数上的 trait bound。

### 数组签名中的 const 长度参数

函数和方法签名支持 `const N: usize`，在 `[T; N]` 中保留长度关系，调用时从数组实参推导。
同一调用中重复使用的 `T` 和 `N` 必须一致；数组不能被整数、字符串或其他容器替代。

```rils
fn first<const N: usize>(values: [i32; N]) -> i32 { values[0] }
let value = first([1, 2, 3]);
```

当前 const 参数支持范围是数组签名与调用推导，不包含 const 表达式计算、函数体中的 const
值读取、默认 const 参数或显式 const 实参。`Vec::from` 的导出签名为
`from<const N: usize>(values: [T; N]) -> Vec<T>`，不再使用 `_` 放宽输入类型。
