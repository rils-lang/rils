# Project dependencies and packaging

本文档记录 Rils 项目依赖库、prelude、Unity 资产导入和最终产物之间的关系。

## 依赖声明

项目通过 `rils.toml` 声明路径依赖：

```toml
[project]
name = "game"
script_paths = ["Assets/Res/rils-script"]

[dependencies.rils_for_unity]
path = "Packages/com.rils-lang.rils-for-unity/Runtime/Rils"
prelude = true
```

依赖库自身可以提供项目配置：

```toml
[project]
name = "rils_for_unity"
script_paths = ["src"]

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

启用依赖的 `prelude = true` 后，项目加载阶段会读取依赖声明的 prelude，并将其注入用户项目的根模块上下文。Prelude 不是用户可寻址的独立模块，也不应被当成 Unity 独立脚本资产导入。

因此，以下引用可以使用依赖库中的公开声明：

```rils
use crate::rils_for_unity::behaviour::RilsBehaviour;
```

## Unity 资产边界

RilsForUnity 自带的 `Runtime/Rils/` 源码属于库源码，不作为独立 Unity 资产导入。Unity importer 会根据最近的 `rils.toml` 判断源码是否属于 `[lib]` 项目；库项目中的 `.rils` 文件由 project dependency graph 加载。

用户项目中的 `.rils` 文件仍然会被 ScriptedImporter 编译为 `RilsBytecodeAsset`。因此多个 Rils 项目可以放在同一个 Unity 工作区中，只要各自拥有独立的 `rils.toml` 和源码根目录。

## 编译与最终产物

当前编译流程是项目级模块合并：

```text
用户脚本
  + 主项目模块
  + 依赖库模块
  + 依赖 prelude
        ↓
统一编译为 Rils bytecode
        ↓
RilsBytecodeAsset / .bytes
```

这意味着：

- 依赖库源码不需要复制到 Unity Player；
- `rils.toml` 不需要随 Player 发布；
- 依赖库中参与编译的代码会进入使用它的 bytecode 产物；
- Player 默认只加载 bytecode，不要求恢复 Rils 源码目录；
- 不作为独立 Unity 资产不会导致依赖内容丢失。

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
