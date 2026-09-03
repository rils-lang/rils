use super::*;
use crate::cursor::TokenStream;

pub(super) fn delimited(
    tokens: &[Token],
    current: &mut usize,
    opening: TokenKind,
    closing: TokenKind,
    span: Span,
) -> Result<Vec<Token>, ParseError> {
    let (body, next) = slice_delimited(tokens, *current, &opening, &closing, span)?;
    *current = next;
    Ok(body.to_vec())
}

pub(super) fn slice_delimited<'a>(
    tokens: &'a [Token],
    start: usize,
    opening: &TokenKind,
    closing: &TokenKind,
    span: Span,
) -> Result<(&'a [Token], usize), ParseError> {
    let stream = TokenStream::new(tokens[start..].to_vec());
    let mut cursor = stream.cursor();
    let mut offset = 0usize;
    let mut depth = 1usize;
    while let Some(token) = cursor.peek() {
        if token_kinds_equal(&token.kind, opening) {
            depth += 1;
        } else if token_kinds_equal(&token.kind, closing) {
            depth -= 1;
            if depth == 0 {
                return Ok((&tokens[start..start + offset], start + offset + 1));
            }
        }
        let Some((_, next)) = cursor.advance() else {
            break;
        };
        cursor = next;
        offset += 1;
    }
    Err(error("unterminated delimited token tree", span))
}

pub(super) fn expect_identifier(
    tokens: &[Token],
    current: &mut usize,
    message: &str,
) -> Result<(String, Span), ParseError> {
    let stream = TokenStream::new(tokens.to_vec());
    let cursor = stream.cursor_at(*current);
    match cursor.peek() {
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
    tokens: &[Token],
    current: &mut usize,
    expected: &TokenKind,
    message: &str,
) -> Result<(), ParseError> {
    if take(tokens, current, expected) {
        Ok(())
    } else {
        Err(error(message, token_span(tokens.get(*current))))
    }
}

pub(super) fn take(tokens: &[Token], current: &mut usize, expected: &TokenKind) -> bool {
    let stream = TokenStream::new(tokens.to_vec());
    stream
        .cursor_at(*current)
        .check(expected)
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
