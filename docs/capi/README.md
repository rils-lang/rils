# Rils C API 与 C# facade（实验）

`rils_capi` 是宿主无关的原生 facade。当前只验证 Windows DLL，ABI 版本为 `3`，但在 M0
基准完成前不视为冻结接口，也不因此修改项目版本号。

## 构建

在 Windows 仓库根目录运行：

```console
python tools/build-capi.py
```

输出位于 `crates/rils_capi/dist/win-x64/`：

- `rils_capi.dll`
- `Rils.CSharp.dll`

`dist/` 是本地构建产物，不进入 git。独立 C# 项目位于
[`crates/rils_capi/csharp/Rils.CSharp`](../../crates/rils_capi/csharp/Rils.CSharp)，业务代码不应分散声明 P/Invoke。

## 当前调用模型

1. `rils_runtime_create` 创建 runtime，并可通过 `rils_runtime_set_max_steps` 设置单次调用预算。
2. `rils_module_compile` 从 UTF-8 内存源码编译；`rils_module_compile_file` 从入口文件加载并递归解析外部模块；
   `rils_module_load_bytecode` 从 bundle 提供的内存字节加载 `.rilbc`，`rils_module_load_bytecode_file` 用于普通文件。
3. 编译后的 module 可通过 `rils_module_bytecode_size` 查询大小，再用 `rils_module_write_bytecode` 写入
   调用方缓冲区；也可用 `rils_module_write_bytecode_file` 直接生成 `.rilbc` 文件。
4. `rils_instance_create` 创建调用实例。
5. `rils_instance_execute` 执行模块顶层入口；`rils_instance_call` 按完整名称调用无捕获的 `pub fn`。
6. `rils_module_trait_implementation_*` 枚举字节码中的 trait 实现；
   `rils_script_value_create_default` 创建持久 opaque script value，`rils_script_value_call_trait` 对其
   精确调用 trait 方法。枚举接口可传空 source name 查询整个 module，也可传完整来源名称只查询某个
   `SourceId` 声明的实现。
7. 逆序销毁 script value、instance、module、runtime；销毁父句柄也会释放其所有子句柄。

Runtime 还可在创建 module 前批量注册宿主函数描述、设置一个统一 dispatcher、单独授权 capability，
再冻结注册表。源码编译会使用同一份宿主签名产生 imports；bytes/file 加载和
`rils_module_validate_host` 会检查缺失函数、签名、宿主 ABI 和授权。非空宿主契约未冻结时不能创建
module，冻结后也不能继续修改函数、dispatcher 或 capability。

完整契约也可通过 `rils_runtime_register_host_manifest` 一次注册。该入口接受
[Host Manifest v1](host-manifest.md) `.rilhm` 二进制数据；`rils_runtime_host_manifest_size` 和
`rils_runtime_write_host_manifest` 可按两段式 API 导出经过 verifier 的规范二进制。JSON 只通过
显式 CLI/Rust 工具进行导入导出，不属于 Runtime 默认路径。Manifest 声明 capability 不代表授权，
仍需逐项调用 `rils_runtime_allow_capability`。

第一阶段 dispatcher 参数和返回值限制为 `()`、`bool`、`i32/u32/i64/u64` 和 `f32/f64`。描述数组、
名称、capability 和参数 tag 只在注册调用期间借用，原生层会复制；dispatcher 的参数和错误切片只在
一次回调期间有效。dispatcher 必须在创建 Runtime 的线程同步返回，不得重入 Rils C API。托管异常
需要在 dispatcher 内转换成非零状态和 UTF-8 错误，不能越过反向 P/Invoke。

`instance` 可拥有多个持久 opaque script value。复杂 Rils 值只保存在 DLL 内部，不暴露 Rust 布局；
C/C# 通过 generation handle 管理其生命周期。普通宿主调用
值协议支持
`()`、`bool`、全部定宽整数、`isize`/`usize`、`f32`/`f64` 和 `char`。`low`/`high` 保存整数位模式，
浮点数使用 IEEE 位模式，`char` 使用 Unicode scalar value。字符串、数组、Option/Result、对象句柄
尚未开放；dispatcher 的 UTF-8 string 值协议也留到下一阶段。

句柄编码了类型、创建线程和 generation。所有句柄必须在创建它们的线程使用；空句柄、类型错误、
跨 runtime、跨线程或释放后的句柄都返回 `RILS_STATUS_INVALID_HANDLE`。这是为 Unity 主线程调用模型
设定的第一阶段约束。

## 错误与所有权

返回值为 `RILS_STATUS_OK` 之外的状态时，可查询 `rils_last_error_*`。错误状态、UTF-8 消息、源文件名
和字节 Span 属于当前线程；字符串切片由 DLL 持有，在同一线程下一次非 getter C API 调用前有效，
调用方不得释放。所有可失败入口都捕获 Rust panic，panic 不会穿过 C ABI。

通过 `compile_file` 编译的项目或兼容模块树会保留每个脚本的 SourceId；编译错误和 VM 运行错误
返回实际出错依赖文件的名称，而不是始终使用入口路径。从 `.rilbc` v5 bytes/file 加载后该映射仍然保留。

输入 `RilsSlice` 仅在调用期间借用；输出句柄由创建者负责释放。`RilsValue.reserved` 必须为零，方便
后续在不复用现有字段含义的前提下扩展协议。完整声明见
[`crates/rils_capi/include/rils.h`](../../crates/rils_capi/include/rils.h)。Rust 构建 `cdylib` 本身不需要
这个头文件；它用于记录供 C/C++、C# P/Invoke 和绑定生成器消费的稳定布局与函数签名。

C# 低层 P/Invoke 已由 `python tools/generate-csharp-bindings.py` 从头文件生成；高层 facade 负责 UTF-8、
错误转换、线程校验、级联资源释放和标量转换。它只依赖 `.NET Standard 2.1`，不引用 Unity API，可以直接
复制到普通 C# 或 Unity 项目继续开发。高层入口对应为 `LoadBytecode(byte[])` 和
`LoadBytecodeFile(string)`；Unity AssetBundle/Addressables 场景应优先把 `TextAsset.bytes` 交给前者，
不需要将脚本恢复成磁盘目录。

编译导出对应 `RilsModule.GetBytecode()` 与 `WriteBytecodeFile(path)`。`GetBytecode` 使用“两段式”C ABI：
先查询长度，再将内容复制进托管 `byte[]`，不存在需要跨 DLL 释放的 Rust 内存。若调用方提供的 C
缓冲区不足，写入接口不会写 buffer，而会通过 `out_written` 返回所需长度。

最小调用示例和复制说明见
[`crates/rils_capi/csharp/README.md`](../../crates/rils_capi/csharp/README.md)。端到端冒烟项目会加载真实
Windows DLL，覆盖函数调用、128 位整数、Unicode scalar、编译诊断和父子句柄级联释放。
