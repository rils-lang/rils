# 目录规范

- crates/ 下放置主项目依赖的本地包，这些包需要和主项目统一版本，包名必须以 `rils_` 开头
- examples/ 放各类测试用例
- editors/ 放各类编辑器插件
- tools/ 放各类辅助工具项目或脚本
- 新增辅助工具时优先使用 Python，并从仓库根目录提供稳定入口；不要再增加只适用于单个平台的临时 shell 脚本。
- 按职责拆分模块，`lib.rs`/`main.rs` 只保留模块声明、公共导出和薄入口；不要持续向单文件堆叠互不相关的声明、实现、协议编解码或工具处理逻辑。文件接近千行或已经同时承担多类职责时，应在继续扩展前拆成命名清晰的子模块，并用测试保证机械迁移不改变行为。

# Git 与提交规范

- 忽略规则应放在拥有生成物的最近目录中，例如 `crates/rils_capi/.gitignore` 管理该 crate 的 `dist/`、`bin/` 和 `obj/`；只有真正覆盖整个仓库的规则才修改根 `.gitignore`。
- 不提交本地构建产物、生成的 Unity 导出目录、NuGet/target 缓存或编辑器临时文件；需要提交的生成源码必须能由仓库脚本重现并通过 `--check` 校验。
- 不相关能力分成依赖顺序明确的提交。一个文件包含多个主题时使用 hunk 级暂存，不要为了方便把语言破坏性变更、字节码格式、C API 和 Unity 打包混在同一提交。
- 提交信息使用现有 Conventional Commits 风格，例如 `feat(bytecode): ...`；破坏性更新使用 `!`，并在提交正文写明 `BREAKING CHANGE:` 和迁移方式。
- 暂存或提交前检查未跟踪文件、生成物、solution/project 路径和绑定生成结果，不能提交引用已经不存在目录的工程文件。

# 版本规范

- 主 crate、主项目依赖的本地 crate、Analyzer 和编辑器插件以主项目版本为基线保持一致。
- 不要因为本地构建、重新打包或尚未发布的实现自动修改版本号。
- 任何 crate 或插件的版本号更新都必须先获得同意。
- 尚未正式发布的变化记录在根目录 `CHANGELOG.md` 的 `Unreleased` 中，不因写入 changelog 自动修改版本号。
- 正式发布时再把 `Unreleased` 内容移动到对应版本；已发布版本按 SemVer 从新到旧排列，例如 `0.2.1`、`0.2.0`、`0.1.0`。
- Changelog 重点记录用户可见能力、破坏性语义、宿主 ABI/磁盘格式变化和明确迁移步骤，不罗列纯内部重构或本地重新打包。

# 语言语义

- Rils 使用显式所有权：非 Copy 值默认 move，不采用默认引用或隐式深拷贝；复制非 Copy 值必须显式 Clone。
- `&T` 和 `&mut T` 是词法局部引用。允许同一 place 同时存在多个 `&mut T`，不要引入 Rust 式唯一可变借用检查。
- 当前不实现生命周期参数或引用生命周期提升。引用不能从函数返回、被闭包捕获，或存入 tuple、数组、Option、Result、struct、enum 等拥有型值。
- Struct/enum 字段默认持有 `T` 的所有权；不要允许字段直接保存有限生命周期的 `&T` / `&mut T`。
- 方法 receiver 使用 Rust 风格的 `self`、`mut self`、`&self`、`&mut self`，不要求写成 `self: Type`；`Self` 指代当前 impl/trait 的具体类型。
- Trait 方法必须保留 trait 身份。固有方法优先；多个 trait 的同名候选不能静默任选，应报告歧义或要求 UFCS。
- 新语法和语义应尽量保持 Rust 风格，但应服从脚本语言的易用性、快速验证和宿主嵌入需求，不机械复制 Rust 的限制。

# 标准库结构

- 标准库采用 Rust 风格的分层路径：语言无关的基础能力放在 `core`，需要宿主或平台能力的内容放在 `std`，常用名称由 prelude 引入。
- `rils_builtins` 是内建声明的唯一事实来源。内建类型、函数、成员、签名、文档、稳定 intrinsic ID 和实现后端不能在运行时、frontend、Analyzer 或 C API 中另建彼此独立的手写目录；新增能力应增加声明到实现、类型检查和编辑器可见性的自动一致性测试。
- IO 和文件系统分别通过 `std::io`、`std::fs` 等明确路径访问；可失败操作返回结构化 `Result<T, E>`，不要用隐式异常替代。
- 集合以数组、`Vec<T>`、`HashMap<K, V>`、`HashSet<T>` 等拥有型类型为基础，并通过 `Iterator` / `IntoIterator` 接入 `for`。
- 新增标准库模块时，同步维护运行时签名、类型推断、Analyzer 可见符号和语言文档，避免解释器与编辑器各自维护不一致的接口。
- 参考 Rust 基础类型 API 时必须适配 Rils 的引用约束；不能直接加入会把局部引用返回、存入 `Option`/容器或提升生命周期的方法。必要时提供 Copy、Clone 或消费式替代接口。

# 编译器与字节码

- 前端流水线保持 `lexer/parser -> static analysis -> HIR -> MIR -> bytecode -> verifier -> VM` 的分层，不要让 VM 直接依赖 AST。
- AST 解释器是字节码迁移期间的完整语义参考。新增字节码能力时，应增加同一源码在解释器和 VM 下结果一致的对照测试。
- 合法但尚未支持的字节码语法必须返回带源码范围的明确 `CompileError`，不能静默退回解释执行。
- 字节码加载或执行前必须经过验证；函数索引、寄存器、局部槽位、跳转、类型表和后续导入表都不能信任外部输入。
- `compile` 不进行隐式文件访问；`compile_file` 与 `Engine::eval_file` 使用一致的 `name.rils` / `name/mod.rils` 递归模块加载和循环检测规则。
- 直接调用保留快速路径；函数值、闭包和宿主调用使用通用调用路径时，不应无必要地降低现有直接调用性能。
- 稳定磁盘格式不得直接序列化 Rust enum 或内存布局。格式版本、语言版本和宿主 ABI 版本分开维护，并在冻结前完成严格限长与索引验证。
- `.rilbc` 的编译、内存导出、文件导出和加载必须复用同一编码器与 verifier；C API 不得维护另一套磁盘格式。

# C API、C# 与 Unity

- `rils_capi` 保持宿主无关，不直接依赖 Unity API。Unity 特化逻辑留在 C# 项目或 Unity 工程，原生层只提供稳定 C ABI。
- C ABI 不跨边界暴露 Rust enum、`String`、`Vec`、trait object 或 Rust 分配器所有权。可变长度输出优先使用“查询长度 + 写入调用方 buffer”或调用方明确提供的文件路径。
- 核心 C ABI 的能力面以 Rils 运行时和宿主扩展模型为准，不能因当前 C#、Unity 或单一业务暂时不用而只暴露示例所需子集；C# 与 Unity facade 可以有意选择较小的高层封装面。复杂值应使用带明确所有权和销毁规则的 opaque handle 或调用方 buffer，不能暴露内部布局。
- Host Manifest、内建声明和 Analyzer 的类型/成员描述应复用兼容的共享模型；新增 host type、method、constant、enum 等扩展点时，不要为各入口设计互不兼容的元数据体系。
- `crates/rils_capi/include/rils.h` 是 ABI 与绑定生成器的输入，不是 Unity 运行时产物；修改后运行 `python tools/generate-csharp-bindings.py`，并用 `--check` 验证生成代码同步。
- `Rils.CSharp` 保持 `.NET Standard 2.1`、不依赖 Unity API。业务项目通过该 facade 使用 P/Invoke，不要重复声明原生入口。
- Unity drop-in 包固定为 `Rils.CSharp/` 根目录放 C# 源码和 asmdef，原生库放 `Internal/<architecture>/`；当前只支持 `Internal/x86_64/rils_capi.dll`。
- Unity Editor 可执行 `Compile/CompileFile -> GetBytecode/WriteBytecodeFile` 并把 `.bytes` 交给 Addressables；Player 默认只用 `LoadBytecode(byte[])`，不要求发布源码或还原模块目录。
- Unity 导出使用 `python tools/export-unity-package.py`，输出目录属于生成物，不进入 git；增加架构时同步构建脚本、目录布局、插件导入说明和真实运行测试。

# 静态分析与编辑器同步

- 名称解析、类型/trait 检查、控制流、所有权和引用逃逸规则应尽量放在可由 CLI、字节码编译器和 Analyzer 共用的独立阶段。
- 新增或修改语法时，同步更新 Analyzer、VS Code TextMate/语言配置以及对应测试；仅运行时内部优化不需要制造编辑器版本变更。
- 保留准确源码 Span。跨文件能力应尽量保留文件身份，确保 Analyzer 定义/引用和诊断能指向正确 URI。
- 项目级编译和分析必须为源码与声明保留稳定的 `SourceId`/`ModuleId`/`SymbolId`（或等价身份）；跨文件定义和引用不能只按符号文本与大致种类匹配，依赖文件诊断也不能借用入口文件的源码位置。
- 类型成员补全应从解析后的 receiver 类型和共享声明表产生，覆盖内建、用户定义、trait 与宿主类型；只对简单变量名或少数硬编码类型特殊处理只能作为过渡实现。

# 测试与文档

- 语言语义、编译后端或 Analyzer 改动完成后，至少运行 `cargo fmt --check`、`cargo test --workspace` 和 `cargo clippy --workspace --all-targets -- -D warnings`。
- VS Code 插件改动后运行其 `npm run check`；需要生成安装包时，从仓库根目录运行 `python tools/release-vscode.py`。
- 用户可见语法或语义同步更新 `docs/language/` 中对应章节及目录；当前能力和使用入口同步更新 `README.md`；实施状态和后续顺序同步更新 `docs/implementation-roadmap.md`；字节码边界和格式设计同步更新 `docs/bytecode.md`。
- 文档中的“当前支持”“暂不支持”和后续计划必须与实现一致。已经完成的能力要及时从待办列表移除。
- 用户可见升级同时更新根 `CHANGELOG.md` 的 `Unreleased`；破坏性类型、语法、ABI 或格式变化必须给出迁移说明。
- C API/C# 改动至少运行绑定生成 `--check`、对应 Rust 测试、C# build 和真实动态库冒烟测试；Unity 导出改动还要核对导出源码与原生库和源产物一致。
- 数值、容器、Manifest、C ABI 和项目模型等具有组合边界的能力应使用表驱动或矩阵测试覆盖类型、边界值、失败路径和多文件场景，不能只验证单个 happy path。性能目标应有可重复的 release benchmark；冻结为发布门槛后再设置能阻止显著回退的宽松 CI 阈值。

# 编辑器插件

- 插件版本跟着主项目版本走，bug修复或者功能增强时，使用 fix 版本号。
- 比如主版本 0.1.0， 那么插件版本应该发布 0.1.0，0.1.1，0.1.2 等等。
- 生成的插件包放在插件项目下 dist/ 子目录下（不进 git）。
- 不要每次重新打包都更新 fix 版本，版本号的更新**必须**要经过同意。
