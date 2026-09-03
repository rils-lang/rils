use std::mem::discriminant;

mod declaration;
mod expression;
mod pattern;
mod support;
mod type_annotation;

use crate::cursor::TokenStream;
use crate::{
    ast::{
        AssociatedType, Attribute, BinaryOp, Block, EnumVariant, Expr, GenericParameter,
        ImplMethod, Literal, LogicalOp, MacroSymbol, MatchArm, NamedField, Parameter, Pattern,
        Program, Stmt, TraitMethod, TypeReference, UnaryOp, UseImport, UseImportKind, Visibility,
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
    parse_with_options(tokens, crate::macros::STANDARD_NATIVE_MACROS, false)
}

pub fn parse_with_native_macros(
    tokens: Vec<Token>,
    native_macros: &[crate::macros::NativeMacroDefinition],
) -> Result<Program, ParseError> {
    parse_with_options(tokens, native_macros, false)
}

/// Parses trusted standard-library declarations, whose callback signatures may
/// contain lexical reference parameters without constructing an owned reference value.
pub fn parse_builtin_declarations(tokens: Vec<Token>) -> Result<Program, ParseError> {
    parse_with_options(tokens, crate::macros::STANDARD_NATIVE_MACROS, true)
}

fn parse_with_options(
    tokens: Vec<Token>,
    native_macros: &[crate::macros::NativeMacroDefinition],
    allow_nested_parameter_references: bool,
) -> Result<Program, ParseError> {
    validate_delimiters(&tokens)?;
    let expansion = crate::macros::expand(tokens, native_macros)?;
    let stream = TokenStream::new(expansion.tokens);
    let mut program = Parser::new(&stream, expansion.macros, allow_nested_parameter_references)
        .parse_program()?;
    if !allow_nested_parameter_references {
        crate::derive::expand(&mut program)?;
    }
    Ok(program)
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
    let fragment = mask_macro_invocations(tokens);
    let stream = TokenStream::new(fragment);
    let mut parser = Parser::new(&stream, Vec::new(), false);
    parser.expression().is_ok() && parser.is_at_end()
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

struct Parser<'a> {
    stream: &'a TokenStream,
    position: usize,
    generic_scopes: Vec<Vec<GenericParameter>>,
    type_references: Vec<TypeReference>,
    macros: Vec<MacroSymbol>,
    loop_depth: usize,
    block_depth: usize,
    allow_nested_parameter_references: bool,
    fallback_token: Token,
}

impl<'a> Parser<'a> {
    pub(super) fn is_at_end(&self) -> bool {
        self.stream.cursor_at(self.position).is_at_end()
    }

    fn new(
        stream: &'a TokenStream,
        macros: Vec<MacroSymbol>,
        allow_nested_parameter_references: bool,
    ) -> Self {
        Self {
            stream,
            position: 0,
            generic_scopes: Vec::new(),
            type_references: Vec::new(),
            macros,
            loop_depth: 0,
            block_depth: 0,
            allow_nested_parameter_references,
            fallback_token: Token::new(TokenKind::Identifier(String::new()), Span::new(0, 0)),
        }
    }

    fn parse_program(mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();
        while !self.is_at_end() {
            statements.push(self.statement()?);
        }
        Ok(Program {
            statements,
            type_references: self.type_references,
            macros: self.macros,
        })
    }

    fn statement(&mut self) -> Result<Stmt, ParseError> {
        if self.check(&TokenKind::Hash) {
            let attributes = self.attributes()?;
            let mut statement = self.statement()?;
            let target = match &mut statement {
                Stmt::Struct { attributes, .. }
                | Stmt::Enum { attributes, .. }
                | Stmt::Function { attributes, .. } => attributes,
                _ => {
                    return Err(ParseError {
                        message: "attributes are currently supported on structs and enums only"
                            .into(),
                        span: attributes[0].span,
                    });
                }
            };
            target.extend(attributes);
            return Ok(statement);
        }
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
            let mut statement = self.statement()?;
            match statement.visibility() {
                Some(Visibility::Private) => {
                    statement.set_visibility(Visibility::Public);
                }
                Some(_) => {
                    return Err(ParseError {
                        message: "visibility is already specified for this declaration".into(),
                        span: token.span,
                    });
                }
                None => {
                    return Err(ParseError {
                        message: "`pub` is only allowed on declarations, modules, and use items"
                            .into(),
                        span: token.span,
                    });
                }
            }
            merge_statement_start(&mut statement, token.span);
            return Ok(statement);
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

    fn attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attributes = Vec::new();
        while let Some(hash) = self.take(&TokenKind::Hash) {
            self.expect(&TokenKind::LeftBracket, "expected `[` after `#`")?;
            let (first, _) = self.expect_identifier("expected attribute name")?;
            let mut path = vec![first];
            while self.take(&TokenKind::ColonColon).is_some() {
                path.push(self.expect_identifier("expected attribute path segment")?.0);
            }
            let mut arguments = Vec::new();
            if self.take(&TokenKind::LeftParen).is_some() {
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        let (first, _) = self.expect_identifier("expected attribute argument")?;
                        let mut argument = vec![first];
                        while self.take(&TokenKind::ColonColon).is_some() {
                            argument
                                .push(self.expect_identifier("expected attribute path segment")?.0);
                        }
                        arguments.push(argument);
                        if self.take(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                self.expect(
                    &TokenKind::RightParen,
                    "expected `)` after attribute arguments",
                )?;
            }
            let right = self.expect(&TokenKind::RightBracket, "expected `]` after attribute")?;
            attributes.push(Attribute {
                path,
                arguments,
                span: hash.span.merge(right.span),
            });
        }
        Ok(attributes)
    }
}

fn merge_statement_start(statement: &mut Stmt, start: Span) {
    let span = match statement {
        Stmt::Module { span, .. }
        | Stmt::Use { span, .. }
        | Stmt::Function { span, .. }
        | Stmt::Struct { span, .. }
        | Stmt::Enum { span, .. }
        | Stmt::TypeAlias { span, .. }
        | Stmt::Trait { span, .. } => span,
        _ => return,
    };
    *span = start.merge(*span);
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
#[path = "../tests/unit/parser.rs"]
mod tests;
