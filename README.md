# Rils

Rils（Rust-Inspired Lightweight Script）是一门面向嵌入场景的轻量脚本语言，采用 Rust 风格语法、
显式所有权和可验证的字节码执行模型，并提供 Rust 宿主 API、静态分析器与 VS Code 支持。

## 快速开始

需要支持 Rust 2024 Edition 的稳定 Rust 工具链：

```console
cargo run -- examples/hello.rils
```

不传脚本路径会进入 REPL：

```console
cargo run
```

## Rust 嵌入

```rust
let value = rils::eval("1 + 2 * 3")?;
let module = rils::compile("let value = 40; value + 2")?;
let value = module.execute()?;
```

需要加载多文件项目或预编译模块时，可使用 `compile_file`、`BytecodeModule::read_file` 和 CLI
的 `compile`、`verify`、`run` 命令。

项目推荐在根目录提供 `rils.toml`：

```toml
[project]
name = "game_scripts"
script_paths = ["scripts"]
```

项目脚本会自动映射为模块，并支持 `crate`、`self`、`super` 路径。可执行入口提供零参数
`fn main()` 即可。

## 文档

- [语言手册](docs/language/README.md)
- [项目模型](docs/project.md)
- [Analyzer 与编辑器能力](docs/analyzer.md)
- [字节码设计](docs/bytecode.md)
- [C API 与 Host Manifest](docs/capi/README.md)
- [示例程序](examples)
- [VS Code 插件](editors/vscode-rils)
- [未来规划与待办](TODO.md)
- [变更日志](CHANGELOG.md)

当前代码处于 `0.2.0` 阶段，适合语言实验、工具开发和受控宿主嵌入。Unity、UE 等引擎集成由
各自独立的插件工程维护，不属于 Rils 核心仓库的版本标准。
