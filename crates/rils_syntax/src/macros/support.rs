use super::*;
use crate::cursor::TokenStream;
pub(super) fn delimited(
    stream: &TokenStream,
    current: &mut usize,
    opening: TokenKind,
    closing: TokenKind,
    span: Span,
) -> Result<TokenStream, ParseError> {
    let (body, next) = slice_delimited(stream, *current, &opening, &closing, span)?;
    *current = next;
    Ok(body)
}

pub(super) fn slice_delimited(
    stream: &TokenStream,
    start: usize,
    opening: &TokenKind,
    closing: &TokenKind,
    span: Span,
) -> Result<(TokenStream, usize), ParseError> {
    let mut offset = 0usize;
    let mut depth = 1usize;
    let mut cursor = stream.cursor_at(start);
    while let Some(token) = cursor.peek() {
        if token_kinds_equal(&token.kind, opening) {
            depth += 1;
        } else if token_kinds_equal(&token.kind, closing) {
            depth -= 1;
            if depth == 0 {
                let mut body = Vec::with_capacity(offset);
                let mut body_cursor = stream.cursor_at(start);
                for _ in 0..offset {
                    let Some(token) = body_cursor.peek() else {
                        return Err(error("unterminated delimited token tree", span));
                    };
                    body.push(token.clone());
                    body_cursor = body_cursor
                        .advance()
                        .expect("body cursor token was present")
                        .1;
                }
                let body = TokenStream::new(body)
                    .map_err(|span| error("unterminated delimited token tree", span))?;
                return Ok((body, start + offset + 1));
            }
        }
        offset += 1;
        cursor = cursor
            .advance()
            .expect("delimited cursor token was present")
            .1;
    }
    Err(error("unterminated delimited token tree", span))
}

pub(super) fn expect_identifier(
    stream: &TokenStream,
    current: &mut usize,
    message: &str,
) -> Result<(String, Span), ParseError> {
    match stream.cursor_at(*current).peek() {
        Some(Token {
            kind: TokenKind::Identifier(name),
            span,
        }) => {
            *current += 1;
            Ok((name.clone(), *span))
        }
        token => Err(error(message, token_span(token))),
    }
}

pub(super) fn expect(
    stream: &TokenStream,
    current: &mut usize,
    expected: &TokenKind,
    message: &str,
) -> Result<(), ParseError> {
    if take(stream, current, expected) {
        Ok(())
    } else {
        Err(error(
            message,
            token_span(stream.cursor_at(*current).peek()),
        ))
    }
}

pub(super) fn take(stream: &TokenStream, current: &mut usize, expected: &TokenKind) -> bool {
    stream
        .cursor_at(*current)
        .peek()
        .is_some_and(|token| token_kinds_equal(&token.kind, expected))
        .then(|| *current += 1)
        .is_some()
}

pub(super) fn token_kinds_equal(left: &TokenKind, right: &TokenKind) -> bool {
    match (left, right) {
        (TokenKind::Identifier(left), TokenKind::Identifier(right)) => left == right,
        (TokenKind::Integer(left), TokenKind::Integer(right)) => left == right,
        (TokenKind::Float(left), TokenKind::Float(right)) => left == right,
        (TokenKind::I8(left), TokenKind::I8(right)) => left == right,
        (TokenKind::I16(left), TokenKind::I16(right)) => left == right,
        (TokenKind::I32(left), TokenKind::I32(right)) => left == right,
        (TokenKind::I64(left), TokenKind::I64(right)) => left == right,
        (TokenKind::I128(left), TokenKind::I128(right)) => left == right,
        (TokenKind::Isize(left), TokenKind::Isize(right)) => left == right,
        (TokenKind::U8(left), TokenKind::U8(right)) => left == right,
        (TokenKind::U16(left), TokenKind::U16(right)) => left == right,
        (TokenKind::U32(left), TokenKind::U32(right)) => left == right,
        (TokenKind::U64(left), TokenKind::U64(right)) => left == right,
        (TokenKind::U128(left), TokenKind::U128(right)) => left == right,
        (TokenKind::Usize(left), TokenKind::Usize(right)) => left == right,
        (TokenKind::F32(left), TokenKind::F32(right)) => left == right,
        (TokenKind::F64(left), TokenKind::F64(right)) => left == right,
        (TokenKind::Char(left), TokenKind::Char(right)) => left == right,
        (TokenKind::String(left), TokenKind::String(right)) => left == right,
        _ => std::mem::discriminant(left) == std::mem::discriminant(right),
    }
}

pub(super) fn token_span(token: Option<&Token>) -> Span {
    token.map_or(Span::new(0, 0), |token| token.span)
}

pub(super) fn error(message: impl Into<String>, span: Span) -> ParseError {
    ParseError {
        message: message.into(),
        span,
    }
}
