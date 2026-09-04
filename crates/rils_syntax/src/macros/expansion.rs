use super::*;
use crate::{
    cursor::TokenStream,
    token_tree::{Delimiter, TokenTree},
};

pub(super) fn expand_sequence(
    tokens: &[Token],
    definitions: &HashMap<String, MacroTemplate>,
    stack: &mut Vec<String>,
) -> Result<Vec<Token>, ParseError> {
    let mut output = Vec::new();
    let stream = TokenStream::new(tokens.to_vec())
        .map_err(|span| error("unterminated delimited token tree", span))?;
    let mut cursor = stream.tree_cursor();
    while let Some(tree) = cursor.first() {
        if let TokenTree::Group {
            open,
            children,
            close,
            ..
        } = tree
        {
            // Macro invocations can occur inside function/block bodies and
            // nested expression groups.  Walk each group's child stream
            // recursively while retaining its delimiters in the output.
            let mut nested = Vec::new();
            for child in children.iter() {
                child.flatten_into(&mut nested);
            }
            let expanded = expand_sequence(&nested, definitions, stack)?;
            output.push(open.clone());
            output.extend(expanded);
            output.push(close.clone());
            cursor = cursor.step().expect("cursor tree was present").1;
            continue;
        }
        let invocation = match tree {
            TokenTree::Token(Token {
                kind: TokenKind::Identifier(name),
                span,
            }) => {
                let Some((_, after_identifier)) = cursor.step() else {
                    return Err(error("invalid macro invocation", *span));
                };
                match after_identifier.step() {
                    Some((bang, after_bang)) => match (bang, after_bang.first()) {
                        (
                            TokenTree::Token(Token {
                                kind: TokenKind::Bang,
                                ..
                            }),
                            Some(TokenTree::Group {
                                delimiter: Delimiter::Parenthesis,
                                ..
                            }),
                        ) => Some((name.clone(), *span)),
                        _ => None,
                    },
                    None => None,
                }
            }
            _ => None,
        };
        let Some((name, call_span)) = invocation else {
            tree.flatten_into(&mut output);
            cursor = cursor.step().expect("cursor tree was present").1;
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

        let (_, after_bang) = cursor
            .step()
            .expect("identifier was present")
            .1
            .step()
            .expect("bang was present");
        let (_, _, input_cursor, next) = after_bang.group().expect("invocation group was present");
        let input_stream = TokenStream::from_trees(input_cursor.remaining());
        let input = input_stream.flatten();
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
        cursor = next;
    }
    Ok(output)
}

fn validate_format_invocation(
    name: &str,
    input: &[Token],
    call_span: Span,
) -> Result<(), ParseError> {
    let stream = TokenStream::new(input.to_vec())
        .map_err(|span| error("unterminated delimited token tree", span))?;
    let arguments = top_level_arguments(&stream);
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
    ] = arguments[0].as_slice()
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

fn top_level_arguments(stream: &TokenStream) -> Vec<Vec<Token>> {
    let trees = stream.trees();
    if trees.is_empty() {
        return Vec::new();
    }
    let mut arguments = Vec::new();
    let mut current = Vec::new();
    for tree in trees {
        if matches!(
            tree,
            TokenTree::Token(Token {
                kind: TokenKind::Comma,
                ..
            })
        ) {
            arguments.push(std::mem::take(&mut current));
        } else {
            tree.flatten_into(&mut current);
        }
    }
    arguments.push(current);
    arguments
}
