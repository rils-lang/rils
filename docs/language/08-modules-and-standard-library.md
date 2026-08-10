# 标准能力、模块与 IO

[← 返回语言手册目录](README.md)

## 标准宏与函数

| 名称 | 类别 | 说明 |
| --- | --- | --- |
| `print!(values...)` | 宏 | 连续输出，不换行，返回 `()` |
| `println!(values...)` | 宏 | 以空格分隔输出并换行，返回 `()` |
| `assert!(condition, message?)` | 宏 | 要求布尔条件为真 |
| `type_of(value)` | 函数 | 返回运行时类型名称 |
| `clone(&value)` | 函数 | 显式创建拥有型值的独立副本 |
| Option/Result 相关函数 | 函数 | 见[值与类型](01-values-and-types.md) |

`print`、`println` 和 `assert` 是保留的内置宏名，不能由用户再次声明；不带 `!` 的旧函数
调用写法会被视为普通的未定义名称。

时间、随机数和 JSON 等能力以后应作为显式模块提供。

## 模块、可见性与 Prelude

内联模块使用 `mod name { ... }`，只有带 `pub` 的声明能通过模块路径访问：

```rust
mod math {
    fn normalize(value: i32) -> i32 { value }

    pub fn add(left: i32, right: i32) -> i32 {
        normalize(left + right)
    }
}

use math::add as sum;
let answer = sum(20, 22);
```

文件模块写作 `mod name;`。`Engine::eval_file`、`rils::compile_file` 和 CLI 文件模式会依次查找
当前目录的 `name.rils` 与 `name/mod.rils`。字符串形式的 `Engine::eval` 和 `rils::compile` 不进行
隐式文件访问。加载器递归处理子模块并拒绝循环加载；`compile_file` 会把加载后的模块链接进同一个
内存字节码模块。

`use path::item;` 以最后一段作为本地名字，`use path::item as alias;` 可显式改名；`pub use`
可以从模块重新导出公开成员。当前暂不支持通配导入、分组导入以及 `crate`、`self`、`super`。

当前内置模块骨架为：

```text
core::{clone, iter, option, result}
std::{collections, io, fs}
prelude
```

`core::option` 与 `core::result` 分别导出对应构造器、状态判断、`unwrap` 和 `unwrap_or`；
同一组常用构造器也由 prelude 自动引入。

常用 Option、Result、Vec 和迭代器名字仍由 prelude 自动提供。`std::io::print` 与
`std::io::println` 当前是底层函数；日常输出仍推荐 `print!` 和 `println!` 宏。

### IO 与文件系统

可失败的标准库 IO 使用 `std::io::Error`：

```rust
use std::io::Error;

fn load(path: string) -> Result<string, Error> {
    std::fs::read_to_string(path)
}
```

`Error` 包含三个公开字段：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `kind` | `ErrorKind` | 可用于模式匹配的错误分类 |
| `message` | `string` | 底层操作系统错误消息 |
| `path` | `Option<string>` | 文件操作的相关路径，控制台 IO 为 `None` |

`ErrorKind` 当前提供 `NotFound`、`PermissionDenied`、`AlreadyExists`、`InvalidInput`、
`InvalidData`、`TimedOut`、`Interrupted`、`UnexpectedEof`、`WriteZero` 和 `Other`。

`std::fs` 初始接口：

| 函数 | 返回类型 |
| --- | --- |
| `read_to_string(path)` | `Result<string, Error>` |
| `write(path, text)` | `Result<(), Error>` |
| `append(path, text)` | `Result<(), Error>` |
| `try_exists(path)` | `Result<bool, Error>` |
| `read_dir(path)` | `Result<Vec<string>, Error>`，路径按字符串排序 |
| `create_dir_all(path)` | `Result<(), Error>` |
| `remove_file(path)` | `Result<(), Error>` |
| `remove_dir(path)` | `Result<(), Error>`，只删除空目录 |

`std::io` 另外提供 `read_line()`、`write(value)`、`write_line(value)` 和 `flush()`，这些接口也
返回 `Result`。`read_line()` 与 Rust 一样保留读取到的换行符。相对文件路径以宿主进程当前目录
为基准。类型标注可以直接使用 `std::io::Error` 等完整限定类型路径，也可以先通过 `use` 导入。

Rust 宿主可通过 `register_module`、`register_module_function` 注册多层模块和捕获状态的闭包。
`register_native_type` 返回类型句柄；句柄能创建封装 Rust payload 的值并注册实例方法。Rils
代码不能直接访问或下转 payload，只有宿主方法可以通过 `Value::host_payload<T>()` 读取它。
需要公开精确签名时可使用 `register_module_typed_function` 与 `register_typed_method`。签名会被
运行时用于参数和返回值校验；内置标准库的同一份签名也供类型推断和 Analyzer 使用。
