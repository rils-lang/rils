# rils_value

`DynamicType::register_owned_operation` 可登记任意消费式操作。`DynamicObject::call_owned` 把唯一拥有的 bytes 和子负载交给该操作，共享的非 Copy 句柄会报错。操作可以保留构造处的类型声明上下文，跨模块转换无需依赖接收方的类型名称查找；存储层不限定操作种类。

`rils_value` 提供类型擦除的原生 Rust 值存储和操作描述。它只依赖 `rils_syntax` 中的共享 `Type`，并用泛型参数表示上层执行值，因此不依赖解释器、字节码 VM 或 `rils_execution`。

`NativeType<V>` 描述 Rust 类型身份、Rils 类型和可注册的 Copy 保证。类型可通过 `register_method` 按任意名称登记操作，处理函数从 `NativeCallContext` 读取类型检查过的 receiver 与参数，也能构造同类型返回对象；存储层不预设 Clone、Equal、Display 或 Next。调用者负责把这些操作与 Rils 导出声明绑定。

`NativeObject<V>` 使用尺寸和对齐信息保存具体 Rust 布局。已登记 Copy 且不超过 32 字节、16 字节对齐的值直接放在对象内，不需要逐值分配；其余值使用共享堆存储，以支持词法引用和可变 receiver。擦除布局的分配与复制集中在 `storage.rs`，组合布局的移动与析构集中在 `dynamic.rs`；这些模块显式标注 `unsafe` 前提，不使用 `dyn Any` 作为负载。内联 Copy 值的可变方法需要上层 place 写回，不能通过克隆出的 handle 修改。包含 Rils 值的容器必须通过 `NativeChildren<V>` 报告嵌套值、活动引用和部分 move；Rils 的所有权与词法引用仍由执行层负责。

`Range<T>`、全部标准库整数宽度、`f32`、`f64` 和 `string` 已在解释器与 VM 的实际值路径中使用 `Value::Native`。类型描述和数值、字符串方法注册由标准库声明生成；手工构造 `NativeType` 仅用于验证独立存储 API。

`DynamicLayout` 另提供运行时组合的泛型布局。`Option<T>` 使用显式标记和按 `T` 对齐的负载，可嵌套、移动、复制 Copy 负载并正确析构非 Copy 负载；`None` 只需标记字节。用户结构体可由字段布局递归计算对齐与偏移，字段初始化标记与负载放在同一块存储中；按索引访问、移出、重新填入和析构支持嵌套结构及 `Option<T>`。字段名到索引的映射保存在布局描述中，操作注册的上下文可通过类型检查后的字段入口访问负载。`DynamicObject<V>` 按类型 Copy 性质选择内联句柄或共享存储；`DynamicType<V>` 为它提供任意操作的注册表和带布局检查的调用上下文。`NativeObject::into_rust` 和 `DynamicValue::into_rust` 可以消耗已验证类型的原生负载，避免隐式 Clone 或重复析构。执行层已有从标准库声明过程宏生成的整数、浮点数、`string` 与 `Option<T>` 布局工厂，也能从已有的 `StructType` 声明为具体泛型实例解析嵌套布局。独立用户结构体实例尚未迁出 `Value::Struct`；容器中的原生用户记录已支持沿布局路径进行嵌套引用与写回，所有标准库整数、`f32`、`f64` 和 `string` 的 `Option<T>` 已接入实际 `Value::Dynamic` 路径。

`DynamicValue::view_path` 以经过校验的路径借用布局与 Rust 叶子，不暴露裸指针。`reference_path` 返回词法租约，并为路径经过的序列元素保留结构稳定性；多个可变租约可指向同一位置。`replace_path_reference` 直接替换字段或序列元素并返回旧负载，若父字段仍有后代引用则拒绝；字段 move 也拒绝与活动租约重叠。路径账本在首次引用时才分配，Copy 产生新账本，租约不保留任何 Rust 借用。

`DynamicLayout::copy_of<T: Copy>` 登记普通字节复制；`copy_handle_of<T: Clone>` 登记 Rils 的 Copy 身份句柄，复制时调用所注册 Rust 类型的 Clone 以保留所有者，不隐式复制其所指负载。Option、record 和 variant 递归执行子布局的复制操作，普通 Copy 组合保留字节复制快路径；复制失败或 panic 会清理已经完成的子负载，部分 move 的字段保留空标记。`DynamicValueRef::any_leaf` 只遍历存活的 Rust 叶子，跳过未选中分支与已移出字段，供执行层检查原生存储中的词法引用来源。

执行层的 `runtime_layouts` 为引用和调用目标登记叶子布局及消费式转换，bytes 中保存租约或调用目标句柄；`HostLayoutProvider` 从传入的宿主声明登记其布局与 Copy 策略，不把未知名称当作宿主类型。宿主声明现由执行层的 `NativeOwnedContext` 自动传入组合存储与原生构造，弱声明表按完整路径收集宿主类型，VM 与 C API 从共享 Host Contract 建立上下文。Host enum 的变体由同一声明投影生成，包含 raw flags 分支；组合字段保留不透明对象身份，借用读取不改变 Copy 策略。独立用户实例的构造仍在迁移中。
