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
零字段值可写成 `(Marker {})`；括号用于将空记录构造与控制流的空块区分开。

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
会消费容器。`values.iter()` 返回借用型 `Iter<&T>`，可通过 `next()` 或 `for` 读取元素，
不会消费容器：

```rust
{
    let values = [2, 3, 5];
    let mut sum = 0;
    for value in values.iter() {
        sum = sum + *value;
    }
    assert!(values.len() == 3usize);
}
```

借用迭代器和它产出的引用不能超过源集合的词法作用域。借用迭代期间不能结构修改 Vec；
`iter_mut()` 以及其他容器的借用迭代器尚未提供。

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

除 `next/nth` 会推进现有迭代器外，上述方法会消费 receiver。数组和 Vec 的拥有型 `into_iter()` 在调用 `next()` 时逐项移出元素；当前转换适配器和字符串迭代会先收集结果，再生成拥有型内建迭代器。
数组、Vec、HashMap、HashSet、BTreeMap 和 BTreeSet 已提供共享借用迭代，`iter_mut()` 尚未实现。

## VecDeque 与 BinaryHeap

`VecDeque<T>` 提供双端 `push_front/push_back`、`pop_front/pop_back`、`front_cloned/back_cloned`，
适合队列。`BinaryHeap<T>` 是最大优先队列，提供 `new/len/is_empty/push/pop/peek_cloned/clear`；
`pop` 每次取出最大元素，`peek_cloned` 显式克隆堆顶。它目前支持整数、`char` 和 `string` 元素；
不支持的类型在 `push` 时返回明确错误。两个类型都可由 prelude 或 `std::collections` 访问。

```rust
let mut priorities: BinaryHeap<i32> = BinaryHeap::new();
priorities.push(2);
priorities.push(5);
let highest = priorities.pop(); // Some(5)
```

## BTreeMap

`BTreeMap<K, V>` 是按键排序的拥有型 Map，可从 prelude 或 `std::collections` 访问。
支持 `new/len/is_empty/clear/contains_key/insert/get_cloned/remove`，
`first_key_cloned/last_key_cloned` 返回两端键的显式克隆；`into_iter()` 或直接用于 `for` 会消费 Map，
按键从小到大产生 `(K, V)`。`iter()` 则按键顺序借用并产生 `(&K, &V)`，不会消费 Map。
当前键类型限于 `bool`、整数、`char` 和 `string`；
浮点键等不支持的类型会在操作时返回错误。Rils 尚未提供通用 `Ord` trait，
因此自定义类型暂不能作为有序 Map 的键。

```rust
let mut scores: BTreeMap<string, i32> = BTreeMap::new();
scores.insert("b", 2);
scores.insert("a", 1);
let first = scores.first_key_cloned(); // Some("a")
for entry in scores {
    println!("{}: {}", entry.0, entry.1);
}
```

`BTreeSet<T>` 使用相同的有序元素约束，提供
`new/len/is_empty/clear/contains/insert/remove`、`first_cloned/last_cloned`，
以及 `is_subset/is_superset/is_disjoint/union/intersection/difference/symmetric_difference`。
集合运算返回新的拥有型 Set；`into_iter()` 或直接用于 `for` 会消费 Set 并按升序遍历。
`iter()` 产生按升序排列的 `&T`，且保留原 Set。

```rust
let mut values: BTreeSet<i32> = BTreeSet::new();
values.insert(3);
values.insert(1);
for value in values {
    println!("{}", value); // 1, 3
}
```

## HashMap 与 HashSet

`HashMap<K, V>` 和 `HashSet<T>` 位于 prelude，也可通过 `std::collections` 访问。当前可作为键或
集合元素的类型是实现内建 `Eq + Hash` 的 `bool`、整数、`char`、`string`，以及字段可递归作为键的
非泛型 struct 和 enum。后两者可用 `#[derive(Eq, Hash)]`；浮点数会在静态分析阶段拒绝。
两种容器都提供 `iter()`：Map 产生 `(&K, &V)`，Set 产生 `&T`；哈希容器的遍历顺序不保证固定。
借用迭代器或其产出的引用仍存活时，不能结构修改原集合。

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

Map 提供 `len/is_empty/clear/contains_key/insert/get_cloned/remove`。Map value 可以在局部作用域中携带引用；
`get_cloned` 返回对应值的副本（引用值会复制引用本身）。`keys_cloned`、`values_cloned`
和消费 Map 的 `into_iter` 分别产生键、值以及 `(K, V)` 的拥有型迭代器。

Set 还提供 `is_subset/is_superset/is_disjoint` 与
`union/intersection/difference/symmetric_difference`。集合代数返回新的拥有型 Set。Map 和 Set
都可直接用于 `for`，并会在进入循环时被消费。借用查询回调、借用迭代器以及 Map 索引 place
留待后续实现。

## Cell

`Cell<T>` 使用内部可变性，提供 `new`、`get`、`set` 和 `replace`。
`get()` 仅适用于实现 `Copy` 的值；非 `Copy` 值可用 `replace()` 取出旧值。
当前方法的泛型约束由运行时检查，前端提前报告此类错误仍待实现。

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
