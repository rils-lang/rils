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

推荐使用项目模式。在项目根目录放置 `rils.toml`，文件名自动映射为模块路径，不需要再写
`mod name;`：

```toml
[project]
name = "unity_game"
script_paths = ["Assets/Res/rils-script"]

[host]
manifest_dirs = [".rils/manifest"] # 可选；未配置时也是默认目录
manifests = ["generated/extra.rilhm"] # 可选的额外 fragment
```

`player.rils` 映射为 `player`，`gameplay/player.rils` 映射为 `gameplay::player`，
`gameplay/mod.rils` 映射为 `gameplay`。同一模块路径出现两个文件会直接报错。项目中的任意脚本都
可以作为构建入口，但被选中的入口必须定义零参数 `fn main()`；入口的返回值就是脚本执行结果。
项目加载器把脚本根目录中的模块链接进同一个字节码产物。

项目路径支持 Rust 风格锚点：`crate::` 从当前项目根开始，`self::` 从当前文件模块开始，
`super::` 返回父模块且可以重复。`use crate::gameplay::player as player;` 与完整限定调用都可使用。
`project.name` 是稳定的 crate 标识，为后续外部项目依赖预留；当前项目内部应使用 `crate::`。

`.rils/manifest/**/*.rilhm` 会按规范化路径排序并合并成一个逻辑 Host Contract。相同声明可以在
多个 fragment 中幂等出现；ABI/contract/module 版本、函数名称、签名或全局 function ID 冲突都会
使整个项目加载失败。旧的 `[host].manifest` 单文件配置继续兼容。

Host Manifest v2 可以声明命名宿主类型和单继承。类型路径可直接用于标注和推断，不需要在 Rils
源码中重复声明：

```rils
let object: unity_engine::GameObject = unity_engine::object::get();
let id = object.instance_id(); // instance_id 声明在 unity_engine::Object
```

宿主类型也遵循普通的 `use` 名称解析。显式导入、通配导入和 `as` 别名都会解析到 manifest 中的
完整类型身份，后续的类型推断、继承成员查找与 Analyzer 使用同一结果：

```rils
use unity_engine::*;

fn inspect(object: GameObject) {
    object.instance_id();
}
```

短名必须通过 `use` 进入当前作用域；多个通配导入带来同名宿主类型时会报告候选列表，必须改用
显式导入、别名或完整限定名，不能按导入顺序静默选择。

派生宿主类型可传给基类参数，并继承基类的 receiver 方法。它们在 Rils 中保持不同的逻辑类型，在
宿主 ABI 上则按 manifest 声明降级到 transport；当前命名类型使用 `HostHandle`。这不会改变 Rils
拥有型 struct/enum 的语义，也不允许脚本访问宿主 payload。

没有 `rils.toml` 时保留旧的单文件兼容模式：`mod name;` 依次查找同目录的 `name.rils` 与
`name/mod.rils`。项目模式只保留 `mod name { ... }` 作为局部内联模块，外部 `mod name;` 会给出
迁移诊断。字符串形式的 `Engine::eval` 和 `rils::compile` 始终不进行隐式文件访问。

`use path::item;` 以最后一段作为本地名字，`use path::item as alias;` 可显式改名。通配导入只引入
目标模块的公开直接成员，分组可以递归嵌套并在叶子处改名：

```rust
use crate::api::*;
use crate::model::{User, Role as UserRole, nested::{Config, Error}};
```

同一作用域中导入重名会报告冲突，不会按照书写顺序静默覆盖；私有成员不会由 `*` 暴露。
`pub use path::item;` 可以从模块重新导出单个公开成员；通配重新导出的完整静态链接支持仍属于后续增强。

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
