use super::*;

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
    let mut depth = 1usize;
    let mut current = start;
    while current < tokens.len() {
        if token_kinds_equal(&tokens[current].kind, opening) {
            depth += 1;
        } else if token_kinds_equal(&tokens[current].kind, closing) {
            depth -= 1;
            if depth == 0 {
                return Ok((&tokens[start..current], current + 1));
            }
        } else if matches!(tokens[current].kind, TokenKind::Eof) {
            break;
        }
        current += 1;
    }
    Err(error("unterminated delimited token tree", span))
}

pub(super) fn expect_identifier(
    tokens: &[Token],
    current: &mut usize,
    message: &str,
) -> Result<(String, Span), ParseError> {
    match tokens.get(*current) {
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
    if tokens
        .get(*current)
        .is_some_and(|token| token_kinds_equal(&token.kind, expected))
    {
        *current += 1;
        true
    } else {
        false
    }
}

pub(super) fn token_kinds_equal(left: &TokenKind, right: &TokenKind) -> bool {
    match (left, right) {
        (TokenKind::Identifier(left), TokenKind::Identifier(right)) => left == right,
        (TokenKind::Integer(left), TokenKind::Integer(right)) => left == right,
        (TokenKind::Float(left), TokenKind::Float(right)) => left == right,
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
