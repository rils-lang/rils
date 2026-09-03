use super::*;
use crate::cursor::TokenStream;

pub(super) fn expand_sequence(
    tokens: &[Token],
    definitions: &HashMap<String, MacroTemplate>,
    stack: &mut Vec<String>,
) -> Result<Vec<Token>, ParseError> {
    let mut output = Vec::new();
    let stream = TokenStream::new(tokens.to_vec());
    let mut cursor = stream.cursor();
    while let Some(token) = cursor.peek() {
        let current = cursor.position();
        let invocation = match tokens.get(current..current + 3) {
            Some(
                [
                    Token {
                        kind: TokenKind::Identifier(name),
                        span,
                    },
                    Token {
                        kind: TokenKind::Bang,
                        ..
                    },
                    Token {
                        kind: TokenKind::LeftParen,
                        ..
                    },
                ],
            ) => Some((name.clone(), *span)),
            _ => None,
        };

        let Some((name, call_span)) = invocation else {
            output.push(token.clone());
            cursor = cursor.advance().expect("cursor token was present").1;
            continue;
        };
        let definition = definitions
            .get(&name)
            .ok_or_else(|| error(format!("unknown macro `{name}`"), call_span))?;
        if stack.len() >= MAX_EXPANSION_DEPTH {
            let mut chain = stack.join(" -> ");
            if !chain.is_empty() {
                chain.push_str(" -> ");
            }
            chain.push_str(&name);
            return Err(error(
                format!("macro expansion exceeded {MAX_EXPANSION_DEPTH} levels: {chain}"),
                call_span,
            ));
        }

        let (input, next) = invocation_input(tokens, current + 2, call_span)?;
        if matches!(name.as_str(), "print" | "println") {
            validate_format_invocation(&name, &input, call_span)?;
        }
        let Some((arm, bindings)) = definition
            .arms
            .iter()
            .find_map(|arm| match_arm(&arm.matcher, &input).map(|bindings| (arm, bindings)))
        else {
            if let [arm] = definition.arms.as_slice()
                && let Some(expected) = arm.legacy_arity
            {
                return Err(error(
                    format!(
                        "macro `{name}` expects {expected} argument(s), but received {}",
                        top_level_argument_count(&input)
                    ),
                    call_span,
                ));
            }
            return Err(error(
                format!("no matching branch for macro `{name}`"),
                call_span,
            ));
        };
        let substituted = expand_template(&arm.template, &bindings, None)?
            .into_iter()
            .map(|mut token| {
                // Native macro templates are synthetic and therefore carry an
                // empty span. Give their generated callee and punctuation the
                // invocation span so separate expansions retain distinct
                // expression identities in later semantic side tables.
                if token.span == Span::default() {
                    token.span = call_span;
                }
                token
            })
            .collect::<Vec<_>>();
        stack.push(name);
        let result = expand_sequence(&substituted, definitions, stack);
        stack.pop();
        output.extend(result?);
        cursor = stream.cursor_at(next);
    }
    Ok(output)
}

fn validate_format_invocation(
    name: &str,
    input: &[Token],
    call_span: Span,
) -> Result<(), ParseError> {
    let arguments = top_level_arguments(input);
    if arguments.is_empty() {
        return if name == "println" {
            Ok(())
        } else {
            Err(error("macro `print` requires a format string", call_span))
        };
    }
    let [
        Token {
            kind: TokenKind::String(format),
            span,
        },
    ] = arguments[0]
    else {
        return Err(error(
            format!("macro `{name}` requires a string literal as its first argument"),
            arguments[0].first().map_or(call_span, |token| token.span),
        ));
    };
    let pieces = crate::format::parse_format_string(format)
        .map_err(|format_error| error(format_error.message, *span))?;
    let value_count = arguments.len() - 1;
    let mut used = vec![false; value_count];
    for piece in pieces {
        let crate::format::FormatPiece::Placeholder { argument, .. } = piece else {
            continue;
        };
        let Some(slot) = used.get_mut(argument) else {
            return Err(error(
                format!(
                    "format placeholder references argument {argument}, but only {value_count} value argument(s) were supplied"
                ),
                *span,
            ));
        };
        *slot = true;
    }
    if let Some(unused) = used.iter().position(|used| !used) {
        return Err(error(
            format!("format argument {} is never used", unused + 1),
            arguments[unused + 1]
                .first()
                .map_or(call_span, |token| token.span),
        ));
    }
    Ok(())
}

fn top_level_arguments(tokens: &[Token]) -> Vec<&[Token]> {
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut arguments = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LeftParen | TokenKind::LeftBrace | TokenKind::LeftBracket => depth += 1,
            TokenKind::RightParen | TokenKind::RightBrace | TokenKind::RightBracket => {
                depth = depth.saturating_sub(1);
            }
            TokenKind::Comma if depth == 0 => {
                arguments.push(&tokens[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    arguments.push(&tokens[start..]);
    arguments
}

pub(super) fn invocation_input(
    tokens: &[Token],
    opening_paren: usize,
    call_span: Span,
) -> Result<(Vec<Token>, usize), ParseError> {
    let (input, next) = slice_delimited(
        tokens,
        opening_paren + 1,
        &TokenKind::LeftParen,
        &TokenKind::RightParen,
        call_span,
    )?;
    Ok((input.to_vec(), next))
}
