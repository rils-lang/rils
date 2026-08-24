# 安装与环境包

Rils 的 GitHub Release 为常用桌面和服务器平台提供预编译环境包。普通用户只需下载和解压，不需要
安装 Rust、Cargo 或克隆源码仓库。

## 下载

在 [GitHub Releases](https://github.com/rils-lang/rils/releases) 中选择与系统匹配的归档：

| 系统 | 架构 | 文件名后缀 |
| --- | --- | --- |
| Windows | x86_64 | `windows-x86_64.zip` |
| Linux | x86_64 | `linux-x86_64.tar.gz` |
| Linux | aarch64 | `linux-aarch64.tar.gz` |
| macOS | Intel | `macos-x86_64.tar.gz` |
| macOS | Apple Silicon | `macos-aarch64.tar.gz` |

每个 Release 同时提供 `SHA256SUMS`。下载后可用系统自带或常用的 SHA-256 工具核对归档，确保文件
与 Release 中记录的摘要一致。

## 配置命令

环境包包含以下目录：

```text
rils-<version>-<platform>/
├── bin/
│   ├── rils
│   └── rils-analyzer
├── docs/
├── examples/
├── README.md
├── CHANGELOG.md
└── LICENSE
```

Windows 可将解压目录中的 `bin` 添加到用户 `PATH`；Linux 和 macOS 可以把该目录加入 shell 的
`PATH`，或将两个可执行文件复制到已有的用户级命令目录。配置完成后验证：

```console
rils --version
```

运行脚本和启动交互环境：

```console
rils examples/hello.rils
rils repl
```

VS Code 插件的正式平台包会自带匹配的 Analyzer；只有其他编辑器或自定义 LSP 客户端需要直接配置
`rils-analyzer` 命令。

## 从源码构建

从源码构建和维护发布包属于仓库开发流程，需要 Rust 工具链。普通 Rils 项目不应依赖 Cargo，也
不应把仓库内部的构建命令写入用户使用说明。维护者可参阅[发布与分支流程](release-process.md)。
