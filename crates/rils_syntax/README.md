# rils_syntax

Rils 的共享语法层，包含 lexer、parser、AST、源码位置和语法类型模型。

该 crate 不依赖 runtime、builtins 或静态分析，可同时用于普通源码和构建期标准库声明解析。
`rils_quote!` 生成待解析的 Rils 声明，派生展开时会使用原声明的位置解析为 AST；
`rils_quote_tokens!` 生成可组合片段。
过程宏实现在独立的 `rils_syntax_macros` crate 中，由本 crate 统一导出。
