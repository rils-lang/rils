# 发布与分支流程

本仓库采用短生命周期开发分支与发布分支相结合的流程。目标是让 `main` 保持可集成、可验证，
同时让 ABI、Host Manifest、字节码格式和编辑器产物在发布窗口内稳定下来。

## 分支约定

- `main` 是下一次可发布版本的集成分支。不得直接在其上开发功能或修复；所有改动先在短分支完成并
  通过检查后合入。
- `feature/<topic>` 用于用户可见能力或较大改动，`fix/<topic>` 用于常规修复，`docs/<topic>` 用于
  纯文档改动。
- 功能冻结后，从 `main` 创建 `release/<major>.<minor>`。该分支只接受 RC 修复、版本元数据、
  CHANGELOG、文档、绑定生成、发布脚本和产物验证，不再合入新功能或大范围重构。
- 已发布版本的补丁从对应 tag 创建 `hotfix/<major>.<minor>.<patch>`。修复发布后须同步回 `main`，
  避免后续版本重新引入问题。

## 日常合入

每个短分支在合入 `main` 前至少应完成与改动范围相称的测试。语言语义、编译后端或 Analyzer 改动
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

1. 从已通过日常检查的 `main` 创建 `release/<major>.<minor>`，冻结功能范围。
2. 在 release 分支上完成 RC 修复、文档与迁移说明。破坏性语法、类型、ABI 或磁盘格式变更必须在
   `CHANGELOG.md` 给出迁移步骤。
3. 获得明确同意后，统一更新主 crate、本地 `rils_*` crate、Analyzer 和编辑器插件的版本；不要因
   本地构建或重新打包自行递增版本号。
4. 执行完整发布门禁：Rust workspace 检查、C# binding `--check`、C API/C# DLL smoke、Unity 导出
   核对、VS Code 检查与打包验证。发布脚本生成的 `target/`、`dist/` 和 Unity 导出目录均为生成物，
   不提交到 Git。
5. 需要候选验证时，在 release 分支创建 annotated tag `vX.Y.Z-rc.N`，用于预览 VSIX 和宿主产物
   验收。
6. 所有门禁及产物验收通过后，在最终 release commit 创建 annotated tag `vX.Y.Z`。推送 tag、发布
   crates 或上传产物属于独立的外部操作，需明确授权。
7. 将 release 分支上的最终修复合回 `main`；若保留 release 分支，只用于该小版本后续 hotfix。

## 版本与兼容性

- 主 crate、主项目依赖的本地 crate、Analyzer 和 VS Code 插件以主项目版本为基线保持一致。
- `CHANGELOG.md` 的 `Unreleased` 只记录尚未发布的内容；正式发布时仅归档该版本实际完成的用户可见
  变更。
- Host Manifest、C ABI 与 `.rilbc` 的版本独立维护。它们发生不兼容变化时，release notes 必须说明
  是否需要重导出 Manifest、重新编译字节码/Unity `.bytes`，以及宿主侧库的成套升级要求。
