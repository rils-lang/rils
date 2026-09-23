//! Parse generated Rils source while attributing it to the declaration that produced it.

use crate::{
    ast::Stmt,
    lexer,
    macros::STANDARD_NATIVE_MACROS,
    parser::{self, ParseCapabilities, ParseError},
    source::Span,
};

/// Generated source whose location is supplied when a derive is expanded.
#[derive(Clone, Debug)]
pub struct QuotedStatement {
    source: String,
}

impl QuotedStatement {
    pub fn new(source: String) -> Self {
        Self { source }
    }

    pub fn parse(self, origin: Span) -> Result<Stmt, ParseError> {
        statement(&self.source, origin)
    }
}

fn statement(source: &str, origin: Span) -> Result<Stmt, ParseError> {
    let mut tokens =
        lexer::lex_with_source_id(source, origin.source).map_err(|error| ParseError {
            message: error.message,
            span: origin,
        })?;
    for token in &mut tokens {
        token.span = origin;
    }
    let mut program =
        parser::parse_with_capabilities(tokens, STANDARD_NATIVE_MACROS, ParseCapabilities::USER)
            .map_err(|error| ParseError {
                message: error.message,
                span: origin,
            })?;
    if program.statements.len() != 1 {
        return Err(ParseError {
            message: "quoted Rils source must contain exactly one statement".into(),
            span: origin,
        });
    }
    Ok(program.statements.remove(0))
}
