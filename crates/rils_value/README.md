# rils_value

`rils_value` 提供类型擦除的原生 Rust 值存储和操作描述。它只依赖 `rils_syntax` 中的共享 `Type`，并用泛型参数表示上层执行值，因此不依赖解释器、字节码 VM 或 `rils_execution`。

`NativeType<V>` 描述 Rust 类型身份、Rils 类型和可注册的 Copy 保证。类型可通过 `register_method` 按任意名称登记操作，处理函数从 `NativeCallContext` 读取类型检查过的 receiver 与参数，也能构造同类型返回对象；存储层不预设 Clone、Equal、Display 或 Next。调用者负责把这些操作与 Rils 导出声明绑定。

`NativeObject<V>` 使用尺寸和对齐信息保存具体 Rust 布局。已登记 Copy 且不超过 32 字节、16 字节对齐的值直接放在对象内，不需要逐值分配；其余值使用共享堆存储，以支持词法引用和可变 receiver。擦除布局的分配与复制集中在 `storage.rs`，组合布局的移动与析构集中在 `dynamic.rs`；这些模块显式标注 `unsafe` 前提，不使用 `dyn Any` 作为负载。内联 Copy 值的可变方法需要上层 place 写回，不能通过克隆出的 handle 修改。包含 Rils 值的容器必须通过 `NativeChildren<V>` 报告嵌套值、活动引用和部分 move；Rils 的所有权与词法引用仍由执行层负责。

`Range<T>`、`i8`、`i32`、`usize` 和 `string` 已在解释器与 VM 的实际值路径中使用 `Value::Native`。类型描述和整数、字符串方法注册由标准库声明生成；手工构造 `NativeType` 仅用于验证独立存储 API。

`DynamicLayout` 另提供运行时组合的泛型布局。`Option<T>` 使用显式标记和按 `T` 对齐的负载，可嵌套、移动、复制 Copy 负载并正确析构非 Copy 负载；`None` 只需标记字节。`DynamicObject<V>` 按类型 Copy 性质选择内联句柄或共享存储；`DynamicType<V>` 为它提供任意操作的注册表和带布局检查的调用上下文。执行层已有从标准库声明过程宏生成的整数、`string` 与 `Option<T>` 布局工厂。`Option<i8>`、`Option<i32>`、`Option<usize>` 和 `Option<string>` 的 `Some` 与可推导出具体元素类型的 `None` 已接入实际 `Value::Dynamic` 路径；方法仍通过过渡适配器转换，其余泛型实例尚未迁移。
