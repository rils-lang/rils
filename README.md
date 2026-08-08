# Rils

Rils（Rust-Inspired Lightweight Script）是一门面向嵌入场景的轻量脚本语言。它采用熟悉的
Rust 风格语法和显式所有权，同时保留脚本语言所需的快速验证与宿主集成能力。

```rust
struct Counter { value: int }

impl Counter {
    fn increment(&mut self) {
        self.value = self.value + 1;
    }
}

fn factorial(value: int) -> int {
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
```

## 文档

- [语言手册](docs/language/README.md)
- [示例程序](examples)
- [VS Code 插件](editors/vscode-rils)

当前代码处于 `0.1.0` 候选阶段，适合语言实验、工具开发和受控宿主嵌入。HashMap/HashSet、稳定的
字节码磁盘格式和完整的不可信脚本资源限制留待后续版本。
