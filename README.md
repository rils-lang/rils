# Rils

Rils（Rust-Inspired Lightweight Script）是一门面向嵌入场景的轻量脚本语言，采用 Rust 风格语法、
显式所有权和可验证的字节码执行模型，并提供 Rust 宿主 API、静态分析器与 VS Code 支持。

## 快速开始

从 [GitHub Releases](https://github.com/rils-lang/rils/releases) 下载当前平台的 Rils 环境包，解压后将
其中的 `bin` 目录加入 `PATH`。环境包已经包含 `rils` 和 `rils-analyzer`，使用 Rils 不需要安装
Rust 工具链。完整步骤参见[安装与环境包](docs/installation.md)。

运行单文件脚本：

```console
rils examples/hello.rils
```

通过 `repl` 命令进入 REPL：

```console
rils repl
```

## Rust 嵌入

```rust
let value = rils::eval("1 + 2 * 3")?;
let module = rils::compile("let value = 40; value + 2")?;
let value = module.execute()?;
```

解释器与字节码 VM 默认允许 1024 层脚本调用。嵌入方可通过
`Engine::set_max_call_depth` 或 `BytecodeModule::execute_with_limits` 配置调用深度和指令步数预算；
超过预算会返回运行时错误，不会继续递归直到宿主线程栈溢出。

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

## 当前迭代重点

- 语言内置 `Default` 与 `#[derive(Default)]`，派生 Struct 的每个字段都必须满足 `Default`；Trait
  可以声明 supertrait。
- `.rilbc` v5 保留经过 verifier 校验的 trait implementation 身份，宿主可以发现实现、用
  `Default::default()` 构造持久脚本值并按 trait 方法调用。
- 项目支持路径源码依赖和实验性的 `.rilslib` 导出/验证。开发期仍以自动编译源码依赖为默认流程；
  入口到共享 `.rilslib` 的动态链接尚未完成。
- Host Manifest v2 支持命名宿主类型、单继承和独立 ABI transport；C ABI version 4 与 C# facade
  可以注册并调用保留逻辑类型身份的宿主对象。

## 文档

- [可运行示例](examples/README.md)
- [安装与环境包](docs/installation.md)
- [语言手册](docs/language/README.md)
- [项目模型](docs/project.md)
- [项目依赖与打包](docs/project-dependencies-and-packaging.md)
- [Rils 库产物](docs/library-artifacts.md)
- [Analyzer 与编辑器能力](docs/analyzer.md)
- [字节码设计](docs/bytecode.md)
- [发布与分支流程](docs/release-process.md)
- [C API 与 Host Manifest](docs/capi/README.md)
- [Unity 互操作边界](docs/unity-interoperability.md)
- [示例程序](examples)
- [VS Code 插件](editors/vscode-rils)
- [未来规划与待办](TODO.md)
- [变更日志](CHANGELOG.md)

当前代码处于 `0.3.0` 阶段，适合语言实验、工具开发和受控宿主嵌入。Unity、UE 等引擎集成由
各自独立的插件工程维护，不属于 Rils 核心仓库的版本标准。
