# Unity 互操作边界（第一版）

Rils 与 Unity 的主要调用方向是 **Rils → C# facade → Unity API**。C# → Rils 只提供启动、生命周期和少量显式入口，不作为主要执行模型。

## 值传递

- 实现 `Copy` 的值允许按值传递。
- 仅实现 `Clone` 的值必须显式深拷贝后传递。
- 两者都未实现的拥有型值不能传递给 C#。
- 容器的 `Copy`/`Clone` 能力由元素递归决定。
- 字符串和字节数据使用拥有型复制，不跨边界暴露 Rust/C# 内存布局。

## Unity 对象

`GameObject`、`Component`、资源等使用 session 绑定的 opaque handle，不传递指针，也不转移 Unity 对象所有权。句柄包含 runtime session、对象身份、generation 和类型身份；Domain Reload、Play Mode 重启或对象销毁后必须失效。

## 宿主函数声明

每个宿主函数声明参数/返回值的逻辑类型、ABI transport、capability 和线程策略。C# facade 可将命名
Unity 对象以 `HostHandle` 跨越 ABI；符合规则且规范编码不超过 16 字节的 struct 使用字段化
`InlineValue` 传递。`Vector2`、`Vector3`、`Quaternion`、`Color` 都由公开实例字段自动得到
`fields(f32,...)` 布局，而不是按类型名特判。字符串、集合和结构化错误缓冲区仍需后续 ABI 扩展。

## 线程与错误

Unity API 默认要求主线程。跨线程调用必须返回明确错误，不能在底层隐式阻塞切换线程。宿主错误使用稳定错误码和消息模型，当前原生 ABI 暂统一映射为执行错误。

当前 inline type 注册使用 C ABI version 5 和 Host Manifest v4。原生库、生成的 P/Invoke 与 C# facade
必须成套更新；version 4 只支持 opaque 类型表。opaque script value 与 trait 调用接口继续保持兼容。

## Unity 生命周期资产

Unity 场景中的 `RilsBehaviour` 组件引用 `.rils` 主资产下生成的 `RilsEntryAsset`
子资产，不需要在 C# 脚本中硬编码源码。入口由实现同名 trait 的类型声明：

```rils
#[derive(Default)]
pub struct PlayerBehaviour { }

impl RilsBehaviour for PlayerBehaviour {
    fn awake(&mut self, host: unity_engine::GameObject) { }
    fn start(&mut self, host: unity_engine::GameObject) { }
    fn update(&mut self, host: unity_engine::GameObject, delta_seconds: f32) { }
    fn on_destroy(&mut self, host: unity_engine::GameObject) { }
}
```

方法参数在 Rils 中保留 `unity_engine::GameObject` 逻辑类型，在 C ABI 上仍使用 session 绑定的
`HostHandle` transport，不拥有或暴露 Unity 对象本身。导入器从经过 verifier 校验的字节码 trait implementation 表生成 entry 子资产，
不再扫描源码；项目级 module 包含多个脚本时，只为当前导入源文件声明的实现创建子资产。运行时按
`EntryId` 调用 `Default::default()` 构造一个 opaque script value，并在组件存活
期间持久保存；`awake/start/update/on_destroy` 均以 `RilsBehaviour` trait 方法身份分发到该值。

## Unity 宿主 manifest

Unity 集成使用项目根目录下的 `.rils/manifest/` 作为生成的二进制宿主契约目录，
不把 manifest 放进 `Assets`，也不把它作为 Unity 资源提交。Editor 扫描 `rils.toml` 指定的程序集
并建立统一 Binding IR，再从同一 IR 生成 `.rils/manifest/unity/*.rilhm` 和
`Assets/RilsGenerated/Bindings/*.g.cs`。前者只供 Rils 编译器、Analyzer 和导入器使用，不经过
AssetDatabase；后者由 Unity 编译为 Mono/IL2CPP 可直接调用的静态 C# handler。
生成 manifest 不执行 handler，也不创建假的 Unity 对象表。

菜单 `Rils > Generate Unity Bindings` 会同步两类输出并清理各自拥有目录内的过期生成文件；
`Rils > Check Generated Unity Bindings` 只比较内容，不修改输出。仓库根目录也提供
`python tools/generate-unity-bindings.py [--project <UnityProject>] [--check]` 稳定入口。
导入器会用这些 fragment 校验带有 `unity_engine::*` 调用的脚本，并把合并后的 manifest 字节保存到对应的
`RilsScriptAsset` 主资产。其 `RilsEntryAsset` 子资产共享这些数据，不重复存储。多个 fragment 会按路径排序后合并。这样 Player 运行时只依赖导入资产，不需要访问
工程根目录的 `.rils` 文件；`.rils/manifest/*.rilhm` 作为动态生成物由集成项目的
局部 `.gitignore` 忽略。

生成的 Host Manifest v4 按程序集保存 fragment，声明扫描得到的所有可表达命名类型；例如：

```text
unity_engine::Object
├─ unity_engine::Component
│  ├─ unity_engine::Behaviour
│  │  └─ unity_engine::MonoBehaviour
│  └─ unity_engine::Transform
└─ unity_engine::GameObject

inline values:
├─ unity_engine::Vector2     fields(f32,f32)
├─ unity_engine::Vector3     fields(f32,f32,f32)
├─ unity_engine::Quaternion  fields(f32,f32,f32,f32)
└─ unity_engine::Color       fields(f32,f32,f32,f32)
```

Rils 源码、编译器和 Analyzer 保留这些逻辑类型，派生对象可以传给基类参数，也能调用基类 receiver
方法。对象在 dispatcher 边界降级为 `HostHandle`；值类型按小端 IEEE-754 分量显式打包到 16 字节，
不能直接复制 Unity 托管 struct 布局。生成 handler 使用 `RilsInlineValueReader/Writer` 逐字段转换，
或通过 `UnityObjectHandleTable` 解析对象，不在 Player 中反射调用 Unity API。

Unity 工程在 `rils.toml` 中只选择要扫描的程序集；namespace 自动映射为 snake_case Rils module：

```toml
[unity.bindings]
assemblies = ["UnityEngine.CoreModule", "UnityEngine.PhysicsModule"]
```

菜单 `Rils > Generate Unity Bindings` 扫描当前 Editor 实际加载的程序集，并将可传输类型、成员、
跳过原因和映射签名冲突写入 `.rils/generated/unity-bindings-report.json`。可表达的 C# overload 会保留
相同 Rils 名称和不同参数签名；`ref struct`、普通 managed
class、泛型调用、`ref/out/in` 参数、无法传输的字段及超过 payload 上限的 struct 都会明确进入报告。
不支持的类型、废弃 API、`ref/out/in`、开放泛型、超出 16 字节的值布局以及映射签名冲突都会写入
报告而不会生成不可编译的 handler。合法重载保留同一个 Rils 名称，由编译器按参数签名静态选择。
