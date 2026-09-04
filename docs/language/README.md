# Rils 语言手册

本手册描述 Rils `0.4.0` 当前已经实现的语法与语义。示例默认可以由解释器执行；属于当前字节码
能力矩阵的语法也应在 VM 中得到相同结果。

Rils 借用 Rust 风格的表达式、所有权、trait 和模块结构，但面向脚本与宿主嵌入场景，不机械复制
Rust 的生命周期和唯一可变借用限制。

## 目录

1. [值与类型](01-values-and-types.md)
2. [变量、作用域与所有权](02-bindings-and-ownership.md)
3. [表达式与控制流](03-expressions-and-control-flow.md)
4. [函数与闭包](04-functions-and-closures.md)
5. [Struct、Enum 与集合](05-data-types-and-collections.md)
6. [Impl、泛型与 Trait](06-impl-generics-and-traits.md)
7. [模式匹配与宏](07-patterns-and-macros.md)
8. [标准能力、模块与 IO](08-modules-and-standard-library.md)
9. [语法摘要](09-grammar-and-roadmap.md)

## 核心语义

- 非 Copy 值默认 move，复制非 Copy 值必须显式 Clone。
- `&T` 和 `&mut T` 是词法局部引用；允许同一 place 同时存在多个 `&mut T`。
- 引用不能从函数返回、被闭包捕获或存入拥有型复合值。
- struct/enum 字段持有值的所有权，不能直接保存局部引用。
- 固有方法优先于 trait 方法；多个 trait 的同名候选会产生歧义。
- 模块级 item 与块内闭包函数有明确边界。

## 文档状态

语言手册属于仓库开发文档，暂不包含在 crate 发布包中。项目模型、Analyzer 和字节码内部设计分别记录在
[project](../project.md)、[analyzer](../analyzer.md) 与 [bytecode](../bytecode.md)；它们不是
语言规范的一部分。
