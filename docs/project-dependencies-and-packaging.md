# Project dependencies and packaging

本文档记录 Rils 项目依赖库、prelude、Unity 资产导入和最终产物之间的关系。

## 依赖声明

项目通过 `rils.toml` 声明路径依赖：

```toml
[project]
name = "game"
src = "Assets/Res/rils-script"

[dependencies.rils_for_unity]
path = "Packages/com.rils-lang.rils-for-unity/Runtime/Rils"
prelude = true
```

依赖库自身可以提供项目配置：

```toml
[project]
name = "rils_for_unity"
src = "src"

[lib]
prelude = "src/prelude.rils"
```

`rils_project` 负责解析依赖路径、校验依赖名称、收集源码根目录、建立带有依赖名称前缀的模块路径，并记录 prelude 文件。

## 项目模式

项目默认根据源码布局判断模式：

- 源码根目录存在 `main.rils` 时，按可执行项目处理，并要求零参数 `fn main()`；
- 没有 `main.rils` 时，按库项目处理，不要求 `main`；
- `[lib]` 可以显式声明库项目，并配置库的 prelude。

库项目可以被其他项目作为依赖加载，整个库的模块都属于项目源码图的一部分。

## Prelude

启用依赖的 `prelude = true` 后，项目加载阶段会读取依赖声明的 prelude，并将其注入用户项目的
根模块上下文。Prelude 不是普通的可寻址模块：编译器只注入一次，不会再以常规模块路径重复加载。
不过在 Unity 中，prelude 源文件仍会导入为可检查、可追踪依赖关系的 `RilsScriptAsset` 主资产；
以它触发导入时会编译完整库项目。

依赖 prelude 暴露的常用名称可以直接使用。例如 RilsForUnity 项目中的生命周期脚本无需手写
`use`：

```rils
#[derive(Default)]
pub struct PlayerBehaviour;

impl RilsBehaviour for PlayerBehaviour { /* lifecycle methods */ }
```

公开声明仍保留完整库路径身份，例如
`crate::rils_for_unity::behaviour::RilsBehaviour`，供显式引用、诊断和 Analyzer 定位定义使用。

## Unity 资产边界

Unity 工作区中的每个 `.rils` 文件都会由 ScriptedImporter 导入为 `RilsScriptAsset` 主资产；脚本中每个被识别出的 `RilsBehaviour` 实现会生成一个 `RilsEntryAsset` 子资产。源码位于 `[lib]` 项目中也不会改变这一资产关系；编译器仍通过最近的 `rils.toml` 和 project dependency graph 自动解析同项目模块与源码依赖。

`[lib].prelude` 也是可正常选择的 `RilsScriptAsset`。它仍作为库的特殊根声明注入，不会同时以普通模块路径重复加入。

因此多个 Rils 项目可以放在同一个 Unity 工作区中，只要各自拥有独立的 `rils.toml` 和源码根目录。开发期不要求先把源码依赖手工导出成 `.rilslib`。

## 编译与最终产物

当前编译流程是项目级模块合并：

```text
用户脚本
  + 主项目模块
  + 依赖库模块
  + 依赖 prelude
        ↓
统一编译为当前入口 bytecode
        ↓
RilsScriptAsset
  └─ 0..N RilsEntryAsset
```

这意味着：

- 依赖库源码不需要复制到 Unity Player；
- `rils.toml` 不需要随 Player 发布；
- 依赖库中参与编译的代码会进入使用它的 bytecode 产物；
- Player 默认只加载 bytecode，不要求恢复 Rils 源码目录；
- 每个 entry 子资产共享主资产中的 bytecode 与 host manifest，不重复内嵌大块数据。

这与 Rust 的“依赖先编译，再链接”在最终部署效果上相近，但当前 Rils 仍采用模块合并式 bytecode，而不是独立库文件链接。

## 增量导入要求

依赖库源码或依赖配置变化时，所有依赖它的用户脚本都应重新导入。Unity importer 除了追踪显式 `mod` 文件外，还需要追踪：

- 项目 `rils.toml`；
- 依赖库的 `rils.toml`；
- 依赖库源码文件；
- 依赖库 prelude。

这是 Unity 增量导入正确性的必要条件，后续应通过 `DependsOnSourceAsset` 或等价机制补齐。

## 后续优化

当前模型优先保证语义一致性和嵌入简单性。后续可以增加：

- project 级依赖编译缓存；
- 依赖源码稳定 hash；
- 多个用户脚本共享的编译结果；
- 独立依赖 bytecode 和 linker；
- 未引用模块和成员的裁剪；
- Unity 构建阶段统一生成并复用依赖产物。

这些优化不能改变依赖解析、SourceId、模块路径和 prelude 注入的语义。

## 可分发库产物

库项目可以使用 `rils library compile` 显式导出 `.rilslib`，格式与当前能力边界见
[Rils 库产物](library-artifacts.md)。开发期的路径依赖仍直接使用源码并自动参与项目编译；二进制
依赖声明和入口到共享库的动态链接仍属于下一阶段，不会在尚未具备链接语义时静默回退为内嵌依赖。
