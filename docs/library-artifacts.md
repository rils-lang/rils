# Rils 库产物

Rils 库项目可以导出为宿主无关的 `.rilslib` 文件。它不是操作系统动态库，也不包含平台机器码；
它是带有独立格式头、库身份和经过 verifier 校验的 Rils 字节码模块。

```console
rils library compile path/to/rils.toml -o output/library.rilslib
rils library verify output/library.rilslib
```

只有 `rils.toml` 判定为库项目的项目可以导出 `.rilslib`。库自身 `[lib].prelude` 会参与库编译；
源码依赖的 prelude 仍在编译期注入使用方。

## 第一版格式

第一版头部固定为 64 字节，包含：

- magic `RILSLIB\0`；
- 独立的库格式版本；
- Rils 语言版本；
- Host ABI 与目标指针宽度；
- UTF-8 库名长度和内嵌模块长度；
- 覆盖库名与模块 payload 的 CRC32；
- 用于缓存、去重和依赖匹配的 FNV-1a-128 内容哈希；
- 必须为零的 flags 和保留字段。

加载时会检查总长度、版本、ABI、指针宽度、UTF-8、库名、保留字段和 CRC，随后使用 `.rilbc`
解码器与 verifier 检查内嵌模块。文件上限当前为 64 MiB。

## 当前边界

当前提交先固定 `.rilslib` 的生成、验证和 Unity 资产导入边界。入口字节码仍沿用现有项目合并编译；
后续链接阶段需要把库的公开类型、trait、impl 和函数接口提取为独立声明表，并让 `.rilbc` 通过稳定
库身份和符号导入引用它们。在这一阶段完成前，`.rilslib` 不能替代源码依赖参与入口编译或执行。

Unity 开发期继续支持源码依赖。`.rilslib` 是可选的显式导出与独立分发形式，不要求包作者在每次
修改源码后手动生成二进制库。
