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

当前 host-handle 值扩展使用 C ABI version 2。旧版原生库必须先升级，才能注册或执行 `HostHandle` 参数。第一版先覆盖标量、对象句柄，再逐步接入 `GameObject`、`Component`、`Transform` 等 Unity API。

## Unity 生命周期资产

Unity 场景中的 `RilsBehaviour` 组件引用 `.rils` 主资产下生成的 `RilsEntryAsset`
子资产，不需要在 C# 脚本中硬编码源码。入口由实现同名 trait 的类型声明：

```rils
pub struct PlayerBehaviour { }

impl RilsBehaviour for PlayerBehaviour {
    fn awake(&mut self, host: HostHandle) { }
    fn start(&mut self, host: HostHandle) { }
    fn update(&mut self, host: HostHandle, delta_seconds: f32) { }
    fn on_destroy(&mut self, host: HostHandle) { }
}
```

方法参数中的 `HostHandle` 是当前 GameObject 的 session 绑定句柄，不拥有或
暴露 Unity 对象本身。当前第一版已经建立主资产/入口子资产边界；按具体 entry
构造并持久保存脚本状态、以 trait 方法身份精确调用生命周期函数仍需要字节码元数据
与 instance 模型支持，不能仅用类型名字符串替代。

## Unity 宿主 manifest

Unity 集成使用项目根目录下的 `.rils/manifest/` 作为生成的二进制宿主契约目录，
不把 manifest 放进 `Assets`，也不把它作为 Unity 资源提交。Editor 启动时会把现有
`unity.object.rilhm` 与当前 Unity 宿主绑定生成的内容比较；文件缺失、损坏或已经过期时会
原子重建并自动重新导入 `.rils` 资产。菜单 `Rils > Generate Unity Host Manifest` 仍可用于
显式强制重建。导入器会用该 manifest 校验带有
`unity::object::*` 调用的脚本，并把 manifest 字节保存到对应的
`RilsScriptAsset` 主资产。其 `RilsEntryAsset` 子资产共享这些数据，不重复存储。多个 fragment 会按路径排序后合并。这样 Player 运行时只依赖导入资产，不需要访问
工程根目录的 `.rils` 文件；`.rils/manifest/*.rilhm` 作为动态生成物由集成项目的
局部 `.gitignore` 忽略。
