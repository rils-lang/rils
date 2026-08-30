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

泛型类型采用运行时单态参数信息，但当前不会生成专用机器码。尚不支持显式 turbofish、默认类型参数、生命周期、const 泛型和 `where`。

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

Trait 方法当前没有默认实现，因此签名必须以分号结束。`Self` 表示正在实现该 trait 的具体类型。

以下 trait 由运行时预先声明，用户不能同名重定义：

```rust
trait Copy {}

trait Clone {
    fn clone(&self) -> Self;
}

trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}

trait IntoIterator {
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

派生会在前端生成普通的 `impl Default`，因此解释器、字节码编译器和 Analyzer 使用同一模型。每个字段类型都必须实现 `Default`，否则诊断会指向对应字段。同一类型不能同时派生并显式实现 `Default`。内部派生模型会为泛型字段记录所需的 `Default` bound；泛型条件 impl 的执行仍受本章末尾所述的当前限制。

Trait 方法保留其 trait 身份。同一类型可以实现多个带同名方法的 trait；普通方法调用只有在
候选唯一时才会自动选择，否则必须使用 UFCS：

```rust
Left::value(&both);
<Both as Right>::value(&both);
```

固有方法始终优先于同名 trait 方法。UFCS 不执行接收器自动借用，因此 `&self` 和
`&mut self` 方法需要显式传入引用。

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

- 默认 trait 方法体
- 泛型 trait 本身
- trait 对象和 `dyn Trait`
- 关联常量
- `where` 子句
- 带条件的 trait impl，例如 `impl<T: Display> Trait for Box<T>`

带条件的 trait impl 会在共享 frontend 阶段返回明确诊断；AST 解释器和字节码编译器采用相同的
执行前 gate，不会静默忽略泛型参数上的 trait bound。
