# rils_syntax

Rils 的共享语法层，包含 lexer、parser、AST、源码位置和语法类型模型。

该 crate 不依赖 runtime、builtins 或静态分析，可同时用于普通源码和构建期标准库声明解析。
