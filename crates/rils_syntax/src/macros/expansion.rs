use super::*;
use crate::{
    cursor::TokenStream,
    token_tree::{Delimiter, TokenTree},
};

pub(super) fn expand_sequence(
    stream: &TokenStream,
    definitions: &HashMap<String, MacroTemplate>,
    stack: &mut Vec<String>,
) -> Result<TokenStream, ParseError> {
    let trees = stream.trees();
    let mut output = Vec::new();
    let mut current = 0;
    while current < trees.len() {
        let tree = &trees[current];
        if let TokenTree::Group {
            delimiter,
            open,
            children,
            close,
        } = tree
        {
            let expanded = expand_sequence(&TokenStream::from_trees(children), definitions, stack)?;
            output.push(TokenTree::Group {
                delimiter: *delimiter,
                open: open.clone(),
                children: expanded.trees().to_vec().into_boxed_slice(),
                close: close.clone(),
            });
            current += 1;
            continue;
        }
        let Some((name, call_span)) = (match tree {
            TokenTree::Token(Token {
                kind: TokenKind::Identifier(name),
                span,
            }) => match (trees.get(current + 1), trees.get(current + 2)) {
                (
                    Some(TokenTree::Token(Token {
                        kind: TokenKind::Bang,
                        ..
                    })),
                    Some(TokenTree::Group {
                        delimiter: Delimiter::Parenthesis,
                        ..
                    }),
                ) => Some((name.clone(), *span)),
                _ => None,
            },
            _ => None,
        }) else {
            output.push(tree.clone());
            current += 1;
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
        let TokenTree::Group { children, .. } = &trees[current + 2] else {
            unreachable!()
        };
        let input_stream = TokenStream::from_trees(children);
        if matches!(name.as_str(), "print" | "println") {
            validate_format_invocation(&name, &input_stream, call_span)?;
        }
        let Some((arm, bindings)) = definition
            .arms
            .iter()
            .find_map(|arm| match_arm(&arm.matcher, &input_stream).map(|bindings| (arm, bindings)))
        else {
            if let [arm] = definition.arms.as_slice()
                && let Some(expected) = arm.legacy_arity
            {
                return Err(error(
                    format!(
                        "macro `{name}` expects {expected} argument(s), but received {}",
                        top_level_argument_count(&input_stream)
                    ),
                    call_span,
                ));
            }
            return Err(error(
                format!("no matching branch for macro `{name}`"),
                call_span,
            ));
        };
        let substituted = expand_template(&arm.template, &bindings, None)?;
        let substituted = fix_spans(substituted, call_span);
        stack.push(name);
        let result = expand_sequence(&substituted, definitions, stack);
        stack.pop();
        output.extend(result?.trees().iter().cloned());
        current += 3;
    }
    Ok(TokenStream::from_trees_owned(output))
}

fn fix_spans(stream: TokenStream, span: Span) -> TokenStream {
    fn visit(tree: &mut TokenTree, span: Span) {
        match tree {
            TokenTree::Token(token) => {
                if token.span == Span::default() {
                    token.span = span
                }
            }
            TokenTree::Group {
                open,
                children,
                close,
                ..
            } => {
                if open.span == Span::default() {
                    open.span = span;
                }
                if close.span == Span::default() {
                    close.span = span;
                }
                for child in children.iter_mut() {
                    visit(child, span);
                }
            }
        }
    }
    let trees = stream
        .trees()
        .iter()
        .cloned()
        .map(|mut tree| {
            visit(&mut tree, span);
            tree
        })
        .collect();
    TokenStream::from_trees_owned(trees)
}

fn validate_format_invocation(
    name: &str,
    input_stream: &TokenStream,
    call_span: Span,
) -> Result<(), ParseError> {
    let arguments = top_level_arguments(input_stream);
    if arguments.is_empty() {
        return if name == "println" {
            Ok(())
        } else {
            Err(error("macro `print` requires a format string", call_span))
        };
    }
    let [
        TokenTree::Token(Token {
            kind: TokenKind::String(format),
            span,
        }),
    ] = arguments[0].trees()
    else {
        return Err(error(
            format!("macro `{name}` requires a string literal as its first argument"),
            arguments[0].trees().first().map_or(call_span, tree_span),
        ));
    };
    let pieces = crate::format::parse_format_string(format).map_err(|e| error(e.message, *span))?;
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
                .trees()
                .first()
                .map_or(call_span, tree_span),
        ));
    }
    Ok(())
}

fn top_level_arguments(stream: &TokenStream) -> Vec<TokenStream> {
    let mut arguments = Vec::new();
    let mut current = Vec::new();
    for tree in stream.trees() {
        if matches!(
            tree,
            TokenTree::Token(Token {
                kind: TokenKind::Comma,
                ..
            })
        ) {
            arguments.push(TokenStream::from_trees_owned(std::mem::take(&mut current)));
        } else {
            current.push(tree.clone());
        }
    }
    if !current.is_empty() || !arguments.is_empty() {
        arguments.push(TokenStream::from_trees_owned(current));
    }
    arguments
}

fn tree_span(tree: &TokenTree) -> Span {
    match tree {
        TokenTree::Token(token) => token.span,
        TokenTree::Group { open, close, .. } => open.span.merge(close.span),
    }
}
