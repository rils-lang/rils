use super::*;
use crate::cursor::TokenStream;
use crate::token_tree::{Delimiter, TokenTree};

pub(super) fn match_arm(matcher: &[MatcherElement], stream: &TokenStream) -> Option<Bindings> {
    let bindings = Bindings::default();
    let (position, bindings) = match_elements(matcher, stream.trees(), 0, bindings, false)?;
    (position == stream.trees().len()).then_some(bindings)
}

pub(super) fn match_elements(
    elements: &[MatcherElement],
    input: &[TokenTree],
    position: usize,
    bindings: Bindings,
    inside_repeat: bool,
) -> Option<(usize, Bindings)> {
    let Some((first, rest)) = elements.split_first() else {
        return Some((position, bindings));
    };
    match first {
        MatcherElement::Token(expected) => {
            let TokenTree::Token(actual) = input.get(position)? else {
                return None;
            };
            token_kinds_equal(&actual.kind, expected).then_some(())?;
            match_elements(rest, input, position + 1, bindings, inside_repeat)
        }
        MatcherElement::Group {
            delimiter,
            elements,
        } => {
            let TokenTree::Group {
                delimiter: actual,
                children,
                ..
            } = input.get(position)?
            else {
                return None;
            };
            (actual == delimiter).then_some(())?;
            let (end, bindings) = match_elements(elements, children, 0, bindings, inside_repeat)?;
            (end == children.len()).then_some(())?;
            match_elements(rest, input, position + 1, bindings, inside_repeat)
        }
        MatcherElement::Capture { name, kind } => {
            for end in capture_ends(*kind, input, position).into_iter().rev() {
                let mut candidate = bindings.clone();
                let captured = TokenStream::from_trees(&input[position..end]);
                if inside_repeat {
                    candidate
                        .repeated
                        .entry(name.clone())
                        .or_default()
                        .push(captured);
                } else if candidate.single.insert(name.clone(), captured).is_some() {
                    continue;
                }
                if let Some(result) = match_elements(rest, input, end, candidate, inside_repeat) {
                    return Some(result);
                }
            }
            None
        }
        MatcherElement::Repeat {
            elements: repeated,
            separator,
            one_or_more,
        } => match_repetition(
            repeated,
            separator.as_ref(),
            *one_or_more,
            rest,
            input,
            position,
            bindings,
        ),
    }
}

pub(super) fn match_repetition(
    repeated: &[MatcherElement],
    separator: Option<&TokenKind>,
    one_or_more: bool,
    rest: &[MatcherElement],
    input: &[TokenTree],
    position: usize,
    mut bindings: Bindings,
) -> Option<(usize, Bindings)> {
    initialize_repeated_bindings(repeated, &mut bindings);
    let mut states = vec![(position, bindings)];
    loop {
        let (start, state) = states.last().cloned()?;
        let item_start = if states.len() > 1 {
            if let Some(separator) = separator {
                let Some(TokenTree::Token(token)) = input.get(start) else {
                    break;
                };
                if !token_kinds_equal(&token.kind, separator) {
                    break;
                }
                start + 1
            } else {
                start
            }
        } else {
            start
        };
        let Some((end, next)) = match_elements(repeated, input, item_start, state, true) else {
            break;
        };
        if end == item_start {
            break;
        }
        states.push((end, next));
    }

    let minimum = usize::from(one_or_more);
    for (count, (end, state)) in states.into_iter().enumerate().rev() {
        if count < minimum {
            continue;
        }
        if let Some(result) = match_elements(rest, input, end, state, false) {
            return Some(result);
        }
    }
    None
}

pub(super) fn initialize_repeated_bindings(elements: &[MatcherElement], bindings: &mut Bindings) {
    for element in elements {
        match element {
            MatcherElement::Capture { name, .. } => {
                bindings.repeated.entry(name.clone()).or_default();
            }
            MatcherElement::Repeat { elements, .. } => {
                initialize_repeated_bindings(elements, bindings);
            }
            MatcherElement::Group { elements, .. } => {
                initialize_repeated_bindings(elements, bindings);
            }
            MatcherElement::Token(_) => {}
        }
    }
}

pub(super) fn capture_ends(kind: FragmentKind, input: &[TokenTree], position: usize) -> Vec<usize> {
    match kind {
        FragmentKind::Ident => input
            .get(position)
            .and_then(|tree| match tree {
                TokenTree::Token(token) if matches!(token.kind, TokenKind::Identifier(_)) => {
                    Some(position + 1)
                }
                _ => None,
            })
            .into_iter()
            .collect(),
        FragmentKind::Lit => literal_end(input, position).into_iter().collect(),
        FragmentKind::Expr => ((position + 1)..=input.len())
            .filter(|end| {
                let stream = TokenStream::from_trees(&input[position..*end]);
                is_expression_fragment(&stream.flatten())
            })
            .collect(),
        FragmentKind::Tokens => ((position + 1)..=input.len()).collect(),
    }
}

pub(super) fn top_level_argument_count(stream: &crate::cursor::TokenStream) -> usize {
    let trees = stream.trees();
    if trees.is_empty() {
        return 0;
    }
    let mut count = 1usize;
    for tree in trees {
        if matches!(
            tree,
            crate::token_tree::TokenTree::Token(Token {
                kind: TokenKind::Comma,
                ..
            })
        ) {
            count += 1;
        }
    }
    count
}

pub(super) fn literal_end(input: &[TokenTree], position: usize) -> Option<usize> {
    let TokenTree::Token(token) = input.get(position)? else {
        return None;
    };
    if matches!(
        token.kind,
        TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::I8(_)
            | TokenKind::I16(_)
            | TokenKind::I32(_)
            | TokenKind::I64(_)
            | TokenKind::I128(_)
            | TokenKind::Isize(_)
            | TokenKind::U8(_)
            | TokenKind::U16(_)
            | TokenKind::U32(_)
            | TokenKind::U64(_)
            | TokenKind::U128(_)
            | TokenKind::Usize(_)
            | TokenKind::F32(_)
            | TokenKind::F64(_)
            | TokenKind::Char(_)
            | TokenKind::String(_)
            | TokenKind::True
            | TokenKind::False
    ) {
        return Some(position + 1);
    }
    if let TokenTree::Group {
        delimiter: Delimiter::Parenthesis,
        children,
        ..
    } = input.get(position)?
        && children.is_empty()
    {
        return Some(position + 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, macros::definition::parse_matcher};
    use std::collections::HashSet;

    #[test]
    fn matches_groups_without_flattening_the_input_boundary() {
        let pattern = lex("($value:expr)").unwrap();
        let mut names = HashSet::new();
        let matcher = parse_matcher(&pattern, false, &mut names).unwrap();
        let input = TokenStream::new(lex("(1 + 2)").unwrap()).unwrap();
        let bindings = match_arm(&matcher, &input).expect("group matcher should match");
        let captured = bindings.single.get("value").expect("capture");
        assert_eq!(captured.flatten().len(), 3);
    }
}
