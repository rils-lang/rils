# Struct、Enum 与集合

原生 `Option<T>` / `Result<T, E>` 的嵌套字段和模式绑定保留具体类型及原生存储。
拥有型模式先确认整条分支匹配成功，再移动绑定的非 Copy 负载；借用模式保持原对象的引用。
`?` 只传播错误分支，按当前函数返回声明建立目标 Result，例如
`Result<i32, Error>` 的 Err 可以传播到返回 `Result<string, Error>` 的函数。
`is_some/is_none/is_ok/is_err` 只读取标签，不要求负载实现 Clone。
用户 struct/enum 的 Copy 仍须显式实现，字段均为 Copy 不会自动赋予该能力。

`Some` 和 `None` 必须能确定完整的元素类型，构造失败不会回退为无类型的值。
类型标注、数组其他元素、比较另一侧、if 分支或方法实参可提供上下文，例如
`let absent: Option<i32> = None;` 和 `None.xor(Some(42))`。
独立的 `None` 或仅调用 `is_none(None)` 无法确定元素类型，会报错；请先提供类型标注。

`Ok` / `Err` 同样需要完整的 `Result<T, E>`，包括当前未激活的分支类型。类型可来自绑定、函数返回声明、if/match 的其他分支、泛型实参或 `?` 所在函数的返回类型；函数未写返回标注时也可从成功和错误分支共同推断。构造直接消费负载进入原生存储，不要求负载实现 Clone。无其他上下文的 `Ok(42)` 无法确定 `E`，`Err("missing")` 无法确定 `T`，都会报错。

```rust
let success: Result<i32, string> = Ok(42);
let failure: Result<i32, string> = Err("missing");
fn choose(flag: bool) {
    if flag { Ok(42) } else { Err("missing") }
}
```

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

普通 struct 和 enum 构造直接使用具体实例的原生布局；字段按所有权移入，无需 Clone。
外层类型标注会传入嵌套构造，例如 `Holder<i32>` 的 `Option<i32>` 字段可以直接写 `None`。
泛型参数即使没有出现在当前字段或变体中，也属于实例类型；构造时无法确定它的类型会报错，
应添加类型标注或明确的泛型参数。仅有 Copy 字段不自动获得 Copy，仍须显式实现。

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
旧的 `Array<T>` 名称已移除；原有类型标注应改为包含长度的 `[T; N]`。

```rust
let mut values: [i32; 3] = [10, 20, 30];
values[1] = 21;
let item = &mut values[2];
*item = 31;

let zeroes = [0; 8];
```

函数可用 `&[T]` 借用固定数组或 `Vec<T>` 的元素视图，调用时传入 `&values`；
切片不包含固定长度，也不会移动或复制整个容器。切片类型只能出现在引用内。

```rust
fn first(values: &[i32]) -> i32 { values[0] }
let values: [i32; 3] = [7, 8, 9];
let result = first(&values);
```

数组和切片可调用 `len()`、`is_empty()`、`contains()`、`iter()` 等共享读取方法。只有拥有型数组可调用消费式 `into_iter()`；对 `&[T]` 或 `&[T; N]` 请使用 `iter()` 借用遍历。

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

`Vec::new()` 等零参数泛型集合构造需要确定元素类型。可写 `let values: Vec<i32> = Vec::new();`
或 `let values = Vec::<i32>::new();`；`let values = Vec::new();` 无法确定类型，会在编译时报错。

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
HashMap、HashSet、BTreeMap 和 BTreeSet 也提供共享借用迭代；`iter_mut()` 尚未提供。

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
`collect_vec()` 收集为 `Vec<T>`。实现 `Iterator` 时必须声明 `Item` 并实现 `next`；上述其他方法均提供默认行为，也可以在 impl 中按原签名重写。它们的元素类型由 `Item` 决定，不要求它是类型的第一个泛型参数。`any/all/find/position`
会短路。`filter/find` 的谓词接收 `&T`，筛选拥有型非 Copy 元素时不需要 Clone。

除 `next/nth` 会推进现有迭代器外，上述方法会消费 receiver。`take/skip/rev` 的返回类型是 `Iterator<Item>`，与产生的新迭代器一致，不再是原 receiver 的 `Self`。数组和 Vec 的拥有型 `into_iter()` 接管原有元素存储，并在调用 `next()` 时逐项移出；`iter()` 借用元素，保留原集合的长度和索引。字符串的 `chars/bytes/lines/split` 按需生成下一项，不预先收集全部结果。`take/skip/rev/map/filter/filter_map/enumerate` 的默认实现目前仍会预先收集结果。这些默认行为由标准库的 trait 方法体导出，解释器和字节码共用同一份定义。

## VecDeque 与 BinaryHeap

`VecDeque<T>` 提供双端 `push_front/push_back`、`pop_front/pop_back`、`front_cloned/back_cloned`，
适合队列。`VecDeque<T>::into_iter()` 返回 `core::collections::VecDequeIntoIter<T>`；`for` 也会消费队列，并从队首到队尾产出元素。
`BinaryHeap<T>` 是最大优先队列，提供 `new/len/is_empty/push/pop/peek_cloned/clear`；
`pop` 每次取出最大元素，`peek_cloned` 显式克隆堆顶。它目前支持整数、`char` 和 `string` 元素；
不支持的类型在 `push` 时返回明确错误。`BinaryHeap<T>::into_iter()` 返回 `core::collections::BinaryHeapIntoIter<T>`；`for` 也会消费堆，遍历顺序不保证排序；需要从大到小取值时使用 `pop`。
两个类型都可由 prelude 或 `std::collections` 访问。

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
按键从小到大产生 `(K, V)`，返回的具体类型是 `core::collections::BTreeMapIntoIter<K, V>`。
`iter()` 则按键顺序借用并产生 `(&K, &V)`，不会消费 Map。
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
集合运算返回新的拥有型 Set；`into_iter()` 返回 `core::collections::BTreeSetIntoIter<T>`，直接用于 `for` 也会消费 Set 并按升序遍历。
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
`HashMap<K, V>::into_iter()` 返回 `core::collections::HashMapIntoIter<K, V>`，逐项移出键值对；`HashSet<T>::into_iter()` 返回 `core::collections::HashSetIntoIter<T>`，逐项移出元素。
两种容器都提供 `iter()`：Map 产生 `(&K, &V)`，Set 产生 `&T`；哈希容器的遍历顺序不保证固定。
借用迭代器或其产出的引用仍存活时，不能结构修改原集合。宿主保留的旧 Map/Set 存储也遵守这项规则；投影的比较、原生集合键查询和 Rust 借用读取持有原条目的共享访问 guard，不复制键或值。原生组合投影保留布局与声明上下文，访问冲突、已移出值或失效条目返回错误。 旧 Map/Set 的消费式迭代直接移动键和值，独占字符串与原生复合键不重建负载；若 Map 含已移出的 value，或集合正被借用/访问，则在取出条目前报错并保留集合内容。

原生 Map/Set 的 `contains_key` / `contains`、`get_cloned`、`remove` 直接借用查询键，
支持 Option/Result、tuple、数组及合法用户类型的组合，不要求查询键实现 Clone。
`get_cloned` 仍需复制返回值。查询检查完整键类型，用户 struct/enum 仍须实现 `Eq + Hash`；
字段可 Copy 不会自动使用户类型成为 Copy。整数、bool、char、string 的键身份与有序键能力
由标准库注册，BinaryHeap 的原生比较也复用这套注册。字符串及复合键的不可变身份数据
仍可能分配空间，但不再构造查询键的旧 Value 快照。

Rust 宿主保留的旧 Map/Set 查询也直接借用提取键身份，不要求查询值实现 Clone。
查询身份包含完整类型：例如不同整数宽度、`None::<i32>` 与 `None::<string>` 不混用；
旧包装的缺失类型可由集合声明补全，仍无法确定类型时返回错误。
`HashKey` 是不透明的拥有型键，直接访问旧 Rust 集合时应使用 `KeyIdentity` 查询，
避免用 `HashKey::from_value()` 为查询额外创建负载快照。键身份自身仍可能分配字符串或复合数据。


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

普通用户 struct/enum 的独立实例使用原生组合布局。Enum 保留活动变体标签，各变体字段按声明布局存放；
泛型实例保留具体类型，例如 `Choice<i32>`。构造和字段访问直接使用原生布局，旧 `Value::Struct` /
`Value::Enum` 及实例包装类型已删除。Rust 宿主使用 `TypedStorageContext` 的声明驱动构造器创建实例，
通过 `RilsValue::with_native_view()` 读取活动变体及字段，通过 `RilsValue::field()` 借用 struct 字段。

字段为 Copy 不会自动使 enum 成为 Copy，包括 unit 变体。只有显式实现 `Copy`，并且所有变体的
全部字段都满足 Copy，整个 enum 才可复制；非 Copy 值仍默认 move，复制必须显式 Clone。
