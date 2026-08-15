# Host Manifest v1

Host Manifest 是编译器、Analyzer、字节码 imports、C ABI dispatcher 和 Unity 绑定生成器之间的
宿主契约交换格式。它描述声明和调用策略，不包含函数地址、托管对象引用或运行时授权结果。

运行时规范格式是紧凑的 `.rilhm` 二进制文件。JSON 仅作为显式的 Editor/工具输入和诊断输出；
Runtime 和 Unity Player 的默认注册、查询及导出接口不生成或解析 JSON。

## 二进制布局

所有整数使用小端编码。格式不序列化 Rust struct、enum 或内存布局。文件由固定 64 字节头和一个
payload 组成：

| 偏移 | 大小 | 字段 |
|---:|---:|---|
| 0 | 8 | magic：`RILHOST\0` |
| 8 | 4 | binary format version，当前为 1 |
| 12 | 4 | header size，v1 固定为 64 |
| 16 | 4 | host ABI version |
| 20 | 4 | contract version |
| 24 | 4 | module count |
| 28 | 4 | function count |
| 32 | 4 | string count |
| 36 | 4 | parameter type count |
| 40 | 4 | payload byte length |
| 44 | 4 | hash algorithm ID，v1 的 FNV-1a-128 为 1 |
| 48 | 16 | 128 位契约哈希，小端 |

哈希覆盖头部 `[0, 48)` 和完整 payload，不覆盖哈希字段本身。它用于损坏检测、增量构建和契约
一致性判断，不是密码学签名。

payload 依次包含：

1. 字符串表：每项为 `u32 byte_length + UTF-8 bytes`，按 UTF-8 字节序严格递增且不得重复；
2. 模块表：每项为 `u32 name_string_index + u32 module_version`，按模块名排序；
3. 函数表：每项固定 32 字节，按完整函数名排序；
4. 参数类型表：每个参数一个 `u8` type tag，函数记录通过连续区间引用。

函数记录布局：

| 大小 | 字段 |
|---:|---|
| 8 | stable function ID |
| 4 | full-name string index |
| 4 | module table index |
| 4 | capability string index |
| 4 | parameter start |
| 4 | parameter count |
| 1 | return type tag |
| 1 | call kind：v1 的 Direct 为 0 |
| 1 | thread affinity：v1 的 MainThread 为 0 |
| 1 | reserved，必须为 0 |

类型 tag 为：

```text
0=()  1=bool  2=i32  3=u32  4=i64  5=u64  6=f32  7=f64  8=string
```

`()` 只能作为返回类型。引用、`isize/usize`、`char`、数组、Option/Result、用户 struct/enum 和对象
句柄不属于 v1。当前 C dispatcher 仍只接受除 `string` 外的标量，注册包含 string 的 manifest 会
返回 `RILS_STATUS_UNSUPPORTED_VALUE`。

Verifier 在分配和建模过程中检查 magic、版本、头大小、总长度、哈希、计数上限、UTF-8、排序、
重复项、全部索引、模块归属、参数区间、类型 tag、调用策略、保留位和未使用字符串。外部二进制输入
不能绕过 verifier。

## 版本与限制

- binary format version 描述 `.rilhm` 的物理编码；
- JSON format version 描述可选工具 schema，两者独立演进；
- host ABI version 描述通用值协议和 dispatcher ABI，必须与 Runtime 匹配；
- contract version 由绑定拥有者维护，表达项目宿主 API 的发布代次；
- module version 描述单个宿主模块的契约代次。

v1 限制为二进制 manifest 最大 256 MiB、JSON 工具输入最大 64 MiB、最多 4,096 个模块、65,536 个
函数和 1,048,576 个参数。模块路径、完整函数名和 capability 也有长度限制。

## Rust 与 C ABI

Rust 默认读写二进制：

```rust
let contract = rils::HostContract::from_manifest_bytes(bytes)?;
let canonical = contract.to_manifest_bytes()?;
let hash = contract.contract_hash();
let module = rils::compile_with_host(source, &contract)?;
```

C ABI 的下列接口也只读写 `.rilhm` 二进制数据：

```text
rils_runtime_register_host_manifest
rils_runtime_host_manifest_size
rils_runtime_write_host_manifest
```

注册会复制并验证输入；导出采用“查询长度 + 写入调用方 buffer”。随后仍需设置统一 dispatcher、
单独授权 capability 并冻结注册表。Manifest 中声明 capability 不等于授予 capability。

## Fragment 目录与链接

开发期推荐将 Host Contract 拆成多个 fragment：

```text
.rils/manifests/
├─ unity-engine/core.rilhm
├─ unity-engine/physics.rilhm
└─ project/game.rilhm
```

Analyzer 与源码编译入口递归读取所有 `.rilhm`，按规范化相对路径排序后确定性合并。合并规则与
文件遍历顺序无关：相同声明幂等去重；ABI/contract/module 版本不一致、同名函数声明不同或不同
函数复用同一 ID 都会报错，并指出当前 fragment 文件。

Unity Editor/构建管线应在 Player 打包前链接为一个运行时文件：

```text
rils host-manifest link .rils/manifests -o Library/Rils/host.rilhm
# 也可以传入项目根目录或 rils.toml，使用其中的 host 配置
```

链接结果仍是 `.rilhm` v1，不携带 fragment 路径或生成器来源，整体 contract hash 只由规范化合并
内容决定。Player 和 C API 默认消费这份单一产物。Unity 的稳定 API 选择器可以按模块生成
`unity-engine/*.rilhm`，项目 Attribute 扫描器单独更新 `project/*.rilhm`，避免每次重写整个大文件。

## 显式 JSON 工具

仓库保留严格 JSON schema，方便人工编辑和绑定生成。它不会由默认运行时接口生成。示例输入位于
[`examples/unity-host-manifest.json`](../../examples/unity-host-manifest.json)。使用 CLI 显式转换：

```text
rils host-manifest compile examples/unity-host-manifest.json -o unity-host.rilhm
rils host-manifest export-json unity-host.rilhm -o unity-host.json
```

Rust 工具也可以显式调用 `HostContract::from_manifest_json` 和 `to_manifest_json`。JSON 中的
`contract_hash` 可省略；导出时会写入基于规范二进制内容计算的哈希。函数 ID 继续使用 `0x` 前缀的
64 位十六进制字符串，避免 JavaScript 数字精度损失。

VS Code 插件通过 `rils.hostManifest.path` 加载单一 `.rilhm`。未配置时，Analyzer 优先读取
`rils.toml` 的 `[host].manifests` / `manifest_dirs` / 兼容 `manifest`；没有显式配置时递归读取
`.rils/manifests`，若目录不存在再依次检测项目根和各 `script_paths` 下的
`.rils/host.rilhm`、`host.rilhm`、`rils-host.rilhm`。Analyzer 使用与 Runtime 相同的 verifier，
并将宿主函数加入静态检查、hover、语义符号及 `module::` 补全。

## 大规模基准

从仓库根目录运行：

```text
cargo run --release --example host_manifest_bench
```

基准默认覆盖 10,000、20,000、50,000 和 65,536 个函数，对比二进制与显式 JSON 的体积、生成和
解析耗时。基准数据依赖机器，应以目标 Unity Editor/Player 平台上的结果作为发布判断依据。
