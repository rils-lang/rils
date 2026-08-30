# Rils

Rils（Rust-Inspired Lightweight Script）是一门面向嵌入场景的轻量脚本语言，采用 Rust 风格语法、
显式所有权和可验证的字节码执行模型，并提供 Rust 宿主 API、静态分析器与 VS Code 支持。

## 快速开始

```console
rils --version
```

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

字符串形式的 `eval` 和 `compile` 不会隐式访问文件。多文件加载、项目配置与预编译字节码分别参见
[项目模型](docs/project.md)和[字节码设计](docs/bytecode.md)。

## Unity 嵌入

Unity 项目通过独立的 [RilsForUnity](https://github.com/rils-lang/RilsForUnity) 包接入。Editor 会把
`.rils` 源文件导入为经过验证的字节码资产，并为其中的 `RilsBehaviour` 实现生成可挂载入口；在场景中
添加 `RilsBehaviour` 组件并指定对应入口资产，即可由 Unity 生命周期驱动脚本。Player 只加载字节码，
不需要携带 Rils 源码或 Rust 工具链。

当前集成面向 Unity 2022.3 LTS 和 Windows x86_64。Unity API 调用必须位于创建运行时的主线程；
跨边界目前支持基础标量、UTF-8 字符串、固定布局 Unity 值类型、真实 enum 与 session 绑定的 Unity
对象句柄；集合仍需通过宿主 API 或自定义绑定转换。
安装、脚本模板和生命周期示例参见
[RilsForUnity 使用说明](https://github.com/rils-lang/RilsForUnity/blob/main/Packages/com.rils-lang.rils-for-unity/README.md)，
底层对象所有权与线程边界参见 [Unity 互操作边界](docs/unity-interoperability.md)。

## 文档

- [可运行示例](examples/README.md)
- [安装与环境包](docs/installation.md)
- [语言手册](docs/language/README.md)
- [项目模型](docs/project.md)
- [项目依赖与打包](docs/project-dependencies-and-packaging.md)
- [Rils 库产物](docs/library-artifacts.md)
- [编译器架构与迁移边界](docs/compiler-architecture.md)
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
