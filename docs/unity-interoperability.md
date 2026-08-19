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

每个宿主函数声明参数/返回值类型、传输模式（`Copy`、`Clone`、`Handle`）、capability 和线程策略。当前 C# facade 已提供这些描述模型；底层 ABI 仍只承载标量 tag，复杂值和错误缓冲区将在后续 ABI 版本增加。

## 线程与错误

Unity API 默认要求主线程。跨线程调用必须返回明确错误，不能在底层隐式阻塞切换线程。宿主错误使用稳定错误码和消息模型，当前原生 ABI 暂统一映射为执行错误。

当前 opaque script value 与 trait 调用扩展使用 C ABI version 3。旧版原生库必须先升级，才能发现 trait entry、构造持久脚本状态并调用其方法。第一版先覆盖标量、对象句柄，再逐步接入 `GameObject`、`Component`、`Transform` 等 Unity API。

## Unity 生命周期资产

Unity 场景中的 `RilsBehaviour` 组件引用 `.rils` 主资产下生成的 `RilsEntryAsset`
子资产，不需要在 C# 脚本中硬编码源码。入口由实现同名 trait 的类型声明：

```rils
#[derive(Default)]
pub struct PlayerBehaviour { }

impl RilsBehaviour for PlayerBehaviour {
    fn awake(&mut self, host: HostHandle) { }
    fn start(&mut self, host: HostHandle) { }
    fn update(&mut self, host: HostHandle, delta_seconds: f32) { }
    fn on_destroy(&mut self, host: HostHandle) { }
}
```

方法参数中的 `HostHandle` 是当前 GameObject 的 session 绑定句柄，不拥有或
暴露 Unity 对象本身。导入器从经过 verifier 校验的字节码 trait implementation 表生成 entry 子资产，
不再扫描源码；项目级 module 包含多个脚本时，只为当前导入源文件声明的实现创建子资产。运行时按
`EntryId` 调用 `Default::default()` 构造一个 opaque script value，并在组件存活
期间持久保存；`awake/start/update/on_destroy` 均以 `RilsBehaviour` trait 方法身份分发到该值。

## Unity 宿主 manifest

Unity 集成使用项目根目录下的 `.rils/manifest/` 作为生成的二进制宿主契约目录，
不把 manifest 放进 `Assets`，也不把它作为 Unity 资源提交。Editor 与 Player 共享
`UnityEngineBindingCatalog` 中的模块 descriptor：Editor 从 descriptor 为
`unity_engine::object`、`game_object`、`component`、`transform` 和 `time` 分别生成
`.rils/manifest/unity-engine/*.rilhm`，Player 从同一 descriptor 绑定静态 C# handler。
生成 manifest 不执行 handler，也不创建假的 Unity 对象表。

Editor 启动时会逐模块比较当前内容；文件缺失、损坏、已经过期或属于旧模块集合时会原子同步并
自动重新导入 `.rils` 资产。菜单 `Rils > Generate Unity Host Manifest` 仍可用于显式强制重建。
导入器会用这些 fragment 校验带有 `unity_engine::*` 调用的脚本，并把合并后的 manifest 字节保存到对应的
`RilsScriptAsset` 主资产。其 `RilsEntryAsset` 子资产共享这些数据，不重复存储。多个 fragment 会按路径排序后合并。这样 Player 运行时只依赖导入资产，不需要访问
工程根目录的 `.rils` 文件；`.rils/manifest/*.rilhm` 作为动态生成物由集成项目的
局部 `.gitignore` 忽略。

Binding descriptor 已保留 `unity_engine::GameObject` 等逻辑类型名，但 Host Manifest v1 目前仍把
所有 Unity 对象参数降级为 `HostHandle`。命名宿主类型、继承和值类型 transport 需要独立的
manifest 格式升级，不能只在 Unity 绑定中私自扩展。
