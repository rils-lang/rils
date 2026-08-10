# Rils 实施路线

本文档记录 Rils 从语法验证器走向可用于实际脚本的实施顺序。各阶段以前一阶段建立的
语义基础为依赖，不以增加孤立语法为目标。

## 1. Trait 分派与限定路径

状态：已完成。

这是当前最高优先级。Trait 方法必须保留其来源，不能仅以方法名写入类型的公共方法表。

本阶段包括：

- Trait 方法按 `(Trait, Method)` 存储，固有方法继续独立存储。
- `value.method()` 优先选择固有方法；没有固有方法时，只允许唯一的 trait 方法候选。
- 多个 trait 提供同名方法时产生歧义诊断。
- 支持 `Trait::method(value, ...)`。
- 支持 `<Type as Trait>::method(value, ...)`。
- 支持 `<Type as Trait>::Associated`，消除同名关联类型歧义。
- 让内置 `Clone` bound 与 `.clone()` 方法保持一致。

泛型 trait、trait 对象、默认方法体、条件 impl 和 `where` 不属于本阶段。

## 2. Place、字段赋值与索引

状态：已完成。字段和数组/Vec 元素 place 已支持读取、赋值和局部借用，并共享活动引用检查。

为字段和容器元素建立统一的 place 语义：

```rust
value.field = next;
values[index] = next;
let shared = &values[index];
let writable = &mut values[index];
```

元素引用遵循当前局部引用规则：可以在局部作用域使用，但不能返回、捕获或存入拥有型值。
这一阶段是数组、Vec、HashMap 和字符串元素访问的共同基础。

## 3. Tuple、数组与 Vec

状态：已完成拥有型基础。

已实现 tuple、固定数组、列表/重复数组字面量、具体索引 place，以及 `Vec<T>` 的
`new`、`from`、`len`、`push` 和 `pop`。数组与 Vec 通过 `IntoIterator` 进入统一拥有型迭代器，
可直接用于 `for`。共享引用和可写引用迭代器保留为后续增强，不阻塞第 4 阶段。

## 4. 模块、Prelude 与宿主模块注册

状态：已完成基础层。

已实现 `mod`、`use`/`as`、`pub`、多段路径、内联/文件模块加载及循环检测，并建立
`core`、`std` 与 prelude 骨架。Analyzer 启动时扫描 workspace 的 `.rils` 文件，定义和引用请求
可以返回跨文件 URI。宿主 API 支持多层模块、有状态函数闭包，以及带 Rust payload 和实例方法的
原生类型句柄。

通配/分组导入、`crate`/`self`/`super`、原生属性与静态方法元数据属于后续增强。

建议模块结构：

```text
core::{clone, cmp, hash, iter, ops, option, result}
std::{collections, io, fs, path, env, time}
```

## 5. Result、错误传播与 IO

状态：基础层已完成。

已提供内置 `Result<T, E>`、`Ok`、`Err`、`Ok`/`Err` 模式、函数内的 `?`，以及
`is_ok`、`is_err`、`unwrap`、`unwrap_or` 的函数和方法形式。`?` 遇到 `Err` 时立即返回
当前函数，并由函数返回标注校验传播后的完整 Result 类型。

已建立 `std::io::Error`、`std::io::ErrorKind`，并提供返回结构化错误的控制台 IO 与文本文件
读写、追加、目录枚举、存在性查询、目录创建和删除接口。后续增强包括字节 IO、文件句柄、
metadata/permissions、`std::path`，以及可注入的宿主能力策略。

## 6. HashMap、HashSet 与比较/哈希 Trait

状态：待实现。

增加 `PartialEq`、`Eq`、`PartialOrd`、`Ord`、`Hash`、`Default`、`Display`、`Debug`、
`From` 和 `Into` 的核心子集，再实现 `HashMap<K, V>` 与 `HashSet<T>`。

## 7. 循环控制

状态：基础层已完成。

已实现 `loop`、`break`、`break value` 和 `continue`，并保证循环控制不能越过嵌套函数边界。
循环标签保留为后续增强。

## 8. 独立静态语义检查

状态：静态语义小闭环已完成，进入精度和增量性能优化。

名称解析、基础类型检查、trait/method 签名、控制流、match 穷尽性、所有权和引用逃逸检查已经
独立于解释执行，并由 CLI、字节码编译入口和 Analyzer 共用。已知函数与方法会检查参数数量、
参数类型和返回类型；`Self`、类型别名、泛型占位和限定类型路径参与推断。数值模型已切换为
`i8`～`i128`、`isize`、`u8`～`u128`、`usize`、`f32`、`f64` 与 `char`；无后缀数值字面量
接受标注、参数、运算和索引用法约束，未受约束时分别默认 `i32` 与 `f64`。

所有权分析跟踪局部与字段 place 的可用、move、部分 move 和重新初始化状态，检查不可变写入、
活动引用期间的 move、字段引用期间替换所有者，以及引用返回、跨块/循环逃逸或进入拥有型值。
多个 `&mut` 按 Rils 既定语义合法。`if`/match/loop 会合并可达路径，方法 receiver 按四种 self
形式执行消费或临时借用。

控制流分析会报告返回路径缺失和不可达语句，并对 `bool`、Option、Result 与用户 enum 检查
match 穷尽性及不可达 arm。不可达项作为 warning，确定的语义错误作为 error；结果排序去重后
交给 LSP。未知或开放类型保持保守，避免为不能证明的错误报警。

当前优化重点是循环回边固定点、动态索引 place 精度、跨文件语义模型缓存、合并重复 AST 遍历，
以及真实 workspace 的增量分析和性能基准。VS Code 客户端已使用 esbuild 单文件 bundle，发布包
不携带开发用 `node_modules`。平台专属 VSIX 会内置对应平台的 `rils-analyzer`，运行时仍允许通过
`rils.server.path` 覆盖；仓库开发模式会选择 workspace 中最新的 debug/release Analyzer 构建。
成员调用的语义 token 会区分 method 与普通 field，类型别名 hover 会递归展开泛型实参和别名链。

## 9. 字节码与预编译

状态：内存及多文件编译闭环已完成，正在补齐解释器语义覆盖。

已增加保留源码范围的 HIR、寄存器/基本块 MIR、线性字节码、加载前验证和独立 VM。当前覆盖
常量、局部槽位、move/Copy、基础运算、短路、块、条件与基础循环控制。第二轮已加入函数表、
参数、显式调用帧、直接调用、递归、`return`、tuple、数组/重复数组、Range 和局部集合索引。
编译模块可重复执行，可配置最大指令步数，并以 1024 帧上限约束无限递归。AST 解释器继续作为
完整语义实现和字节码迁移的对照基准。

第三轮已加入 Range/数组的 `for` 与迭代终结指令、`break value`/`continue`、局部集合索引和
tuple 字段写入，以及 Option/Result 构造和函数内 `?` 的调用帧级错误传播。

第四轮已加入原生 match 分支图、字面量与 Option/Result 模式、作用域模式绑定、运行时非穷尽
保护，以及静态检查后的类型别名和类型擦除泛型函数。

第五轮完成 VM 局部槽位的共享存储改造，加入局部/数组元素借用、引用参数、解引用读写、活动
引用计数和词法块局部清理，并保持 Rils 允许多个 `&mut` 的既定规则。

第六轮加入模块类型表、struct record 与 enum unit/tuple/record variant 构造、字段 move/Copy
读取、字段写入与借用，以及 struct/enum 模式解构。

第七轮完成 impl 方法登记、关联函数、struct/enum 成员调用、四种 self receiver、trait impl
身份和 UFCS 调用。方法复用普通字节码调用帧，泛型方法继续采用静态检查后的类型擦除策略。

第八轮完成内联模块符号收集和调用链接，支持嵌套模块、多段限定路径、`use ... as ...`、模块限定
的 struct/enum 构造与模式匹配。模块内未限定名称会优先在自身命名空间解析，避免同名符号串线；
模块内 impl、关联函数、成员方法和 UFCS 也使用限定链接名。公开的 `compile_file` 复用现有外部
模块加载规则，可将 `name.rils` / `name/mod.rils` 依赖编译进同一个内存字节码模块。

第九轮完成函数引用、按值间接调用和词法闭包。嵌套函数会提升到函数表，并携带显式共享槽位捕获；
可变捕获、返回函数、嵌套递归和高阶函数均复用 VM 调用帧与现有深度/步数限制。普通具名调用继续
使用直接调用指令，引用捕获由共享静态所有权检查拒绝。

第十轮将 place 统一为“局部根 + 字段/索引投影链”，补齐 struct、tuple、数组和 Vec 任意混合的
嵌套读取、赋值与局部借用。VM 借用深层元素时持有逐级父 place guard，确保外层值不能绕过活动
引用检查被替换；解释器同步使用相同的深层 place 语义，不再修改临时 Copy 副本。

第十一轮完成当前语法形态收尾：任意表达式都可作为 callee，UFCS 方法可作为函数值，Copy 的按值
receiver 可形成绑定方法值；临时值、嵌套字段和索引 place 可直接调用方法。引用根投影与显式 reborrow
进入 HIR/MIR/字节码，解释器与 VM 对 `&mut self` 字段写入和嵌套 receiver 保持一致。当前 AST
表达式、控制流及模块级声明已经全部有字节码路径；后续 P3 聚焦借用迭代器和运行时资源边界，
不再包含已完成的语法迁移。

第十二轮以仓库示例作为完整语法回归集，补齐 `print!`、`println!`、`assert!` 展开目标到 VM host
import 的映射，并统一 `std::io::print` / `std::io::println` 的前端签名与显式 capability。泛型 record
literal 会从字段反推类型实参，嵌套 Option/Result 的 match 穷尽性按递归有限域合并覆盖。除会实际
操作文件系统的示例外，其余示例现在同时执行解释器和 VM，并校验最终结果一致。

随着实现规模增长，字节码内部已拆分宿主 ABI、编码器、验证器、VM 和测试模块，HIR/MIR 也将 IR
定义与 lowering 实现分离；HIR 的符号发现和内建导入签名表进一步独立于函数 lowering。标准库签名
和宿主实现已提升为解释器与字节码共享的顶层模块，避免编译前端反向依赖解释器内部。`Type` 的
静态表示与合并逻辑也不再依赖 `Value`，值接受、约束和运行时类型反射通过 `RuntimeValue` 桥接。
词法、token、AST、宏展开、parser、源码范围和静态类型模型现已提取到统一版本的本地 crate
`rils_frontend`。类型推断、类型检查、控制流、所有权分析、公共诊断和标准库签名目录也已迁入；
Analyzer 现在直接依赖该 crate，主 crate 仅保留 `rils::analysis` 的公开兼容入口以及内部语法导入。
HIR、MIR 及两级 lowering 已进一步提取到统一版本的 `rils_compiler`，该 crate 提供源码/AST 到 MIR
的编译入口且只依赖 `rils_frontend`。字节码 encoder 目前仍留在主 crate，因为类型表编码阶段还会
构造运行时 `StructType`/`EnumType`；完成静态字节码类型描述后再迁移 encoder 与 verifier。

磁盘格式等模块依赖与宿主 ABI 稳定后再冻结，详细设计见 [bytecode.md](bytecode.md)。

## 10. 后续实施计划

以下顺序以“先完成可用脚本闭环，再优化已有实现”为原则。每一阶段都继续使用解释器与字节码
执行结果对照测试；除非语言语法或公开行为改变，否则不需要单独提升编辑器插件版本。

### P0：函数值、间接调用与闭包

状态：已完成。

- 在 VM 值模型中加入函数引用和闭包值，闭包由函数索引与捕获环境组成。
- HIR 提升嵌套函数并显式计算捕获列表；MIR/字节码增加创建闭包和按值调用指令。
- 非捕获顶层函数可以直接作为值传递、返回和存入局部变量。
- 捕获沿用解释器的共享词法存储语义，可变捕获对同一槽位可见；引用类型仍禁止捕获和逃逸。
- 验证器检查函数索引、参数数量、捕获槽位及调用目标，VM 调用栈和步数限制继续生效。

验收标准：高阶函数、返回函数、可变捕获、嵌套递归和闭包所有权错误都有解释器/VM 对照测试；
已有直接调用不因通用调用路径而明显退化。

### P1：标准库、宿主 ABI 与自定义迭代器

状态：基础链接层、Vec 和自定义迭代器已完成，HashMap/HashSet 与宿主注册整合继续推进。

当前已完成稳定名称、函数签名、ABI 版本和 capability 名称组成的内存导入表。`BytecodeHost` 在 VM
启动前检查 ABI、授权、缺失符号和签名不兼容；默认只链接确定性的 core/prelude 能力，`std::io`
和 `std::fs` 必须显式启用。首批 core 导入覆盖类型查询、Clone、Option/Result 查询与解包，以及
Vec 的 `new`、`from`、`len`、`push`、`pop`。Vec 已支持拥有型 `for`。

脚本 `Iterator`/`IntoIterator` impl 已进入模块迭代方法表。VM 通过普通脚本调用帧执行 `into_iter`
和 `next`，保留步数与调用深度预算，并在运行时验证 `next` 返回 Option。

- 为字节码模块增加导入符号表，区分脚本函数、标准库函数和宿主提供的能力。
- 通过稳定的名称、签名和 ABI 版本链接能力，不把 Rust 函数地址写进预编译文件。
- 先接入无状态且确定的 core/prelude 功能，再接 Vec、自定义 `Iterator`/`IntoIterator`，最后接
  `std::io`、`std::fs` 等需要显式授权的宿主能力。
- ABI 在加载时校验缺失符号、参数/返回类型和 capability policy；未授权 IO 在执行前失败。
- 同期完成 Hash、Eq 等核心 trait 的最小语义，并实现 HashMap/HashSet 基础接口。

验收标准：同一预编译模块可链接不同宿主能力表；缺失或签名不兼容的导入产生确定诊断；自定义
迭代器、Vec 和标准库 Result 错误路径均与解释器一致。

### P2：稳定 `.rilbc` 格式与工具链

状态：实验性 v1 容器、Rust bytes/file API、CLI compile/verify/run，以及 C ABI/C# 编译结果导出和
bytes/file 加载已完成；
跨版本冻结、多文件源码身份、可剥离调试 section 与 fuzz 测试待完成。

- 冻结独立的格式版本和语言版本，定义常量、字符串、类型、导入、函数、指令、源码映射及调试
  section；未知必需 section 拒绝，未知可选 section 跳过。
- 实现严格限长的编码器/加载器、校验和、索引验证和资源上限，禁止直接序列化 Rust enum 布局。
- CLI 增加源码编译为 `.rilbc`、验证预编译文件和直接执行预编译文件的入口。
- 保留 `compile`/`compile_file` 的内存 API，并增加从 bytes/file 加载的宿主 API。

当前 v1 使用显式小端编码和 section 目录，独立记录格式版本、语言版本、宿主 ABI 与指针宽度；
加载时检查 64 MiB 文件上限、目录边界/重叠、CRC32、字符串/集合/嵌套限制，并在构造模块后执行
完整 verifier。`usize`/`isize` 使 v1 产物暂按 32/64 位目标区分。

验收标准：往返编码结果一致，损坏、截断、版本不兼容及资源超限文件全部被拒绝；预编译文件不
依赖源码即可执行，并保留可选源码位置诊断。

### P3：语法覆盖收尾与运行时边界

状态：当前语法形态与能力矩阵已完成；借用迭代器和运行时预算待实现。

- 完成借用形式的容器迭代器；嵌套字段/索引 place、引用根投影、reborrow 和 Vec 动态元素借用已完成。
- 解释器与字节码能力矩阵已建立，当前 AST 表达式、控制流和模块级声明均已明确覆盖。
- 增加堆内存、容器长度、字符串长度、调用深度和宿主调用次数预算，适配游戏脚本沙箱。
- 改善多文件源码映射，使外部模块中的编译和运行错误指向正确文件，而不只保留局部 Span。

### P4：性能与分析器优化

- 基于基准数据依次实现常量折叠、无效代码删除、分支简化、寄存器复用和热点统计。
- 对直接调用保留快速指令，闭包/间接调用使用通用路径；基准分别覆盖冷启动、单帧短调用、递归、
  集合循环和宿主调用。
- Analyzer 完成循环回边固定点、动态索引 place 精度、跨文件语义缓存和重复 AST 遍历合并。
- 建立解释器、内存字节码和磁盘字节码的可重复性能基线，优化以数据为准。

### P5：可选 Rust AOT 后端

以 MIR 为共同输入生成等价 Rust，复用同一所有权和运行时 ABI，不另建一套语言语义。先生成便于
调试的 Rust 源码，再评估原生库链接、增量编译和缓存；AOT 是部署优化，不替代字节码的快速验证、
能力隔离和资源限制。

### Unity 宿主边界（并行推进）

状态：Windows C ABI 与独立 C# facade 原型已完成，Unity 托管层和实机验证待完成。

已新增统一版本的本地 crate `rils_capi`，以 `cdylib` 输出 Windows DLL。当前边界提供线程绑定、
带 generation 和类型标记的 runtime/module/instance 句柄，所有可失败入口捕获 panic；可从 UTF-8
内存源码编译模块，或从 bytes/file 加载 `.rilbc`，并重复调用无捕获的公开字节码函数。首版值协议
包含 unit、bool、具体整数、具体浮点和 char，
错误通过线程局部借用字符串返回源文件名和 Span。`BytecodeModule::call` 作为共享宿主入口，只暴露
`pub fn`，并继续执行 verifier、宿主链接、步数和调用深度限制。

集中式 `.NET Standard 2.1` C# P/Invoke facade 与真实 DLL 冒烟测试已经完成。下一步可在宿主项目中验证同一两数相加脚本，再测量空调用、
标量、字符串和批量数组成本。基准完成后扩展拥有明确所有权的字符串/缓冲区、宿主回调和有状态实例；
Linux `.so` 与 macOS `.dylib` 暂不进入当前构建矩阵。详细阶段见
[`unity/rils-for-unity-plan.md`](unity/rils-for-unity-plan.md)。

## 后置项目

完整生命周期、trait 对象、过程宏、卫生宏、async/await 和 JIT 暂不阻塞上述主干能力。
长时间嵌入场景还需要处理闭包环境的 `Rc` 环以及资源/内存限制。
