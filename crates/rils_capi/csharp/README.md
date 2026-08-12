# Rils C# facade

`Rils.CSharp` 是独立于 Unity 的托管封装，目标框架为 `.NET Standard 2.1`。复制到其他 C# 项目时需要同时提供对应平台的 `rils_capi` 原生库。

```csharp
using Rils.CSharp;

using var runtime = new RilsRuntime();
using var module = runtime.Compile(
    "pub fn add(left: i32, right: i32) -> i32 { left + right }",
    "calculator.rils");
using var instance = module.CreateInstance();

int answer = instance.Call("add", 20, 22).AsI32();
```

需要使用外部模块时，让原生编译器从入口文件加载：

```csharp
using var module = runtime.CompileFile("scripts/main.rils");
using var instance = module.CreateInstance();
RilsValue result = instance.Execute();
```

`CompileFile` 使用与 Rust API 相同的 `name.rils` / `name/mod.rils` 递归模块规则和循环检测；C# 层不解析 `mod` 或 `use`。

Unity Editor 或其他构建工具可以把编译结果导出为 Addressables 输入：

```csharp
using RilsModule module = runtime.CompileFile("scripts/main.rils");
byte[] image = module.GetBytecode();
File.WriteAllBytes("Assets/RilsScripts/main.rilbc.bytes", image);

// 或由原生层直接写文件
module.WriteBytecodeFile("Assets/RilsScripts/main.rilbc.bytes");
```

写入 Unity `Assets` 后需要由 Editor 侧刷新 AssetDatabase，再把生成的 `.bytes` 标记为 Addressable；
Player 不应执行源码编译，只加载构建阶段生成的 bytes。

发布包可只携带离线生成的 `.rilbc`。从 Unity AssetBundle/Addressables 取得字节后直接加载，不需要
恢复源码目录：

```csharp
byte[] image = scriptTextAsset.bytes;
using var module = runtime.LoadBytecode(image);
using var instance = module.CreateInstance();
RilsValue result = instance.Execute();
```

普通桌面程序也可使用 `runtime.LoadBytecodeFile("scripts/main.rilbc")`。两种入口都会在创建 module
前完成格式、校验和和字节码 verifier 检查。

`RilsRuntime`、`RilsModule` 和 `RilsInstance` 都必须在创建线程显式 `Dispose`。它们没有使用 `SafeHandle` finalizer，因为 finalizer 线程不满足当前原生 ABI 的线程绑定约束。销毁父对象会先按顺序销毁其托管子对象。

低层声明位于 `Generated/NativeMethods.g.cs`，不要手工编辑。修改 `include/rils.h` 后从仓库根目录运行：

```console
python tools/generate-csharp-bindings.py
python tools/generate-csharp-bindings.py --check
```

当前高层封装支持 C ABI 已开放的全部普通调用标量。`RilsInt128`/`RilsUInt128` 使用高低 64 位保存，
`RilsChar` 保存完整 Unicode scalar value。`RilsRuntime.RegisterHostManifest(byte[])` 和
`GetHostManifest()` 注册、导出 `.rilhm` 二进制契约，不使用 JSON。生成的低层 P/Invoke 已包含标量
HostContract dispatcher 入口；高层静态 dispatcher/Attribute 注册、字符串、集合和 Option/Result
等待后续扩展。

## 导出到 Unity

从仓库根目录运行：

```console
python tools/export-unity-package.py
```

生成的 `crates/rils_capi/dist/unity/Rils.CSharp` 可以整体复制到 Unity 的 `Assets`。其中 C# 源码位于
根目录，`Internal/x86_64/rils_capi.dll` 是当前 Windows x86_64 原生运行库，asmdef 已启用 unsafe
代码。也可以直接指定目标：

```console
python tools/export-unity-package.py --output D:/Game/Assets/Rils.CSharp
```

导出会替换完整目标目录；Unity 业务代码应放在该目录之外。
