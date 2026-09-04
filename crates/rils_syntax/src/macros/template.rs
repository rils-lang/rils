use super::*;
use crate::{
    cursor::TokenStream,
    token_tree::{Delimiter, TokenTree},
};

pub(super) fn expand_template(
    stream: &TokenStream,
    bindings: &Bindings,
    iteration: Option<usize>,
) -> Result<TokenStream, ParseError> {
    Ok(TokenStream::from_trees_owned(expand_trees(
        stream.trees(),
        bindings,
        iteration,
    )?))
}

fn expand_trees(
    trees: &[TokenTree],
    bindings: &Bindings,
    iteration: Option<usize>,
) -> Result<Vec<TokenTree>, ParseError> {
    let mut output = Vec::new();
    let mut current = 0;
    while current < trees.len() {
        let TokenTree::Token(token) = &trees[current] else {
            let TokenTree::Group {
                delimiter,
                open,
                children,
                close,
            } = &trees[current]
            else {
                unreachable!()
            };
            output.push(TokenTree::Group {
                delimiter: *delimiter,
                open: open.clone(),
                children: expand_trees(children, bindings, iteration)?.into_boxed_slice(),
                close: close.clone(),
            });
            current += 1;
            continue;
        };
        if !matches!(token.kind, TokenKind::Dollar) {
            output.push(TokenTree::Token(token.clone()));
            current += 1;
            continue;
        }
        let span = token.span;
        let next = current + 1;
        if let Some(TokenTree::Group {
            delimiter: Delimiter::Parenthesis,
            children,
            ..
        }) = trees.get(next)
        {
            let (separator, one_or_more, after) = repetition_suffix_trees(trees, next + 1, span)?;
            let inner = TokenStream::from_trees(children);
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
                    output.push(TokenTree::Token(Token::new(separator.clone(), span)));
                }
                output.extend(expand_trees(children, bindings, Some(index))?);
            }
            current = after;
            continue;
        }
        let Some(TokenTree::Token(Token {
            kind: TokenKind::Identifier(name),
            span: name_span,
        })) = trees.get(next)
        else {
            return Err(error("expected capture name after `$`", span));
        };
        let name = name.clone();
        let name_span = *name_span;
        if let Some(values) = bindings.repeated.get(&name) {
            let index = iteration.ok_or_else(|| {
                error(
                    format!("repeated capture `${name}` used outside repetition"),
                    name_span,
                )
            })?;
            let value = values.get(index).ok_or_else(|| {
                error(
                    format!("repeated capture `${name}` has inconsistent length"),
                    name_span,
                )
            })?;
            output.extend(value.trees().iter().cloned());
        } else if let Some(value) = bindings.single.get(&name) {
            output.extend(value.trees().iter().cloned());
        } else {
            return Err(error(
                format!("unknown macro parameter or capture `${name}`"),
                name_span,
            ));
        }
        current += 2;
    }
    Ok(output)
}

fn repetition_suffix_trees(
    trees: &[TokenTree],
    current: usize,
    span: Span,
) -> Result<(Option<TokenKind>, bool, usize), ParseError> {
    fn token_kind(tree: Option<&TokenTree>) -> Option<&TokenKind> {
        match tree {
            Some(TokenTree::Token(token)) => Some(&token.kind),
            _ => None,
        }
    }
    match token_kind(trees.get(current)) {
        Some(TokenKind::Star) => Ok((None, false, current + 1)),
        Some(TokenKind::Plus) => Ok((None, true, current + 1)),
        Some(separator) => match token_kind(trees.get(current + 1)) {
            Some(TokenKind::Star) => Ok((Some(separator.clone()), false, current + 2)),
            Some(TokenKind::Plus) => Ok((Some(separator.clone()), true, current + 2)),
            _ => Err(error("expected `*` or `+` after macro repetition", span)),
        },
        None => Err(error("expected `*` or `+` after macro repetition", span)),
    }
}

pub(super) fn repetition_count(
    template: &TokenStream,
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
    stream: &TokenStream,
    names: &mut Vec<String>,
) -> Result<(), ParseError> {
    fn visit(trees: &[TokenTree], names: &mut Vec<String>) -> Result<(), ParseError> {
        let mut current = 0;
        while current < trees.len() {
            match &trees[current] {
                TokenTree::Group { children, .. } => {
                    visit(children, names)?;
                    current += 1;
                }
                TokenTree::Token(token) if matches!(token.kind, TokenKind::Dollar) => {
                    if let Some(TokenTree::Group { children, .. }) = trees.get(current + 1) {
                        visit(children, names)?;
                        current += 2;
                    } else if let Some(TokenTree::Token(Token {
                        kind: TokenKind::Identifier(name),
                        ..
                    })) = trees.get(current + 1)
                    {
                        names.push(name.clone());
                        current += 2;
                    } else {
                        return Err(error("expected capture name after `$`", token.span));
                    }
                }
                TokenTree::Token(_) => current += 1,
            }
        }
        Ok(())
    }
    visit(stream.trees(), names)
}
