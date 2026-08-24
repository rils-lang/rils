# 发布与分支流程

本仓库采用版本分支承载迭代、短生命周期分支承载具体改动的流程。目标是让 `main` 始终对应已经
完成版本集成和发布验证的基线，同时让 ABI、Host Manifest、字节码格式和编辑器产物在版本分支内
逐步稳定下来。

## 分支约定

- `main` 是完成版本迭代后用于发包的稳定分支。不得直接在其上开发功能或修复。
- `develop/<major>.<minor>` 是对应版本的集成分支，例如 `develop/0.4`。该版本的 feature、fix 和
  docs 分支都必须从它拉出，完成后也合并回同一版本分支。
- `feature/<topic>` 用于用户可见能力或较大改动，`fix/<topic>` 用于常规修复，`docs/<topic>` 用于
  纯文档改动。短分支不得绕过版本分支直接合并到 `main`。
- 功能冻结后，版本分支只接受 RC 修复、版本元数据、CHANGELOG、文档、绑定生成、发布脚本和产物
  验证，不再合入新功能或大范围重构。
- 已发布版本的补丁从对应 tag 创建 `hotfix/<major>.<minor>.<patch>`。修复发布后须同步回 `main`，
  并合入仍在维护的后续版本分支，避免重新引入问题。

## 日常合入

每个短分支在合入对应版本分支前至少应完成与改动范围相称的测试。语言语义、编译后端或 Analyzer 改动
必须执行：

```console
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

VS Code 插件改动还必须执行：

```console
npm run check --prefix editors/vscode-rils
```

C API 或 C# 改动还必须验证绑定生成、托管构建和真实动态库 smoke test。Unity 导出改动额外核对
导出源码、原生库与生成物一致，但 RilsForUnity 作为独立工程，不随核心仓库版本自动变更。

## 发布流程

1. 在 `develop/<major>.<minor>` 版本分支冻结功能范围并完成 RC 修复、文档与迁移说明。破坏性语法、
   类型、ABI 或磁盘格式变更必须在
   `CHANGELOG.md` 给出迁移步骤。
2. 获得明确同意后，统一更新主 crate、本地 `rils_*` crate、Analyzer 和编辑器插件的版本；不要因
   本地构建或重新打包自行递增版本号。
3. 执行完整发布门禁：Rust workspace 检查、C# binding `--check`、C API/C# DLL smoke、Unity 导出
   核对、VS Code 检查与打包验证。发布脚本生成的 `target/`、`dist/` 和 Unity 导出目录均为生成物，
   不提交到 Git。
4. 使用 `python tools/package-rils.py` 在当前平台生成本地环境包；完整发布验证使用
   `python tools/release-rils.py`。这些是维护者入口，面向 Rils 用户的文档只使用已安装的 `rils`
   或其他 `rils-xxx` 命令，不要求 Cargo 或 Rust 工具链。
5. 需要候选验证时，在版本分支创建 annotated tag `vX.Y.Z-rc.N`，触发多平台环境包和 GitHub
   prerelease 验收。
6. 所有门禁及产物验收通过后，将版本分支合并回 `main`，在最终 release commit 创建 annotated tag
   `vX.Y.Z` 并推送。GitHub Actions 会构建 Windows、Linux 和 macOS 环境包，生成统一的
   `SHA256SUMS`，并将全部产物附加到同一个 GitHub Release。
7. 发布 crates 或编辑器市场包仍属于独立外部操作，需明确授权。版本分支可保留用于该小版本后续
   hotfix，并将修复同步回 `main` 和后续版本分支。

## 版本与兼容性

- 主 crate、主项目依赖的本地 crate、Analyzer 和 VS Code 插件以主项目版本为基线保持一致。
- `rils-up` 使用独立版本线并全局只安装一份；Rils toolchain 发版不得自动递增管理器版本，也不得在
  每个 toolchain 目录中重复打包管理器。安装器或 Release 可以携带经过兼容性验证的独立
  `rils-up` 资产。独立发布使用 `rils-up-vX.Y.Z` tag，触发多平台管理器工作流；tag 版本必须与
  `tools/rils-up/Cargo.toml` 一致。`rils-up self update` 只选择带 `SHA256SUMS` 的最高稳定管理器资产。
- `CHANGELOG.md` 的 `Unreleased` 只记录尚未发布的内容；正式发布时仅归档该版本实际完成的用户可见
  变更。
- Host Manifest、C ABI 与 `.rilbc` 的版本独立维护。它们发生不兼容变化时，release notes 必须说明
  是否需要重导出 Manifest、重新编译字节码/Unity `.bytes`，以及宿主侧库的成套升级要求。
