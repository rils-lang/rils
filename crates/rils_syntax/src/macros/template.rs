use super::*;
use crate::cursor::TokenStream;

pub(super) fn expand_template(
    template: &[Token],
    bindings: &Bindings,
    iteration: Option<usize>,
) -> Result<Vec<Token>, ParseError> {
    let mut output = Vec::new();
    let stream = TokenStream::new(template.to_vec());
    let mut cursor = stream.cursor();
    while let Some(token) = cursor.peek() {
        let mut current = cursor.position();
        if !matches!(token.kind, TokenKind::Dollar) {
            output.push(token.clone());
            cursor = cursor.advance().expect("cursor token was present").1;
            continue;
        }
        let span = template[current].span;
        current += 1;
        if matches!(
            template.get(current).map(|token| &token.kind),
            Some(TokenKind::LeftParen)
        ) {
            current += 1;
            let (inner, next) = slice_delimited(
                template,
                current,
                &TokenKind::LeftParen,
                &TokenKind::RightParen,
                span,
            )?;
            let (separator, one_or_more, next) = repetition_suffix(template, next, span)?;
            let count = repetition_count(&inner, bindings, span)?;
            if one_or_more && count == 0 {
                return Err(error(
                    "`+` macro expansion requires at least one value",
                    span,
                ));
            }
            for index in 0..count {
                if index > 0
                    && let Some(separator) = &separator
                {
                    output.push(Token::new(separator.clone(), span));
                }
                output.extend(expand_template(&inner, bindings, Some(index))?);
            }
            current = next;
            cursor = stream.cursor_at(current);
            continue;
        }
        let (name, name_span) =
            expect_identifier(template, &mut current, "expected capture name after `$`")?;
        if let Some(values) = bindings.repeated.get(&name) {
            let index = iteration.ok_or_else(|| {
                error(
                    format!("repeated capture `${name}` used outside repetition"),
                    name_span,
                )
            })?;
            output.extend(values.get(index).cloned().ok_or_else(|| {
                error(
                    format!("repeated capture `${name}` has inconsistent length"),
                    name_span,
                )
            })?);
        } else if let Some(value) = bindings.single.get(&name) {
            output.extend(value.iter().cloned());
        } else {
            return Err(error(
                format!("unknown macro parameter or capture `${name}`"),
                name_span,
            ));
        }
        cursor = stream.cursor_at(current);
    }
    Ok(output)
}

pub(super) fn repetition_count(
    template: &[Token],
    bindings: &Bindings,
    span: Span,
) -> Result<usize, ParseError> {
    let mut names = Vec::new();
    template_capture_names(template, &mut names)?;
    let mut count = None;
    for name in names {
        let Some(values) = bindings.repeated.get(&name) else {
            continue;
        };
        if count.is_some_and(|count| count != values.len()) {
            return Err(error(
                "macro repetition captures have inconsistent lengths",
                span,
            ));
        }
        count = Some(values.len());
    }
    count.ok_or_else(|| error("macro expansion repetition has no repeated capture", span))
}

pub(super) fn template_capture_names(
    tokens: &[Token],
    names: &mut Vec<String>,
) -> Result<(), ParseError> {
    let stream = TokenStream::new(tokens.to_vec());
    let mut cursor = stream.cursor();
    while let Some(token) = cursor.peek() {
        let mut current = cursor.position();
        if !matches!(token.kind, TokenKind::Dollar) {
            cursor = cursor.advance().expect("cursor token was present").1;
            continue;
        }
        let span = token.span;
        current += 1;
        if matches!(
            tokens.get(current).map(|token| &token.kind),
            Some(TokenKind::LeftParen)
        ) {
            current += 1;
            let (inner, next) = slice_delimited(
                tokens,
                current,
                &TokenKind::LeftParen,
                &TokenKind::RightParen,
                span,
            )?;
            template_capture_names(&inner, names)?;
            let (_, _, next) = repetition_suffix(tokens, next, span)?;
            current = next;
            cursor = stream.cursor_at(current);
        } else {
            let (name, _) =
                expect_identifier(tokens, &mut current, "expected capture name after `$`")?;
            names.push(name);
            cursor = stream.cursor_at(current);
        }
    }
    Ok(())
}
