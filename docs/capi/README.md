# Rils C API 与 C# facade（实验）

`rils_capi` 是宿主无关的原生 facade。当前只验证 Windows DLL，ABI 版本为 `1`，但在 M0
基准完成前不视为冻结接口，也不因此修改项目版本号。

## 构建

在 Windows 仓库根目录运行：

```console
python tools/build-capi.py
```

输出位于 `crates/rils_capi/dist/win-x64/`：

- `rils_capi.dll`
- `Rils.CSharp.dll`

需要直接复制到 Unity `Assets` 的源码包时运行：

```console
python tools/export-unity-package.py
```

默认输出 `crates/rils_capi/dist/unity/Rils.CSharp/`：

```text
Rils.CSharp/
├── RilsException.cs
├── RilsRuntime.cs
├── RilsValue.cs
├── NativeMethods.g.cs
├── Rils.CSharp.asmdef
└── Internal/
    └── x86_64/
        └── rils_capi.dll
```

这是 drop-in `Assets` 目录，不是 `.unitypackage`/UPM 包。可通过 `--output` 直接指定 Unity 项目内的
`Assets/Rils.CSharp`；脚本会替换该目标目录，因此不要把其他业务文件放在其中。目前只导出
Windows x86_64，后续架构继续放在 `Internal/<architecture>/`。

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
6. 逆序销毁 instance、module、runtime；销毁 runtime 也会释放其所有子句柄。

当前 `instance` 是生命周期和未来状态容器的占位实现，函数调用本身无持久 VM 状态。值协议支持
`()`、`bool`、全部定宽整数、`isize`/`usize`、`f32`/`f64` 和 `char`。`low`/`high` 保存整数位模式，
浮点数使用 IEEE 位模式，`char` 使用 Unicode scalar value。字符串、数组、Option/Result、对象句柄
与宿主回调尚未开放。

句柄编码了类型、创建线程和 generation。所有句柄必须在创建它们的线程使用；空句柄、类型错误、
跨 runtime、跨线程或释放后的句柄都返回 `RILS_STATUS_INVALID_HANDLE`。这是为 Unity 主线程调用模型
设定的第一阶段约束。

## 错误与所有权

返回值为 `RILS_STATUS_OK` 之外的状态时，可查询 `rils_last_error_*`。错误状态、UTF-8 消息、源文件名
和字节 Span 属于当前线程；字符串切片由 DLL 持有，在同一线程下一次非 getter C API 调用前有效，
调用方不得释放。所有可失败入口都捕获 Rust panic，panic 不会穿过 C ABI。

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
