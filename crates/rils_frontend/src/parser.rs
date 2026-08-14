use std::mem::discriminant;

mod declaration;
mod expression;
mod pattern;
mod support;
mod type_annotation;

use crate::{
    ast::{
        AssociatedType, BinaryOp, Block, EnumVariant, Expr, GenericParameter, ImplMethod, Literal,
        LogicalOp, MacroSymbol, MatchArm, NamedField, Parameter, Pattern, Program, Stmt,
        TraitMethod, TypeReference, UnaryOp,
    },
    source::Span,
    token::{Token, TokenKind},
    types::Type,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, ParseError> {
    parse_with_native_macros(tokens, crate::macros::STANDARD_NATIVE_MACROS)
}

pub fn parse_with_native_macros(
    tokens: Vec<Token>,
    native_macros: &[crate::macros::NativeMacroDefinition],
) -> Result<Program, ParseError> {
    validate_delimiters(&tokens)?;
    let expansion = crate::macros::expand(tokens, native_macros)?;
    Parser::new(expansion.tokens, expansion.macros).parse_program()
}

fn validate_delimiters(tokens: &[Token]) -> Result<(), ParseError> {
    let mut stack = Vec::new();
    for token in tokens {
        let expected = match token.kind {
            TokenKind::LeftParen => Some(TokenKind::RightParen),
            TokenKind::LeftBracket => Some(TokenKind::RightBracket),
            TokenKind::LeftBrace => Some(TokenKind::RightBrace),
            _ => None,
        };
        if let Some(expected) = expected {
            stack.push((expected, token.span));
            continue;
        }
        if matches!(
            token.kind,
            TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace
        ) {
            let Some((expected, _)) = stack.pop() else {
                return Err(ParseError {
                    message: format!("unexpected {}", token.kind.name()),
                    span: token.span,
                });
            };
            if discriminant(&expected) != discriminant(&token.kind) {
                return Err(ParseError {
                    message: format!("expected {}, found {}", expected.name(), token.kind.name()),
                    span: token.span,
                });
            }
        }
    }
    if let Some((expected, open)) = stack.pop() {
        let eof = tokens.last().map_or(open, |token| token.span);
        return Err(ParseError {
            message: format!("expected {} after delimiter", expected.name()),
            span: eof,
        });
    }
    Ok(())
}

fn scalar_literal(kind: &TokenKind) -> Option<Literal> {
    Some(match kind {
        TokenKind::I8(value) => Literal::I8(*value),
        TokenKind::I16(value) => Literal::I16(*value),
        TokenKind::I32(value) => Literal::I32(*value),
        TokenKind::I64(value) => Literal::I64(*value),
        TokenKind::I128(value) => Literal::I128(*value),
        TokenKind::Isize(value) => Literal::Isize(*value),
        TokenKind::U8(value) => Literal::U8(*value),
        TokenKind::U16(value) => Literal::U16(*value),
        TokenKind::U32(value) => Literal::U32(*value),
        TokenKind::U64(value) => Literal::U64(*value),
        TokenKind::U128(value) => Literal::U128(*value),
        TokenKind::Usize(value) => Literal::Usize(*value),
        TokenKind::F32(value) => Literal::F32(*value),
        TokenKind::F64(value) => Literal::F64(*value),
        TokenKind::Char(value) => Literal::Char(*value),
        TokenKind::Integer(value) => Literal::Integer(*value),
        TokenKind::Float(value) => Literal::Float(*value),
        _ => return None,
    })
}

fn negated_scalar_literal(kind: &TokenKind) -> Option<Literal> {
    Some(match kind {
        TokenKind::I8(value) => Literal::I8(value.checked_neg()?),
        TokenKind::I16(value) => Literal::I16(value.checked_neg()?),
        TokenKind::I32(value) => Literal::I32(value.checked_neg()?),
        TokenKind::I64(value) => Literal::I64(value.checked_neg()?),
        TokenKind::I128(value) => Literal::I128(value.checked_neg()?),
        TokenKind::Isize(value) => Literal::Isize(value.checked_neg()?),
        TokenKind::F32(value) => Literal::F32(-value),
        TokenKind::F64(value) => Literal::F64(-value),
        TokenKind::Integer(value) => Literal::Integer(value.checked_neg()?),
        TokenKind::Float(value) => Literal::Float(-value),
        _ => return None,
    })
}

pub(crate) fn is_expression_fragment(tokens: &[Token]) -> bool {
    if tokens.is_empty() || !has_balanced_delimiters(tokens) {
        return false;
    }
    let mut fragment = mask_macro_invocations(tokens);
    let end = fragment.last().map_or(0, |token| token.span.end);
    fragment.push(Token::new(TokenKind::Eof, Span::new(end, end)));
    let mut parser = Parser::new(fragment, Vec::new());
    parser.expression().is_ok() && parser.check(&TokenKind::Eof)
}

fn has_balanced_delimiters(tokens: &[Token]) -> bool {
    let mut parens = 0usize;
    let mut braces = 0usize;
    let mut brackets = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::LeftParen => parens += 1,
            TokenKind::RightParen if parens > 0 => parens -= 1,
            TokenKind::RightParen => return false,
            TokenKind::LeftBrace => braces += 1,
            TokenKind::RightBrace if braces > 0 => braces -= 1,
            TokenKind::RightBrace => return false,
            TokenKind::LeftBracket => brackets += 1,
            TokenKind::RightBracket if brackets > 0 => brackets -= 1,
            TokenKind::RightBracket => return false,
            _ => {}
        }
    }
    parens == 0 && braces == 0 && brackets == 0
}

fn mask_macro_invocations(tokens: &[Token]) -> Vec<Token> {
    let mut output = Vec::new();
    let mut current = 0;
    while current < tokens.len() {
        if matches!(
            tokens.get(current).map(|token| &token.kind),
            Some(TokenKind::Identifier(_))
        ) && matches!(
            tokens.get(current + 1).map(|token| &token.kind),
            Some(TokenKind::Bang)
        ) && matches!(
            tokens.get(current + 2).map(|token| &token.kind),
            Some(TokenKind::LeftParen)
        ) {
            let mut depth = 1usize;
            let mut end = current + 3;
            while end < tokens.len() {
                match tokens[end].kind {
                    TokenKind::LeftParen => depth += 1,
                    TokenKind::RightParen => {
                        depth -= 1;
                        if depth == 0 {
                            let span = tokens[current].span.merge(tokens[end].span);
                            output.push(Token::new(TokenKind::I32(0), span));
                            current = end + 1;
                            break;
                        }
                    }
                    _ => {}
                }
                end += 1;
            }
            if depth == 0 {
                continue;
            }
        }
        output.push(tokens[current].clone());
        current += 1;
    }
    output
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
    generic_scopes: Vec<Vec<GenericParameter>>,
    type_references: Vec<TypeReference>,
    macros: Vec<MacroSymbol>,
    loop_depth: usize,
    block_depth: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>, macros: Vec<MacroSymbol>) -> Self {
        Self {
            tokens,
            current: 0,
            generic_scopes: Vec::new(),
            type_references: Vec::new(),
            macros,
            loop_depth: 0,
            block_depth: 0,
        }
    }

    fn parse_program(mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();
        while !self.check(&TokenKind::Eof) {
            statements.push(self.statement()?);
        }
        Ok(Program {
            statements,
            type_references: self.type_references,
            macros: self.macros,
        })
    }

    fn statement(&mut self) -> Result<Stmt, ParseError> {
        if self.block_depth > 0
            && matches!(
                self.peek().kind,
                TokenKind::Pub
                    | TokenKind::Mod
                    | TokenKind::Use
                    | TokenKind::Struct
                    | TokenKind::Enum
                    | TokenKind::Type
                    | TokenKind::Impl
                    | TokenKind::Trait
            )
        {
            return Err(ParseError {
                message: "this item declaration is only allowed at module scope".into(),
                span: self.peek().span,
            });
        }
        if let Some(token) = self.take(&TokenKind::Pub) {
            let statement = self.statement()?;
            if !matches!(
                statement,
                Stmt::Function { .. }
                    | Stmt::Struct { .. }
                    | Stmt::Enum { .. }
                    | Stmt::TypeAlias { .. }
                    | Stmt::Trait { .. }
                    | Stmt::Module { .. }
                    | Stmt::Use { .. }
            ) {
                return Err(ParseError {
                    message: "`pub` is only allowed on declarations, modules, and use items".into(),
                    span: token.span,
                });
            }
            let span = token.span.merge(statement_span_for_parser(&statement));
            return Ok(Stmt::Public {
                statement: Box::new(statement),
                span,
            });
        }
        if let Some(token) = self.take(&TokenKind::Mod) {
            return self.module_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Use) {
            return self.use_statement(token.span);
        }
        if self.take(&TokenKind::Let).is_some() {
            return self.let_statement();
        }
        if let Some(token) = self.take(&TokenKind::Fn) {
            return self.function_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Struct) {
            return self.struct_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Enum) {
            return self.enum_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Type) {
            return self.type_alias_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Impl) {
            return self.impl_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Trait) {
            return self.trait_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::While) {
            return self.while_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Loop) {
            return self.loop_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::For) {
            return self.for_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Return) {
            return self.return_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Break) {
            return self.break_statement(token.span);
        }
        if let Some(token) = self.take(&TokenKind::Continue) {
            return self.continue_statement(token.span);
        }
        self.expression_statement()
    }
}

fn statement_span_for_parser(statement: &Stmt) -> Span {
    match statement {
        Stmt::Public { span, .. }
        | Stmt::Module { span, .. }
        | Stmt::Use { span, .. }
        | Stmt::Let { span, .. }
        | Stmt::Function { span, .. }
        | Stmt::Struct { span, .. }
        | Stmt::Enum { span, .. }
        | Stmt::TypeAlias { span, .. }
        | Stmt::Impl { span, .. }
        | Stmt::Trait { span, .. }
        | Stmt::While { span, .. }
        | Stmt::Loop { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. } => *span,
        Stmt::Expr { expression, .. } => expression.span(),
    }
}

fn expression_path(expression: &Expr) -> Option<Vec<String>> {
    match expression {
        Expr::Variable { name, .. } => Some(vec![name.clone()]),
        Expr::Path { segments, .. } => Some(segments.clone()),
        _ => None,
    }
}

fn enum_variant_name(variant: &EnumVariant) -> &str {
    match variant {
        EnumVariant::Unit { name, .. }
        | EnumVariant::Tuple { name, .. }
        | EnumVariant::Record { name, .. } => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    #[test]
    fn parses_function_and_if_expression() {
        let source = "fn max(a, b) { if a > b { a } else { b } }";
        let program = parse(lex(source).unwrap()).unwrap();
        assert!(matches!(program.statements[0], Stmt::Function { .. }));
    }

    #[test]
    fn rejects_invalid_assignment_target() {
        let error = parse(lex("(1 + 2) = 3;").unwrap()).unwrap_err();
        assert_eq!(error.message, "invalid assignment target");
    }

    #[test]
    fn rejects_unclosed_delimiters_before_macro_fragment_matching() {
        for source in [
            "call(",
            "call([1, 2",
            "call({ let value = 1;",
            "call([1, 2})",
        ] {
            let error = parse(lex(source).unwrap()).expect_err("delimiter must be rejected");
            assert!(
                error.message.contains("expected") || error.message.contains("unexpected"),
                "unexpected error for `{source}`: {}",
                error.message
            );
        }
    }

    #[test]
    fn only_functions_can_be_declared_inside_blocks() {
        let error = parse(lex("fn outer() { struct Local { value: i32 } }").unwrap())
            .expect_err("local type items are not part of the language");
        assert!(error.message.contains("module scope"));

        parse(lex("fn outer() { fn local() -> i32 { 1 } local() }").unwrap())
            .expect("nested functions remain valid");
    }

    #[test]
    fn recognizes_function_call_comparisons_as_macro_expression_fragments() {
        let mut tokens = crate::lexer::lex("type_of(getter) == \"fn() -> i32\"").unwrap();
        tokens.pop();
        assert!(super::is_expression_fragment(&tokens));
    }

    #[test]
    fn reports_removed_numeric_type_names() {
        let integer = parse(lex("let value: int = 1;").unwrap()).unwrap_err();
        assert!(integer.message.contains("`int` was removed"));

        let float = parse(lex("let value: float = 1.0;").unwrap()).unwrap_err();
        assert!(float.message.contains("`float` was removed"));
    }

    #[test]
    fn parses_crate_self_and_super_paths() {
        let source = r#"
            use crate::math::Value;
            fn read(value: self::Value) {
                crate::math::make();
                self::helper();
                super::super::shared::run();
            }
        "#;
        parse(lex(source).unwrap()).unwrap();
    }
}
