# 项目模型

Rils 项目通过项目根目录的 `rils.toml` 描述脚本根目录、项目名称、依赖和宿主 Manifest。项目模型
为跨文件模块解析、Analyzer 和宿主库依赖保留稳定边界。依赖库和 Unity 打包规则见
[项目依赖与打包](project-dependencies-and-packaging.md)。

## 配置

```toml
[project]
name = "game_scripts"
script_paths = ["Assets/Res/rils-script"]

[dependencies.rils_for_unity]
path = "Packages/com.rils-lang.rils-for-unity/Runtime/Rils"
prelude = true

[host]
manifest_dirs = [".rils/manifest"]
```

`name` 是项目对外的逻辑名称；`script_paths` 可以指定一个或多个脚本根目录。脚本根目录下
的文件按相对路径映射为模块：`gameplay/player.rils` 对应 `gameplay::player`，目录模块使用
`mod.rils`。项目中的脚本不需要通过外部 `mod name;` 互相挂接。

## 入口与路径

项目模式默认根据源码根目录下是否存在 `main.rils` 判断：存在时按可执行项目处理，并要求零参数
`fn main()`；不存在时按库项目处理。库项目也可以通过 `[lib]` 显式声明。模块引用支持：

- `crate::`：项目脚本根；
- `self::`：当前模块；
- `super::`：父模块；
- 普通路径：按当前模块和项目根进行解析。

`use` 支持单项、别名、通配和递归分组，例如：

```rils
use crate::api::*;
use crate::api::{Client, model::{User, Role}};
```

公开可见性由 `pub` 控制。Analyzer、编译器和项目加载使用同一模块路径规则。

## Host Manifest

项目可以在 `.rils/manifest/` 下按模块保存多个 `.rilhm` fragment，工具会按稳定顺序加载并
合并它们。需要单文件部署时可显式执行 Manifest link；JSON 只作为显式交换格式，不是运行时
默认产物。Manifest 的二进制布局见 [Host Manifest](capi/host-manifest.md)。

## 无项目配置时

没有 `rils.toml` 时，Rils 保留兼容加载规则：入口 `name.rils` 可以递归加载同目录的
`name/mod.rils` 以及显式声明的旧式模块文件。新项目推荐始终提供 `rils.toml`，以获得稳定的
项目根、模块身份和跨文件诊断。
