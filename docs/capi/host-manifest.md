# Host Manifest v4

Host Manifest 是编译器、Analyzer、字节码 imports、C ABI dispatcher 和 Unity 绑定生成器之间的
宿主契约交换格式。它描述逻辑类型、函数声明和调用策略，不包含函数地址、托管对象引用或运行时
授权结果。

运行时规范格式是紧凑的 `.rilhm` 二进制文件。JSON 仅作为显式的 Editor/工具输入和诊断输出；
Runtime 和 Unity Player 的默认注册、查询及导出接口不生成或解析 JSON。

v3 在 v2 命名对象和单继承的基础上增加字段化 inline value transport；v4 允许同一完整函数名
声明多个不同参数签名的 overload。opaque 对象使用
`HostHandle`；值类型使用 16 字节 `InlineValue` payload，并以 `fields(...)` 声明按顺序紧密打包的
规范标量字段。Rils 和 Analyzer 始终看见精确逻辑类型，ABI 边界不暴露 Rust、C# 或 Unity struct
的内存布局。旧的 `f32x2`、`f32x3`、`f32x4` 输入仍可读取并规范化为字段序列；enum 和常量尚未
进入本格式。

## 二进制布局

所有整数使用小端编码。格式不序列化 Rust struct、enum 或内存布局。文件由固定 64 字节头和一个
payload 组成：

| 偏移 | 大小 | 字段 |
|---:|---:|---|
| 0 | 8 | magic：`RILHOST\0` |
| 8 | 4 | binary format version，当前为 4 |
| 12 | 4 | header size，固定为 64 |
| 16 | 4 | host contract ABI version |
| 20 | 4 | contract version |
| 24 | 4 | module count |
| 28 | 4 | function count |
| 32 | 4 | string count |
| 36 | 4 | parameter type-reference count |
| 40 | 4 | payload byte length |
| 44 | 4 | hash algorithm ID，FNV-1a-128 为 1 |
| 48 | 16 | 128 位契约哈希，小端 |

哈希覆盖头部 `[0, 48)` 和完整 payload，不覆盖哈希字段本身。它用于损坏检测、增量构建和契约
一致性判断，不是密码学签名。

payload 依次包含：

1. 字符串表：每项为 `u32 byte_length + UTF-8 bytes`，按 UTF-8 字节序严格递增且不得重复；
2. `u32 type_count`；
3. 类型表：每项固定 12 字节，按完整类型名排序；
4. 模块表：每项为 `u32 name_string_index + u32 module_version`，按模块名排序；
5. 函数表：每项固定 36 字节，按“完整函数名 + 参数类型列表”排序；
6. 参数类型引用表：每项一个 `u32`，函数记录通过连续区间引用。

类型记录布局：

| 大小 | 字段 |
|---:|---|
| 4 | full-name string index |
| 4 | relation string index：opaque 为 base type，value 为 layout；无关系时为 `u32::MAX` |
| 1 | transport tag：`HostHandle` 为 9，`InlineValue` 为 10 |
| 1 | kind：opaque type 为 0，inline value type 为 1 |
| 2 | reserved，必须为 0 |

opaque 类型支持单继承。基类必须在同一合并契约中声明，不能自继承或形成循环。派生类型可赋值给
基类参数，也会继承基类声明的 receiver 方法；反向赋值不成立。inline value 不允许继承，relation
必须指向字符串表中的规范字段布局，例如 `fields(f32,f32,f32)`。字段类型支持
`bool`、`i8/i16/i32/i64/i128`、`u8/u16/u32/u64/u128`、`f32/f64`；紧密打包后的总长度不能超过
16 字节。

函数记录布局：

| 大小 | 字段 |
|---:|---|
| 8 | stable function ID |
| 4 | full-name string index |
| 4 | module table index |
| 4 | capability string index |
| 4 | parameter start |
| 4 | parameter count |
| 4 | return type reference |
| 1 | call kind；Direct 为 0 |
| 1 | thread affinity；MainThread 为 0 |
| 1 | receiver：无 receiver 为 0，`self`/`&self`/`&mut self` 分别为 1/2/3 |
| 1 | reserved，必须为 0 |

primitive type reference 为：

```text
0=()  1=bool  2=i32  3=u32  4=i64  5=u64  6=f32  7=f64
8=string  9=HostHandle
```

`()` 只能作为返回类型。命名类型引用为 `0x80000000 | type_index`，其中索引指向按名称排序的
类型表。引用、`isize/usize`、`char`、数组、Option/Result、Rils struct/enum 仍不能作为宿主函数
契约签名。C dispatcher 继续拒绝未声明稳定 transport 的值。

v4 以“完整名称 + 参数类型列表”标识一个 overload。相同名称可以重复，但映射后的参数类型列表
必须不同；仅返回类型不同不能构成 overload。每个候选仍有独立且全局唯一的 stable function ID。
编译器按参数数量、精确类型和命名宿主类型的继承距离静态选择候选；不使用返回类型，也不执行
隐式数值转换。无法唯一选择时编译失败并列出候选，运行时不会按声明顺序猜测。

Verifier 在分配和建模过程中检查 magic、版本、头大小、总长度、哈希、计数上限、UTF-8、排序、
重复项、全部索引、模块归属、参数区间、类型引用、继承图、调用策略、保留位和未使用字符串。外部
二进制输入不能绕过 verifier。

## 版本与兼容

- binary format version 描述 `.rilhm` 的物理编码；
- JSON format version 描述可选工具 schema，两者独立演进；
- host contract ABI version 描述契约中的通用值协议，必须与 Runtime 匹配；
- C API 的 `RILS_ABI_VERSION` 描述导出函数/结构体 ABI；inline type 注册对应 C ABI version 5；
- contract version 由绑定拥有者维护，表达项目宿主 API 的发布代次；
- module version 描述单个宿主模块的契约代次。

Runtime、CLI 和 Analyzer 仍可读取二进制及 JSON Host Manifest v1/v2/v3。v1 内容加载后会被建模为
无命名类型的兼容契约，v2 保留 opaque 类型语义，v3 保留 inline value 语义；再次导出或链接时统一写为 v4。读取未来版本会
明确失败，不会猜测布局。

当前限制为二进制 manifest 最大 256 MiB、JSON 工具输入最大 64 MiB、最多 65,536 个命名类型、
4,096 个模块、65,536 个函数和 1,048,576 个参数。类型路径、模块路径、完整函数名和 capability
也有长度限制。

## Rust 与 C ABI

Rust 默认读写二进制：

```rust
let contract = rils::HostContract::from_manifest_bytes(bytes)?;
let canonical = contract.to_manifest_bytes()?; // 始终写 v4
let hash = contract.contract_hash();
let module = rils::compile_with_host(source, &contract)?;
```

C ABI 的下列接口读写 `.rilhm` 二进制数据：

```text
rils_runtime_register_host_manifest
rils_runtime_host_manifest_size
rils_runtime_write_host_manifest
```

直接声明 v4 命名类型和 overload 时，先调用 `rils_runtime_register_host_types_v2`，再调用
`rils_runtime_register_host_functions_v2`。`RilsHostParameter` 把逻辑类型名与 transport tag 分开；
例如逻辑返回类型可为 `unity_engine::GameObject`，transport 仍为 `RILS_VALUE_HOST_HANDLE`。旧的
`rils_runtime_register_host_types` 保留给只有 opaque handle 类型的 v2 调用方；
`rils_runtime_register_host_functions` 保留给只使用 primitive/裸 `HostHandle` 的 v1 风格调用方。

`RilsHostTypeV2` 用 `kind`、`transport_tag` 和 `value_layout` 明确区分 opaque 与 inline value。
dispatcher 中 `RILS_VALUE_INLINE_VALUE` 的 `low/high` 合计为 16 字节；整数和 IEEE-754 浮点字段
按声明顺序使用小端规范编码，未使用的尾部字节必须为零。调用方不能 `memcpy` Unity 或托管
struct，并且必须同时携带逻辑类型名，Runtime 才能按 manifest layout 验证 payload。C# facade
提供无分配的 `RilsInlineValueWriter` / `RilsInlineValueReader` 执行字段级编码。

C ABI version 5 的 `rils_script_value_call_trait` 接收与参数数组等长的 `RilsHostParameter` 数组。
托管调用方必须为 primitive 填写 transport tag，为命名对象同时填写逻辑类型名；Runtime 据此把
opaque handle 恢复为可参与继承检查的命名宿主值。C# facade 的 `CallTraitTyped` 和
`RilsHostArgument.NamedHandle` 封装了这一过程。

注册会复制并验证输入；导出采用“查询长度 + 写入调用方 buffer”。随后仍需设置统一 dispatcher、
单独授权 capability 并冻结注册表。Manifest 中声明 capability 不等于授予 capability。

## Fragment 目录与链接

开发期推荐将 Host Contract 拆成多个 fragment：

```text
.rils/manifest/
├─ unity-engine/core.rilhm
├─ unity-engine/physics.rilhm
└─ project/game.rilhm
```

Analyzer 与源码编译入口递归读取所有 `.rilhm`，按规范化相对路径排序后确定性合并。相同的类型、
模块和函数声明可幂等去重；ABI/contract/module 版本不一致、类型基类或 transport 冲突、同名函数
的映射参数签名重复、不同函数复用同一 ID，或合并后继承图非法，都会使链接失败。同名但参数签名
不同的函数会合并成一个 overload set。

```text
rils host-manifest link .rils/manifest -o Library/Rils/host.rilhm
# 也可以传入项目根目录或 rils.toml，使用其中的 host 配置
```

链接结果是 `.rilhm` v4，不携带 fragment 路径或生成器来源，整体 contract hash 只由规范化内容决定。
C API 也允许在冻结前重复注册兼容 fragment；Player 通常消费链接后的单一产物。

## 显式 JSON 工具

JSON v4 的 `types` 数组每项包含 `name`、`kind`、`transport`，opaque 可带 `base`，value 必须带
`layout`。函数参数与返回类型直接使用逻辑类型名；命名类型必须在 `types` 中声明。示意结构：

```json
{
  "format_version": 4,
  "types": [
    { "name": "unity_engine::Object", "kind": "opaque", "transport": "HostHandle" },
    {
      "name": "unity_engine::GameObject",
      "kind": "opaque",
      "base": "unity_engine::Object",
      "transport": "HostHandle"
    },
    {
      "name": "unity_engine::Vector3",
      "kind": "value",
      "transport": "InlineValue",
      "layout": "fields(f32,f32,f32)"
    }
  ]
}
```

仓库保留严格 JSON schema，方便人工编辑和绑定生成。它不会由默认运行时接口生成。使用 CLI 显式
转换：

```text
rils host-manifest compile examples/unity-host-manifest.json -o unity-host.rilhm
rils host-manifest export-json unity-host.rilhm -o unity-host.json
```

Rust 工具也可以显式调用 `HostContract::from_manifest_json` 和 `to_manifest_json`。JSON 中的
`contract_hash` 可省略；导出时会写入基于规范二进制内容计算的哈希。函数 ID 继续使用 `0x` 前缀的
64 位十六进制字符串，避免 JavaScript 数字精度损失。

VS Code 插件通过 `rils.hostManifest.path` 加载单一 `.rilhm`。未配置时，Analyzer 优先读取
`rils.toml` 的 `[host].manifests` / `manifest_dirs` / 兼容 `manifest`；没有显式配置时递归读取
`.rils/manifest`。Analyzer 使用与 Runtime 相同的 verifier，将命名类型加入静态分析，并依据
receiver 的实际类型和继承链提供宿主方法补全。

## 大规模基准

从仓库根目录运行：

```text
cargo run --release --example host_manifest_bench
```

基准对比二进制与显式 JSON 的体积、生成和解析耗时。基准数据依赖机器，应以目标 Unity
Editor/Player 平台上的结果作为发布判断依据。
