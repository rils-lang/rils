# 语法摘要

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
useDecl     = "use" useTree ";" ;
useTree     = path ("as" IDENT)?
            | path "::" "*"
            | path? "::"? "{" useTree ("," useTree)* ","? "}" ;
publicDecl  = "pub" (fnDecl | structDecl | enumDecl | traitDecl
            | typeAlias | moduleDecl | useDecl) ;
path        = pathRoot? IDENT ("::" pathSegment)* ;
pathRoot    = ("crate" | "self" | "super") "::" ;
pathSegment = IDENT | "self" | "super" ;
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
            | "()" | "bool" | "char" | integerType | floatType | "string"
            | "function" | "Option" "<" type ">"
            | "Result" "<" type "," type ">"
            | "(" type "," (type ",")* ")"
            | "[" type ";" INTEGER "]"
            | "fn" "(" (type ("," type)*)? ")" "->" type
            | IDENT ("::" IDENT)* ("<" type ("," type)* ">")?
            | "<" type "as" IDENT ">" "::" IDENT
              ("<" type ("," type)* ">")? ;
integerType = "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
            | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" ;
floatType   = "f32" | "f64" ;
typeAlias   = "type" IDENT genericParams? "=" type ";" ;
structDecl  = "struct" IDENT genericParams? (";" | "{" namedFields? "}") ;
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
castExpr    = unaryExpr ("as" integerType)* ;

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
