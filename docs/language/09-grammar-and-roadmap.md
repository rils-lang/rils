# 语法摘要与后续候选

[← 返回语言手册目录](README.md)

## 简化语法

```ebnf
program     = moduleStatement* EOF ;
moduleStatement = itemDecl | blockStatement ;
itemDecl    = macroDecl | structDecl | enumDecl | implDecl | traitDecl
            | typeAlias | moduleDecl | useDecl | publicDecl ;
blockStatement = letDecl | fnDecl | whileStmt | loopStmt | forStmt
               | returnStmt | breakStmt | continueStmt | exprStmt ;
letDecl     = "let" "mut"? IDENT (":" type)? "=" expression ";" ;
moduleDecl  = "mod" IDENT (";" | "{" moduleStatement* "}") ;
useDecl     = "use" path ("as" IDENT)? ";" ;
publicDecl  = "pub" (fnDecl | structDecl | enumDecl | traitDecl
            | typeAlias | moduleDecl | useDecl) ;
path        = IDENT ("::" IDENT)* ;
fnDecl      = "fn" IDENT genericParams?
              "(" parameters? ")" ("->" type)? block ;
macroDecl   = legacyMacroDecl | branchingMacroDecl ;
legacyMacroDecl = "macro" IDENT "(" macroParams? ")"
                  "{" macroTokens* "}" ;
macroParams = macroParam ("," macroParam)* ;
macroParam  = "$" IDENT ;
branchingMacroDecl = "macro" IDENT "{" macroArm+ "}" ;
macroArm    = "(" macroMatcher* ")" "=>"
              "{" macroTokens* "}" ("," | ";")? ;
macroMatcher = macroCapture | macroRepeat | TOKEN ;
macroCapture = "$" IDENT ":" ("expr" | "lit" | "ident") ;
macroRepeat = "$(" macroMatcher* ")" TOKEN? ("*" | "+") ;
genericParams = "<" genericParam ("," genericParam)* ">" ;
genericParam = IDENT (":" IDENT ("+" IDENT)*)? ;
parameters  = parameter ("," parameter)* ;
parameter   = receiver | "mut"? IDENT (":" type)? ;
receiver    = "self" | "mut" "self" | "&" "self" | "&" "mut" "self" ;
type        = "&" "mut"? type
            | "()" | "bool" | "int" | "float" | "string"
            | "function" | "Option" "<" type ">"
            | "Result" "<" type "," type ">"
            | "(" type "," (type ",")* ")"
            | "[" type ";" INTEGER "]"
            | "fn" "(" (type ("," type)*)? ")" "->" type
            | IDENT ("::" IDENT)* ("<" type ("," type)* ">")?
            | "<" type "as" IDENT ">" "::" IDENT
              ("<" type ("," type)* ">")? ;
typeAlias   = "type" IDENT genericParams? "=" type ";" ;
structDecl  = "struct" IDENT genericParams? "{" namedFields "}" ;
enumDecl    = "enum" IDENT genericParams? "{"
              enumVariant ("," enumVariant)* ","? "}" ;
enumVariant = IDENT
            | IDENT "(" (type ("," type)*)? ")"
            | IDENT "{" namedFields "}" ;
namedFields = IDENT ":" type ("," IDENT ":" type)* ","? ;
traitDecl   = "trait" IDENT "{"
              (traitMethod | associatedType)* "}" ;
traitMethod = "fn" IDENT genericParams?
              "(" parameters? ")" ("->" type)? ";" ;
associatedType = "type" IDENT genericParams? ("=" type)? ";" ;
implDecl    = "impl" genericParams?
              (type | IDENT "for" type) "{"
              (fnDecl | associatedType)* "}" ;
whileStmt   = "while" expression block ;
loopStmt    = "loop" block ;
forStmt     = "for" IDENT "in" expression block ;
returnStmt  = "return" expression? ";"? ;
breakStmt   = "break" expression? ";"? ;
continueStmt = "continue" ";" ;
block       = "{" blockStatement* "}" ;

expression  = assignment ;
assignment  = range ("=" assignment)? ;
range       = logicOr (".." logicOr)? ;
logicOr     = logicAnd ("||" logicAnd)* ;
logicAnd    = equality ("&&" equality)* ;
equality    = comparison (("==" | "!=") comparison)* ;
comparison  = term (("<" | "<=" | ">" | ">=") term)* ;
term        = factor (("+" | "-") factor)* ;
factor      = unary (("*" | "/" | "%") unary)* ;
unary       = ("!" | "-" | "*" | "&" "mut"?) unary | call ;
call        = primary
              ("(" arguments? ")" | "." (IDENT | INTEGER)
              | "[" expression "]" | recordFields)* ;
recordFields = "{" IDENT ":" expression
               ("," IDENT ":" expression)* ","? "}" ;
primary     = literal | IDENT | macroInvocation | "()" | "(" expression ")"
            | "(" expression "," (expression ",")* ")"
            | "[" (expression ("," expression)* ",?" | expression ";" expression)? "]"
            | "<" type "as" IDENT ">" "::" IDENT
            | block | ifExpr | matchExpr ;
macroInvocation = IDENT "!" "(" macroArguments? ")" ;
macroArguments  = macroTokens ("," macroTokens)* ;
macroTokens     = /* 保持括号与花括号平衡的任意 token 序列 */ ;
ifExpr      = "if" expression block ("else" (ifExpr | block))? ;
matchExpr   = "match" expression "{"
              matchArm (","? matchArm)* ","? "}" ;
matchArm    = pattern "=>" expression ;
pattern     = "_" | IDENT | literal | "()"
            | "Some" "(" pattern ")" | "None"
            | IDENT "::" IDENT
            | IDENT "::" IDENT "(" patterns? ")"
            | path "{" recordPatterns "}"
            | "(" pattern ")" ;
```

## 后续候选

- 模式守卫、或模式与 `@` 绑定
- tuple struct、unit struct 和 record 模式中的 `..`
- 默认 trait 方法、trait 对象和条件 impl
- `where`、显式类型实参和 const 泛型
- HashMap/HashSet 以及借用形式的容器迭代器
- 带标签的循环控制
- 通配/分组导入以及 `crate`、`self`、`super` 模块路径
- 宿主原生类型的属性、可写接收者和静态方法元数据
- 卫生宏、更多片段类型、嵌套重复与过程宏
- 字节码自定义宿主注册整合和稳定磁盘格式
- 静态分析的循环固定点、动态索引 place 精度和跨文件增量缓存
- 可选的 Rust AOT 代码生成
