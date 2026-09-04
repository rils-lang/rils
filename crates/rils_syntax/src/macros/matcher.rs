use super::*;
use crate::cursor::TokenStream;

pub(super) fn match_arm(matcher: &[MatcherElement], stream: &TokenStream) -> Option<Bindings> {
    let input = stream.flatten();
    let bindings = Bindings::default();
    let (position, bindings) = match_elements(matcher, &input, 0, bindings, false)?;
    (position == input.len()).then_some(bindings)
}

pub(super) fn match_elements(
    elements: &[MatcherElement],
    input: &[Token],
    position: usize,
    bindings: Bindings,
    inside_repeat: bool,
) -> Option<(usize, Bindings)> {
    let Some((first, rest)) = elements.split_first() else {
        return Some((position, bindings));
    };
    match first {
        MatcherElement::Token(expected) => {
            let actual = input.get(position)?;
            token_kinds_equal(&actual.kind, expected).then_some(())?;
            match_elements(rest, input, position + 1, bindings, inside_repeat)
        }
        MatcherElement::Capture { name, kind } => {
            for end in capture_ends(*kind, input, position).into_iter().rev() {
                let mut candidate = bindings.clone();
                if inside_repeat {
                    candidate
                        .repeated
                        .entry(name.clone())
                        .or_default()
                        .push(input[position..end].to_vec());
                } else if candidate
                    .single
                    .insert(name.clone(), input[position..end].to_vec())
                    .is_some()
                {
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
    input: &[Token],
    position: usize,
    mut bindings: Bindings,
) -> Option<(usize, Bindings)> {
    initialize_repeated_bindings(repeated, &mut bindings);
    let mut states = vec![(position, bindings)];
    loop {
        let (start, state) = states.last().cloned()?;
        let item_start = if states.len() > 1 {
            if let Some(separator) = separator {
                let Some(token) = input.get(start) else {
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
            MatcherElement::Token(_) => {}
        }
    }
}

pub(super) fn capture_ends(kind: FragmentKind, input: &[Token], position: usize) -> Vec<usize> {
    match kind {
        FragmentKind::Ident => input
            .get(position)
            .filter(|token| matches!(token.kind, TokenKind::Identifier(_)))
            .map_or_else(Vec::new, |_| vec![position + 1]),
        FragmentKind::Lit => literal_end(input, position).into_iter().collect(),
        FragmentKind::Expr => ((position + 1)..=input.len())
            .filter(|end| is_expression_fragment(&input[position..*end]))
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

pub(super) fn literal_end(input: &[Token], position: usize) -> Option<usize> {
    let token = input.get(position)?;
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
    if matches!(token.kind, TokenKind::LeftParen)
        && matches!(
            input.get(position + 1).map(|token| &token.kind),
            Some(TokenKind::RightParen)
        )
    {
        return Some(position + 2);
    }
    None
}
