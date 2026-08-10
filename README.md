# Rils

Rils（Rust-Inspired Lightweight Script）是一门面向嵌入场景的轻量脚本语言。它采用熟悉的
Rust 风格语法和显式所有权，同时保留脚本语言所需的快速验证与宿主集成能力。

```rust
struct Counter { value: i32 }

impl Counter {
    fn increment(&mut self) {
        self.value = self.value + 1;
    }
}

fn factorial(value: i32) -> i32 {
    if value <= 1 { 1 } else { value * factorial(value - 1) }
}

let mut counter = Counter { value: 0 };
counter.increment();
println!("6! =", factorial(6));
```

Rils 目前提供解释器和字节码 VM 两条执行路径，并带有静态分析器与 VS Code 支持。主要能力包括：

- 非 Copy 值默认 move，复制非 Copy 值需要显式 Clone。
- 词法局部 `&T` / `&mut T`，不采用 Rust 的唯一可变借用限制。
- struct、enum、trait、impl、泛型、模块、模式匹配和函数式宏。
- tuple、数组、`Vec<T>`、Range、Option、Result 与 `?` 错误传播。
- Rust 风格的定宽数值类型、`usize` 索引、`char`，以及受用法约束的无后缀数值字面量推导。
- 函数值、闭包、递归、UFCS 和自定义 Iterator/IntoIterator。
- 可注册宿主模块、函数、原生类型和方法。

## 快速开始

使用支持 Rust 2024 Edition 的稳定 Rust 工具链运行示例：

```console
cargo run -- examples/hello.rils
```

不传脚本路径会进入 REPL：

```console
cargo run
```

仓库中的 [examples](examples) 还包含闭包、引用、集合、trait、模块、宏和标准库等示例。

## 嵌入 Rust

一次性解释执行：

```rust
let value = rils::eval("1 + 2 * 3")?;
```

需要保留全局环境时使用 `Engine`：

```rust
let mut engine = rils::Engine::new();
engine.eval("let mut total = 40;")?;
let value = engine.eval("total = total + 2; total")?;
```

也可以编译为经过验证、可重复执行的内存字节码模块：

```rust
let module = rils::compile("let value = 40; value + 2")?;
let value = module.execute()?;

// 加载入口文件及其依赖的模块
let module = rils::compile_file("scripts/main.rils")?;

// 生成/加载实验性磁盘字节码；加载时会校验容器、版本、校验和和全部指令索引
module.write_file("scripts/main.rilbc")?;
let module = rils::BytecodeModule::read_file("scripts/main.rilbc")?;
let value = module.execute()?;
```

CLI 同样支持 `rils compile scripts/main.rils -o scripts/main.rilbc`、
`rils verify scripts/main.rilbc` 和 `rils run scripts/main.rilbc`。`compile_file` 会先递归解析
`mod`，因此产物是无需源码文件布局即可加载的单个模块。

字节码模块还可以按名称重复调用公开函数；宿主无关的实验性 C ABI 已放在
[`crates/rils_capi`](crates/rils_capi)，当前提供 Windows DLL 与独立 C# facade。接口范围和限制见
[`docs/capi`](docs/capi)。

独立的 `.NET Standard 2.1` 封装位于 [`crates/rils_capi/csharp/Rils.CSharp`](crates/rils_capi/csharp/Rils.CSharp)，
不依赖 Unity。运行 `python tools/build-capi.py` 会生成低层 P/Invoke，并一起输出 `rils_capi.dll`、
`Rils.CSharp.dll`。C# 可通过 `Compile/CompileFile` 编译源码，再用 `RilsModule.GetBytecode()` 或
`WriteBytecodeFile()` 生成供 Addressables 使用的字节码产物。

需要源码形式的 Unity drop-in 包时运行 `python tools/export-unity-package.py`。默认生成
`crates/rils_capi/dist/unity/Rils.CSharp`，其中 C# facade 位于根目录，Windows x86_64 原生库位于
`Internal/x86_64/`。

## 文档

- [变更日志](CHANGELOG.md)
- [语言手册](docs/language/README.md)
- [示例程序](examples)
- [VS Code 插件](editors/vscode-rils)
- [Unity 接入计划](docs/unity/rils-for-unity-plan.md)

当前代码处于 `0.1.0` 候选阶段，适合语言实验、工具开发和受控宿主嵌入。`.rilbc` v1 已可实验
使用，但尚未承诺跨版本稳定；HashMap/HashSet 和完整的不可信脚本资源限制留待后续版本。
