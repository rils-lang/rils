# Struct、Enum 与集合

[← 返回语言手册目录](README.md)

## Struct

Struct 可以使用命名字段，也可以声明为不保存状态的单位结构体：

```rust
struct Point {
    x: f64,
    y: f64,
}

let mut point: Point = Point {
    x: 3.0,
    y: 4.0,
};

println!("{}", point.x);
point.x = 10.0;

struct Marker;
struct Empty {}
```

`struct Marker;` 和零字段的 `struct Empty {}` 都是不保存字段的零大小名义类型。带字段 Struct
在构造时必须提供所有字段，不能提供未知字段，并且字段值必须符合声明类型。Struct 是名义类型：
字段相同但名字不同的两个 struct 不是同一种类型。

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

## String 与内建迭代器

`string` 是拥有型 UTF-8 字符串；`len()` 和 `find/rfind()` 返回 UTF-8 字节位置。Unicode 字符数量
应通过 `chars().count()` 获取，不能把字节长度当成字符数量：

```rust
let bytes = "R世".len();          // 4
let characters = "R世".chars().count(); // 2
```

字符串支持 `trim/trim_start/trim_end`、`to_lowercase/to_uppercase`、`repeat`、`replace`、
`strip_prefix/strip_suffix` 等拥有型结果。`chars()` 产生 `char`，`bytes()` 产生 `u8`，`lines()` 和
`split(pattern)` 产生新的 `string`；它们返回的内建迭代器均可直接用于 `for`。

`Iterator` 支持 `next/nth/count/last`，可通过 `take/skip/rev/enumerate` 继续组成迭代器，通过
`map/filter/filter_map` 转换或筛选，通过 `fold/for_each/any/all/find/position` 聚合和查询，或通过
`collect_vec()` 收集为 `Vec<T>`。这些默认方法同样适用于脚本实现的自定义 `Iterator`；`any/all/find/position`
会短路。`filter/find` 的谓词接收 `&T`，筛选拥有型非 Copy 元素时不需要 Clone。

除 `next/nth` 会推进现有迭代器外，上述方法会消费 receiver。当前转换适配器生成拥有型内建迭代器；
共享引用和可写引用的容器迭代器仍未实现。

## HashMap 与 HashSet

`HashMap<K, V>` 和 `HashSet<T>` 位于 prelude，也可通过 `std::collections` 访问。当前可作为键或
集合元素的类型是实现内建 `Eq + Hash` 的 `bool`、整数、`char` 和 `string`；浮点数会在静态分析
阶段拒绝。

```rust
let mut scores: HashMap<string, i32> = HashMap::new();
let player = "alice";
scores.insert(player.clone(), 42);
let score = scores.get_cloned(&player); // Option<i32>
let previous = scores.remove(&player);  // Option<i32>

let mut tags: HashSet<string> = HashSet::new();
tags.insert("player");
tags.insert("online");
```

Map 提供 `len/is_empty/clear/contains_key/insert/get_cloned/remove`。由于引用不能存入 `Option`，
查询接口不会返回 `Option<&V>`；`get_cloned` 明确生成拥有型副本。`keys_cloned`、`values_cloned`
和消费 Map 的 `into_iter` 分别产生键、值以及 `(K, V)` 的拥有型迭代器。

Set 还提供 `is_subset/is_superset/is_disjoint` 与
`union/intersection/difference/symmetric_difference`。集合代数返回新的拥有型 Set。Map 和 Set
都可直接用于 `for`，并会在进入循环时被消费。借用查询回调、借用迭代器以及 Map 索引 place
留待后续实现。

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
