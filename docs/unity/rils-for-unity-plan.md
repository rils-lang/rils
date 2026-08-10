# Rils for Unity 接入计划

## 1. 定位

Rils for Unity 面向需要快速迭代的游戏业务逻辑：开发期保存 `.rils` 后快速编译并热替换，正式构建默认加载已验证字节码，并在性能数据证明有必要时选择性切换到原生 AOT 后端。

目标不是替代 C# 或完整映射全部 Unity API，而是在 Unity 与 Rils 之间建立窄、稳定、可生成和可测试的宿主边界。

推荐职责划分：

| 层 | 主要职责 |
| --- | --- |
| Unity/C# | 生命周期、场景与资源、序列化、线程调度、原生插件加载 |
| Rils Unity facade | 稳定 C ABI、值编解码、对象句柄、诊断、模块与实例管理 |
| Rils 字节码 VM | 开发期执行、热重载、发布期默认执行、资源预算 |
| Rils 业务脚本 | 玩法、UI 流程、任务、技能、AI 状态与配置驱动逻辑 |
| 可选 AOT 后端 | 已测量的热点模块和不需要运行时替换的发布包 |

## 2. 当前基础与缺口

仓库当前已经具备：

- `lexer/parser -> static analysis -> HIR -> MIR -> bytecode -> verifier -> VM` 分层；
- AST 解释器与字节码 VM 两条执行路径；
- 多文件 `compile_file`、源码 Span、闭包、函数值和直接调用快速路径；
- 由稳定名称、签名、宿主 ABI 版本和 capability 构成的内存字节码导入表；
- `BytecodeHost` 链接前检查和指令步数/调用深度限制；
- 实验性 `.rilbc` v1 显式磁盘容器、严格加载验证，以及从 bytes/file 加载的 Rust/C#/C ABI；
- 解释器侧宿主模块、带类型函数、原生类型和实例方法注册。
- 实验性的 `rils_capi` C ABI：Windows DLL 构建、线程绑定 generation 句柄、panic 边界、
  内存源码编译、公开字节码函数标量调用，以及带源文件和 Span 的线程局部错误查询。
- `python tools/export-unity-package.py` 可导出 drop-in `Assets/Rils.CSharp` 源码包；根目录包含
  C# facade 与 asmdef，`Internal/x86_64` 包含当前 Windows 原生运行库。

Unity MVP 之前仍需补齐：

- 字节码 VM 与自定义游戏宿主函数的完整注册闭环；
- C ABI 字符串、数组、Option/Result、宿主对象句柄和反向回调值协议；
- 真正持有可变脚本状态的实例 API；当前原型实例只负责模块调用所有权，公开函数调用是无状态的；
- 跨边界值协议、Unity 对象句柄和错误所有权；
- 模块热替换及状态迁移协议；
- Unity Editor 导入、监听、Console 诊断和平台原生库打包；
- 堆内存、容器、字符串和宿主调用次数预算；
- `.rilbc` 跨版本兼容策略、可剥离调试信息和多文件源码身份；
- 可选 MIR 到 Rust AOT 后端。

在这些能力落地前，文档和示例必须把 Unity 接入标记为计划或实验能力，不能描述为稳定支持。

## 3. 总体架构

```text
RilsAsset / RilsBehaviour / Build Pipeline (C#)
                    │
             P/Invoke + C ABI
                    │
       rils_capi native facade (Rust)
          │          │           │
     Runtime      Handle table   Diagnostics
          │
   frontend → MIR → bytecode → verifier → VM
                    │
              typed host imports
                    │
             Unity C# facade APIs
```

### 3.1 C ABI 原则

- 只导出 `extern "C"`、固定宽度整数、字节切片和不透明句柄。
- 不跨 ABI 暴露 Rust enum、trait object、`String`、`Vec` 或内部内存布局。
- 所有入口捕获 Rust panic，并转换为可查询错误；panic 不得越过 FFI。
- 创建者负责释放，API 明确每个字符串、缓冲区、runtime、module 和 instance 的所有权。
- 每类句柄包含 generation，释放后再次使用返回确定错误。
- C ABI 版本、Rils 语言版本、字节码格式版本和宿主 ABI 版本独立维护。
- C# 层集中声明 P/Invoke，业务代码不得直接调用原生入口。
- Rils 调用 C# 宿主能力时使用静态生成的回调表；C# 必须保活 delegate，托管异常必须在边界内转成错误，不能越过反向 P/Invoke。

建议的首批概念 API：

```text
runtime_create / runtime_destroy
runtime_set_limits
module_compile / module_load_bytecode / module_destroy
module_list_imports / module_validate_host
instance_create / instance_destroy
instance_call
error_code / error_message / error_source_span
```

这里只规定职责，不在格式冻结前承诺具体函数签名。

### 3.2 值与对象边界

第一阶段限制跨边界值为 `()`、`bool`、具体整数类型、`f32`/`f64`、`char`、UTF-8 `string`、受限数组/Vec、Option、Result 和宿主对象句柄。复杂脚本 struct/enum 优先通过稳定的序列化值树传递，而不是镜像 Rust 内存布局。

Unity 对象使用 `(index, generation, type_id)` 形式的不透明句柄：

- C# 保有 Unity 对象的真实引用；
- Rust 只持有可验证的标识；
- 对象销毁、场景卸载或 runtime 释放时使句柄失效；
- 每次宿主调用验证 generation 和类型；
- 不允许 Rils 直接持有 `UnityEngine.Object` 指针。

### 3.3 宿主 API

API 使用显式模块，例如 `unity::log`、`unity::time`、`game::entity`，避免把 Unity 的大型继承式 API 原样映射进语言。

每个绑定应由同一份描述生成或校验：

- Rils/Analyzer 可见签名；
- 字节码导入名、签名、ABI 与 capability；
- Rust/C ABI glue；
- C# facade；
- 用户文档和绑定测试。

优先提供面向业务的粗粒度 API。不得在没有基准的情况下鼓励每帧大量细碎 P/Invoke 或反向回调。

## 4. 运行与热重载模型

### 4.1 三种构建模式

| 模式 | 输入 | 用途 | 是否包含前端 |
| --- | --- | --- | --- |
| 开发字节码 | `.rils` 保存后编译为内存模块 | Editor 快速迭代 | Editor/runtime 可配置包含 |
| 发布字节码 | 构建前生成并验证 `.rilbc` | 默认 Player 发布 | Player 可不包含 parser/analyzer |
| 原生 AOT | MIR 生成 Rust 后随平台插件构建 | 选择性热点优化 | 不需要运行时前端 |

AST 解释器继续作为语义参考和测试 oracle，不作为 Unity 默认执行路径。AOT 复用 MIR、所有权规则、宿主导入和错误语义，不建立独立语言分支。

当前 C# facade 已支持 Editor 构建链：`Compile/CompileFile` 得到 module，随后用
`GetBytecode()` 取得 `byte[]`，或用 `WriteBytecodeFile(path)` 直接写出 `.rilbc`。Addressables 构建
应把产物作为 `.bytes`/`TextAsset` 收集；Player 通过 `LoadBytecode(textAsset.bytes)` 加载，避免携带
源码与模块文件布局。

### 4.2 模块实例

编译后的 `Module` 应是不可变且可共享的代码与元数据；可变全局状态、闭包环境和宿主对象绑定属于 `Instance`。一个模块可以创建多个隔离实例，方便场景、测试和并行游戏世界分别管理状态。

Unity 生命周期只通过显式适配器调用。首个 `RilsBehaviour` 可以配置脚本资产、入口类型/工厂和是否启用若干回调，但不得假设所有脚本都有 `awake/start/update/on_destroy`。

### 4.3 事务式热重载

```text
文件变化 → 后台编译/验证 → 检查宿主 ABI → 导出旧状态
       → 创建新实例 → 迁移状态 → 主线程安全点原子切换
```

任一步失败都保留旧模块继续运行，并将带文件 URI 和 Span 的诊断发送到 Unity Console。第一版可以只支持“重启实例”；状态迁移应作为显式 opt-in 能力加入，不能隐式复制 VM 内存。

可持久状态只能包含宿主序列化协议支持的拥有型值。引用、活动调用帧、VM 内部闭包和未声明可恢复的宿主资源不能跨重载保存。事件订阅必须随旧实例卸载而解除。

## 5. 分阶段交付

### M0：边界原型与基准

状态：C ABI Rust 侧原型、Windows DLL 构建、独立 `.NET Standard 2.1` C# facade、托管端到端冒烟测试和
10,000 次 generation 句柄回收测试已完成；Unity Editor/Player 验证、原生内存基线和调用基准待完成。
当前 ABI 仍是实验接口，不承诺冻结。

范围：Windows x64 Editor 和 Standalone，仅验证 Mono 与 IL2CPP 所需接口形态。

- 新增独立的 `crates/rils_capi` facade crate，包名遵守 `rils_` 前缀并与主项目保持 `0.1.0`；不修改任何版本号。
- 建立 runtime/module/instance/error 句柄模型和 panic 边界。
- 从 C# 编译并执行一个内存脚本，传入两个数并取回结果。
- 测量空调用、标量参数、字符串、批量数组和 Rils→宿主回调成本。
- 用基准结果决定值协议和批量 API，不提前冻结 ABI。

验收：Editor 与 Windows Player 都能运行同一脚本；循环创建/释放 10,000 次无泄漏或失效句柄误用；错误包含 `.rils` 文件和 Span；形成可重复基准报告。

### M1：最小可用 Unity 包

- 创建 Unity Package Manager 包，包含 Runtime、Editor、Tests 和平台插件目录。
- 提供 `RilsAsset` 导入器、`RilsRuntime` 服务和最小 `RilsBehaviour`。
- 提供显式 lifecycle 调用、Unity 日志适配与少量业务示例绑定。
- 在程序集重载、进入/退出 Play Mode、场景卸载时可靠释放 runtime 与回调。
- 支持关闭 Domain Reload 时显式重置静态状态，避免重复注册。

验收：示例场景中 Rils 控制一个可见行为；重复进入 Play Mode 不重复回调、不残留句柄；EditMode 和 PlayMode 测试覆盖生命周期与错误路径。

### M2：开发期热重载

- 先明确编译器和编译产物的线程安全边界；移除/封装不可跨线程的 `Rc` 状态，或在后台只生成可转移 bytes，再启用异步解析、静态分析和字节码编译。
- 在主线程安全点提交新模块；失败时保持旧模块。
- Unity Console 诊断可点击跳转到正确 `.rils` 文件和行列。
- 支持手动重载、自动重载开关和去抖。
- 第一阶段重建实例；随后加入版本化状态导出/迁移协议。

验收：Play Mode 中修改逻辑可见效果更新；语法、类型、ABI 或迁移失败都不会停止旧逻辑；连续快速保存只提交最后一次有效结果。

### M3：宿主绑定工具链

- 定义单一绑定描述格式，并生成/校验 C# facade、Rils 签名和 Analyzer 元数据。
- 接入 `BytecodeHost` 自定义函数、原生类型/句柄方法和 capability policy。
- 首批模块聚焦日志、时间、输入快照、实体查询和批量命令提交。
- 所有 Unity 对象访问回到主线程；后台任务只处理明确标记为线程安全的数据。

验收：解释器与 VM 对同一绑定契约有对照测试；缺失函数、签名、ABI、capability 或对象 generation 不匹配均在确定位置失败。

### M4：发布字节码与沙箱

- 完成并冻结 `.rilbc` 的首个稳定格式、严格加载验证和可选调试 section。
- Unity 构建管线把 `.rils` 转换为目标包内 `.rilbc`，发布包默认不携带源码。
- Player 可裁剪 parser/analyzer，但始终保留 verifier。
- 增加堆内存、容器长度、字符串长度、调用深度、指令数和宿主调用次数预算。
- 生成构建清单，记录语言版本、格式版本、宿主 ABI、模块哈希和 capability。

验收：损坏、截断、版本不兼容、越界索引和超预算模块全部被拒绝；开发内存字节码与发布字节码结果一致。

### M5：跨平台

按 Windows → macOS → Android → iOS 的顺序扩展插件和 CI 构建矩阵。WebGL 单独立项评估 Rust/Wasm、Unity Web 平台链接方式、线程和文件系统限制，不作为首轮跨平台验收条件。

每个平台都需要运行 C ABI、句柄、生命周期、异常/错误、Player 构建和裁剪测试，不能仅以原生库成功加载作为完成标准。

### M6：可选 AOT

- 从现有 MIR 生成可读 Rust，先做 differential test，再做优化。
- AOT 模块继续通过同一宿主 ABI 和对象句柄访问 Unity。
- 构建缓存键至少包含源码、依赖、语言版本、编译器版本、目标平台和宿主 ABI。
- 支持项目或模块级选择字节码/AOT；开发期仍以字节码为默认。
- 以 profile 数据决定 AOT 范围，并与优化后的 VM、宿主调用批处理进行对比。

验收：代表性脚本在解释器、VM 和 AOT 下结果一致；所有权错误仍由共享前端报告；AOT 不绕过 capability 和宿主 ABI 检查。

## 6. 测试矩阵

Rust 侧至少覆盖：

- C ABI 空指针、重复释放、错误句柄、generation 失配和 panic 转换；
- 值编解码、UTF-8、超长输入和资源预算；
- 解释器/VM，以及未来 VM/AOT 的同源对照测试；
- 宿主导入缺失、签名/ABI/capability 不匹配；
- 热替换成功、编译失败、迁移失败和回滚。

Unity 侧至少覆盖：

- EditMode 导入、诊断映射和构建预处理；
- PlayMode 生命周期、场景切换、Domain Reload 开/关；
- Mono Editor 与 IL2CPP Player；
- Unity 对象销毁后的句柄失效；
- 长时间运行、频繁重载和 GC/原生内存基线。

语言、编译后端或 Analyzer 改动继续运行：

```console
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Unity 包另外运行其 EditMode/PlayMode 测试和实际 Player 构建。编辑器插件若有改动，继续执行对应的 `npm run check`。

## 7. 首个纵向 Demo

第一条完整链路只做一个场景：

1. `RilsBehaviour` 引用一个 `.rils` 资产；
2. 脚本通过少量类型化宿主 API 读取输入快照并提交移动命令；
3. Play Mode 中保存脚本后，在满足线程安全约束的编译通道中生成新模块并于安全点替换；
4. 合法修改立即改变角色行为；
5. 非法修改在 Console 显示可跳转诊断，旧逻辑继续运行；
6. Windows Player 构建使用构建时生成的字节码运行相同行为。

这个 Demo 同时验证产品价值、ABI 粒度、诊断、生命周期、热重载和发布路径，应先于大规模 Unity API 绑定或 AOT 实现。

## 8. 明确后置

以下内容不阻塞首个可用版本：

- 全量 Unity API 自动绑定；
- 任意脚本状态的透明热迁移；
- 运行时 JIT；
- 所有平台同步首发；
- AOT 与字节码自由混合调用的高级优化；
- Rils 直接持有 Unity 对象或参与 Unity 序列化器内部对象图。

先用 Windows 纵向 Demo 和基准验证工作流，再冻结 C ABI、绑定描述和 `.rilbc` 格式。
