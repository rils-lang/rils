# Impl、泛型与 Trait

[← 返回语言手册目录](README.md)

## Impl

Struct 和 enum 都可以拥有 `impl`：

```rust
impl Point {
    fn new(x: f64, y: f64) -> Point {
        Point { x: x, y: y }
    }

    fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }
}

let point = Point::new(3.0, 4.0);
println!(point.length_squared());
```

规则如下：

- 没有 `self` 参数的函数通过 `Type::function(...)` 调用。
- 实例方法的第一个参数必须是 `self`。
- 未标注的 `self` 自动视为当前名义类型。
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

当前暂不支持：

- 默认 trait 方法体
- 泛型 trait 本身
- trait 对象和 `dyn Trait`
- 关联常量
- `where` 子句
- 带条件的 trait impl，例如 `impl<T: Display> Trait for Box<T>`
- 孤儿规则与跨模块一致性检查
