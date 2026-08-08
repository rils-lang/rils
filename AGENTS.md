# 目录规范

- crates/ 下放置主项目依赖的本地包，这些包需要和主项目统一版本，包名必须以 `rils_` 开头
- examples/ 放各类测试用例
- editors/ 放各类编辑器插件
- tools/ 放各类辅助工具项目或脚本

# 版本规范

- 主 crate、主项目依赖的本地 crate、Analyzer 和编辑器插件以主项目版本为基线保持一致。
- 不要因为本地构建、重新打包或尚未发布的实现自动修改版本号。
- 任何 crate 或插件的版本号更新都必须先获得同意。

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
- IO 和文件系统分别通过 `std::io`、`std::fs` 等明确路径访问；可失败操作返回结构化 `Result<T, E>`，不要用隐式异常替代。
- 集合以数组、`Vec<T>`、`HashMap<K, V>`、`HashSet<T>` 等拥有型类型为基础，并通过 `Iterator` / `IntoIterator` 接入 `for`。
- 新增标准库模块时，同步维护运行时签名、类型推断、Analyzer 可见符号和语言文档，避免解释器与编辑器各自维护不一致的接口。

# 编译器与字节码

- 前端流水线保持 `lexer/parser -> static analysis -> HIR -> MIR -> bytecode -> verifier -> VM` 的分层，不要让 VM 直接依赖 AST。
- AST 解释器是字节码迁移期间的完整语义参考。新增字节码能力时，应增加同一源码在解释器和 VM 下结果一致的对照测试。
- 合法但尚未支持的字节码语法必须返回带源码范围的明确 `CompileError`，不能静默退回解释执行。
- 字节码加载或执行前必须经过验证；函数索引、寄存器、局部槽位、跳转、类型表和后续导入表都不能信任外部输入。
- `compile` 不进行隐式文件访问；`compile_file` 与 `Engine::eval_file` 使用一致的 `name.rils` / `name/mod.rils` 递归模块加载和循环检测规则。
- 直接调用保留快速路径；函数值、闭包和宿主调用使用通用调用路径时，不应无必要地降低现有直接调用性能。
- 稳定磁盘格式不得直接序列化 Rust enum 或内存布局。格式版本、语言版本和宿主 ABI 版本分开维护，并在冻结前完成严格限长与索引验证。

# 静态分析与编辑器同步

- 名称解析、类型/trait 检查、控制流、所有权和引用逃逸规则应尽量放在可由 CLI、字节码编译器和 Analyzer 共用的独立阶段。
- 新增或修改语法时，同步更新 Analyzer、VS Code TextMate/语言配置以及对应测试；仅运行时内部优化不需要制造编辑器版本变更。
- 保留准确源码 Span。跨文件能力应尽量保留文件身份，确保 Analyzer 定义/引用和诊断能指向正确 URI。

# 测试与文档

- 语言语义、编译后端或 Analyzer 改动完成后，至少运行 `cargo fmt --check`、`cargo test --workspace` 和 `cargo clippy --workspace --all-targets -- -D warnings`。
- VS Code 插件改动后运行其 `npm run check`；需要生成安装包时，从仓库根目录运行 `python tools/release-vscode.py`。
- 用户可见语法或语义同步更新 `docs/language/` 中对应章节及目录；当前能力和使用入口同步更新 `README.md`；实施状态和后续顺序同步更新 `docs/implementation-roadmap.md`；字节码边界和格式设计同步更新 `docs/bytecode.md`。
- 文档中的“当前支持”“暂不支持”和后续计划必须与实现一致。已经完成的能力要及时从待办列表移除。

# 编辑器插件

- 插件版本跟着主项目版本走，bug修复或者功能增强时，使用 fix 版本号。
- 比如主版本 0.1.0， 那么插件版本应该发布 0.1.0，0.1.1，0.1.2 等等。
- 生成的插件包放在插件项目下 dist/ 子目录下（不进 git）。
- 不要每次重新打包都更新 fix 版本，版本号的更新**必须**要经过同意。
