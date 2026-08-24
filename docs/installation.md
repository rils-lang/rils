# 安装与环境包

Rils 的 GitHub Release 为常用桌面和服务器平台提供预编译环境包。普通用户只需下载和解压，不需要
安装 Rust、Cargo 或克隆源码仓库。

## 下载

推荐先从 [GitHub Releases](https://github.com/rils-lang/rils/releases) 下载当前平台的独立
`rils-up`，它是无需 Rust 工具链的原生版本管理器。Windows 可以直接运行 `.exe`；Linux 和 macOS
首次下载后需要赋予执行权限。可以将带版本和平台后缀的下载文件重命名为 `rils-up`（Windows 为
`rils-up.exe`），随后安装当前稳定版：

```console
rils-up install stable
```

安装完成后，将提示的 `.rils/bin` 目录加入 `PATH`。此后使用固定的 `rils-up` 命令管理版本：

```console
rils-up update
rils-up install 0.4.0
rils-up default 0.4.0
rils-up list
```

常用管理命令：

| 命令 | 作用 |
| --- | --- |
| `rils-up install <version>` | 下载并校验指定版本；首次安装会自动成为默认版本 |
| `rils-up update` | 安装最新稳定版并将其设为全局默认版本 |
| `rils-up default <version>` | 切换全局默认版本，不重新下载 |
| `rils-up list` | 列出本机安装的全部版本 |
| `rils-up which [rils-analyzer]` | 显示当前选择的真实可执行文件及版本 |
| `rils-up uninstall <version>` | 删除非默认版本 |
| `rils-up home` | 显示 Rils 安装目录 |
| `rils-up self update` | 从独立的 `rils-up-v*` Release 更新全局管理器和代理 |

也可以直接选择与系统匹配的离线归档：

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

## 切换版本

`rils-up` 使用独立版本并全局只安装一份，不属于任何 Rils toolchain。`PATH` 中的 `rils` 和
`rils-analyzer` 是它安装的固定代理，真实工具链位于 `.rils/toolchains/<version>/bin`：

```text
~/.rils/
├── bin/
│   ├── rils-up
│   ├── rils
│   └── rils-analyzer
├── settings.toml
└── toolchains/
    ├── 0.4.0/bin/
    │   ├── rils
    │   └── rils-analyzer
    └── 0.4.1/bin/
        ├── rils
        └── rils-analyzer
```

切换默认版本只会原子更新 `.rils/settings.toml`，下一次启动命令时代理会选择新的真实可执行文件。
已经运行的 REPL、脚本和 Analyzer 不受中途切换影响。

`rils-up self update` 只更新全局管理器，不安装、删除或切换任何 Rils toolchain。Windows 会启动下载
的新管理器作为辅助进程，在当前进程退出后替换 `rils-up.exe`；若 Analyzer 正在占用旧代理，管理器
保持现有代理可用，并在后续不再占用时刷新。Linux 和 macOS 直接使用原子文件替换。

在项目根目录固定版本：

```console
rils-up override set 0.4.0
```

这会写入 `.rils-version`。选择顺序为 `RILS_TOOLCHAIN` 环境变量、从当前目录向上找到的最近
`.rils-version`、全局默认版本。也可以仅为一次命令选择版本：

```console
rils +0.3.0 script.rils
```

## 从源码构建

从源码构建和维护发布包属于仓库开发流程，需要 Rust 工具链。普通 Rils 项目不应依赖 Cargo，也
不应把仓库内部的构建命令写入用户使用说明。维护者可参阅[发布与分支流程](release-process.md)。
