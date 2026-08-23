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

当前高层封装支持 C ABI 已开放的全部普通调用标量与固定布局 inline host value。
`RilsInt128`/`RilsUInt128` 使用高低 64 位保存，`RilsChar` 保存完整 Unicode scalar value；
`RilsInlineValue` 通过无分配 reader/writer 按 `fields(...)` 声明显式打包小端标量字段；旧
`f32x2/f32x3/f32x4` 布局仍可读取。`RilsRuntime.RegisterHostManifest(byte[])` 和
`GetHostManifest()` 注册、导出 `.rilhm` 二进制契约，不使用 JSON。生成的低层 P/Invoke 已包含标量
HostContract dispatcher 入口；高层静态 dispatcher/Attribute 注册、字符串、集合和 Option/Result
等待后续扩展。

## 宿主 Binding IR

`RilsHostFunctionDescriptor` 将宿主声明与 managed handler 分离，`RilsHostModuleDescriptor`
为每个可独立发布的模块建立稳定 fragment 边界。函数 ID 应通过
`RilsHostStableId.FromCanonicalName` 从规范化 managed member identity 生成，不能依赖反射顺序或
`MetadataToken`。`RilsHostManifestBuilder.Build(module)` 直接调用原生规范编码器生成单模块
`.rilhm`，不安装 dispatcher，也不需要创建假的宿主运行时对象；Player 再通过
`new RilsHostFunction(descriptor, handler)` 绑定真实实现。

`RilsHostTypeDescriptor` 声明 Host Manifest v4 命名类型；opaque 对象可以声明基类，
`InlineValue(name, layout)` 声明固定布局值类型。
`RilsHostParameter.NamedHandle("path::Type")` 在函数签名中引用该逻辑类型，并明确使用
`HostHandle` 作为 ABI transport；`NamedValue` 使用 `InlineValue` transport。`RilsHostManifestBuilder` 会先注册类型再注册函数。Player 侧也应
先调用 `RilsHostRegistry.Register(type)`，或先加载包含类型表的 manifest，再绑定 handler。
enum、常量和其他 value layout 尚未实现，不应在 C# 或 Unity 侧另建不兼容格式。

## 导出到 Unity

从仓库根目录运行：

```console
python tools/export-unity-package.py
```

默认会直接更新独立的 `integrations/RilsForUnity` 工程，将文件写入
`Packages/com.rils-lang.rils-for-unity/Runtime/Rils.CSharp`。其中 C# 源码位于该目录根部，
`Internal/x86_64/rils_capi.dll` 是当前 Windows x86_64 原生运行库，asmdef 已启用 unsafe 代码。
导出会保留已有 Unity `.meta` 文件，避免重复导出导致资源 GUID 变化。也可以直接指定目标：

```console
python tools/export-unity-package.py --output D:/Game/Assets/Rils.CSharp
```

导出会替换完整目标目录；Unity 业务代码应放在该目录之外。
