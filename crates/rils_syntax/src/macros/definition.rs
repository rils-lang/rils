use super::*;
use crate::token_tree::Delimiter;

pub(super) fn collect_definitions(
    tokens: Vec<Token>,
    native_macros: &[NativeMacroDefinition],
) -> Result<CollectedMacros, ParseError> {
    let mut definitions = native_macros
        .iter()
        .map(|definition| {
            (
                definition.name.to_owned(),
                forwarding_template(definition.target),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut symbols = Vec::new();
    let mut output = Vec::new();
    let stream = crate::cursor::TokenStream::new(tokens.clone())
        .map_err(|span| error("unterminated delimited token tree", span))?;
    let mut current = 0;
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;

    while current < tokens.len() {
        if matches!(tokens[current].kind, TokenKind::Macro) {
            if brace_depth != 0 || paren_depth != 0 {
                return Err(error(
                    "macro declarations are only allowed at the top level",
                    tokens[current].span,
                ));
            }
            let start = tokens[current].span;
            current += 1;
            let (name, name_span) =
                expect_identifier(&stream, &mut current, "expected macro name")?;
            let arms = if take(&stream, &mut current, &TokenKind::LeftParen) {
                vec![legacy_arm(&stream, &mut current, start)?]
            } else if take(&stream, &mut current, &TokenKind::LeftBrace) {
                branching_arms(&stream, &mut current, start)?
            } else {
                return Err(error(
                    "expected `(` or `{` after macro name",
                    token_span(tokens.get(current)),
                ));
            };

            if definitions
                .insert(name.clone(), MacroTemplate { arms })
                .is_some()
            {
                return Err(error(format!("duplicate macro `{name}`"), name_span));
            }
            symbols.push(MacroSymbol {
                name,
                name_span,
                references: Vec::new(),
            });
            continue;
        }

        match tokens[current].kind {
            TokenKind::LeftBrace => brace_depth += 1,
            TokenKind::RightBrace => brace_depth = brace_depth.saturating_sub(1),
            TokenKind::LeftParen => paren_depth += 1,
            TokenKind::RightParen => paren_depth = paren_depth.saturating_sub(1),
            _ => {}
        }
        output.push(tokens[current].clone());
        current += 1;
    }

    let output = crate::cursor::TokenStream::new(output)
        .map_err(|span| error("unterminated delimited token tree", span))?;
    Ok(CollectedMacros {
        tokens: output,
        definitions,
        symbols,
    })
}

pub(super) fn forwarding_template(target: &str) -> MacroTemplate {
    let span = Span::default();
    let capture = "arguments".to_owned();
    MacroTemplate {
        arms: vec![MacroArm {
            matcher: vec![MatcherElement::Repeat {
                elements: vec![MatcherElement::Capture {
                    name: capture.clone(),
                    kind: FragmentKind::Expr,
                }],
                separator: Some(TokenKind::Comma),
                one_or_more: false,
            }],
            template: crate::cursor::TokenStream::new(vec![
                Token::new(TokenKind::Identifier(target.to_owned()), span),
                Token::new(TokenKind::LeftParen, span),
                Token::new(TokenKind::Dollar, span),
                Token::new(TokenKind::LeftParen, span),
                Token::new(TokenKind::Dollar, span),
                Token::new(TokenKind::Identifier(capture), span),
                Token::new(TokenKind::RightParen, span),
                Token::new(TokenKind::Comma, span),
                Token::new(TokenKind::Star, span),
                Token::new(TokenKind::RightParen, span),
            ])
            .expect("synthetic native macro template delimiters are balanced"),
            legacy_arity: None,
        }],
    }
}

pub(super) fn legacy_arm(
    stream: &crate::cursor::TokenStream,
    current: &mut usize,
    start: Span,
) -> Result<MacroArm, ParseError> {
    let mut matcher = Vec::new();
    let mut names = HashSet::new();
    if !take(stream, current, &TokenKind::RightParen) {
        loop {
            expect(
                stream,
                current,
                &TokenKind::Dollar,
                "expected `$` before macro parameter",
            )?;
            let (name, span) = expect_identifier(stream, current, "expected macro parameter name")?;
            if !names.insert(name.clone()) {
                return Err(error(format!("duplicate macro parameter `${name}`"), span));
            }
            matcher.push(MatcherElement::Capture {
                name,
                kind: FragmentKind::Tokens,
            });
            if take(stream, current, &TokenKind::RightParen) {
                break;
            }
            expect(
                stream,
                current,
                &TokenKind::Comma,
                "expected `,` between macro parameters",
            )?;
            matcher.push(MatcherElement::Token(TokenKind::Comma));
        }
    }
    expect(
        stream,
        current,
        &TokenKind::LeftBrace,
        "expected `{` before macro body",
    )?;
    let template = delimited(
        stream,
        current,
        TokenKind::LeftBrace,
        TokenKind::RightBrace,
        start,
    )?;
    validate_template(&template, &matcher)?;
    Ok(MacroArm {
        matcher,
        template,
        legacy_arity: Some(names.len()),
    })
}

pub(super) fn branching_arms(
    stream: &crate::cursor::TokenStream,
    current: &mut usize,
    declaration_span: Span,
) -> Result<Vec<MacroArm>, ParseError> {
    let mut arms = Vec::new();
    while !take(stream, current, &TokenKind::RightBrace) {
        let start = stream
            .cursor_at(*current)
            .peek()
            .map_or(declaration_span, |token| token.span);
        expect(
            stream,
            current,
            &TokenKind::LeftParen,
            "expected `(` before macro matcher",
        )?;
        let matcher_tokens = delimited(
            stream,
            current,
            TokenKind::LeftParen,
            TokenKind::RightParen,
            start,
        )?;
        let mut names = HashSet::new();
        let matcher = parse_matcher(&matcher_tokens, false, &mut names)?;
        expect(
            stream,
            current,
            &TokenKind::FatArrow,
            "expected `=>` after macro matcher",
        )?;
        expect(
            stream,
            current,
            &TokenKind::LeftBrace,
            "expected `{` before macro expansion",
        )?;
        let template = delimited(
            stream,
            current,
            TokenKind::LeftBrace,
            TokenKind::RightBrace,
            start,
        )?;
        validate_template(&template, &matcher)?;
        arms.push(MacroArm {
            matcher,
            template,
            legacy_arity: None,
        });
        take(stream, current, &TokenKind::Comma);
        take(stream, current, &TokenKind::Semicolon);
        if stream.cursor_at(*current).peek().is_none() {
            return Err(error("unterminated macro declaration", declaration_span));
        }
    }
    if arms.is_empty() {
        return Err(error(
            "macro declaration requires at least one matching branch",
            declaration_span,
        ));
    }
    Ok(arms)
}

pub(super) fn parse_matcher(
    stream: &crate::cursor::TokenStream,
    inside_repeat: bool,
    names: &mut HashSet<String>,
) -> Result<Vec<MatcherElement>, ParseError> {
    parse_matcher_trees(stream.trees(), inside_repeat, names)
}

fn parse_matcher_trees(
    trees: &[crate::token_tree::TokenTree],
    inside_repeat: bool,
    names: &mut HashSet<String>,
) -> Result<Vec<MatcherElement>, ParseError> {
    let mut elements = Vec::new();
    let mut current = 0;
    while let Some(tree) = trees.get(current) {
        let crate::token_tree::TokenTree::Token(token) = tree else {
            let crate::token_tree::TokenTree::Group {
                delimiter,
                children,
                ..
            } = tree
            else {
                unreachable!()
            };
            elements.push(MatcherElement::Group {
                delimiter: *delimiter,
                elements: parse_matcher_trees(children, inside_repeat, names)?,
            });
            current += 1;
            continue;
        };
        if !matches!(token.kind, TokenKind::Dollar) {
            elements.push(MatcherElement::Token(token.kind.clone()));
            current += 1;
            continue;
        }
        let dollar_span = token.span;
        current += 1;
        if let Some(crate::token_tree::TokenTree::Group {
            delimiter: Delimiter::Parenthesis,
            children,
            ..
        }) = trees.get(current)
        {
            if inside_repeat {
                return Err(error(
                    "nested macro repetitions are not supported",
                    dollar_span,
                ));
            }
            let inner = parse_matcher_trees(children, true, names)?;
            if !contains_capture(&inner) {
                return Err(error(
                    "macro repetition must contain at least one capture",
                    dollar_span,
                ));
            }
            current += 1;
            let (separator, one_or_more, next) =
                repetition_suffix_trees(trees, current, dollar_span)?;
            current = next;
            elements.push(MatcherElement::Repeat {
                elements: inner,
                separator,
                one_or_more,
            });
            continue;
        }

        let Some(crate::token_tree::TokenTree::Token(Token {
            kind: TokenKind::Identifier(name),
            span,
        })) = trees.get(current)
        else {
            return Err(error("expected capture name after `$`", dollar_span));
        };
        let name = name.clone();
        let span = *span;
        current += 1;
        if !names.insert(name.clone()) {
            return Err(error(format!("duplicate macro capture `${name}`"), span));
        }
        if !matches!(
            trees.get(current),
            Some(crate::token_tree::TokenTree::Token(Token {
                kind: TokenKind::Colon,
                ..
            }))
        ) {
            return Err(error("expected fragment type after macro capture", span));
        }
        current += 1;
        let Some(crate::token_tree::TokenTree::Token(Token {
            kind: TokenKind::Identifier(fragment),
            span: fragment_span,
        })) = trees.get(current)
        else {
            return Err(error("expected macro fragment type", span));
        };
        let fragment = fragment.clone();
        let fragment_span = *fragment_span;
        current += 1;
        let kind = match fragment.as_str() {
            "expr" => FragmentKind::Expr,
            "lit" => FragmentKind::Lit,
            "ident" => FragmentKind::Ident,
            _ => {
                return Err(error(
                    format!("unsupported macro fragment type `{fragment}`"),
                    fragment_span,
                ));
            }
        };
        elements.push(MatcherElement::Capture { name, kind });
    }
    Ok(elements)
}

fn repetition_suffix_trees(
    trees: &[crate::token_tree::TokenTree],
    current: usize,
    span: Span,
) -> Result<(Option<TokenKind>, bool, usize), ParseError> {
    fn token_kind(tree: Option<&crate::token_tree::TokenTree>) -> Option<&TokenKind> {
        match tree {
            Some(crate::token_tree::TokenTree::Token(token)) => Some(&token.kind),
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

pub(super) fn repetition_suffix(
    stream: &crate::cursor::TokenStream,
    current: usize,
    span: Span,
) -> Result<(Option<TokenKind>, bool, usize), ParseError> {
    let cursor = stream.cursor_at(current);
    match cursor.peek().map(|token| &token.kind) {
        Some(TokenKind::Star) => Ok((None, false, current + 1)),
        Some(TokenKind::Plus) => Ok((None, true, current + 1)),
        Some(separator) => match cursor
            .advance()
            .and_then(|(_, next)| next.peek())
            .map(|token| &token.kind)
        {
            Some(TokenKind::Star) => Ok((Some(separator.clone()), false, current + 2)),
            Some(TokenKind::Plus) => Ok((Some(separator.clone()), true, current + 2)),
            _ => Err(error("expected `*` or `+` after macro repetition", span)),
        },
        None => Err(error("expected `*` or `+` after macro repetition", span)),
    }
}

pub(super) fn validate_template(
    template: &crate::cursor::TokenStream,
    matcher: &[MatcherElement],
) -> Result<(), ParseError> {
    let mut single = HashSet::new();
    let mut repeated = HashSet::new();
    collect_capture_names(matcher, false, &mut single, &mut repeated);
    validate_template_tokens(template, false, &single, &repeated)
}

pub(super) fn validate_template_tokens(
    stream: &crate::cursor::TokenStream,
    inside_repeat: bool,
    single: &HashSet<String>,
    repeated: &HashSet<String>,
) -> Result<(), ParseError> {
    let mut cursor = stream.cursor();
    while let Some(token) = cursor.peek() {
        let mut current = cursor.position();
        if !matches!(token.kind, TokenKind::Dollar) {
            cursor = cursor.advance().expect("cursor token was present").1;
            continue;
        }
        let span = token.span;
        current += 1;
        if stream.cursor_at(current).check(&TokenKind::LeftParen) {
            current += 1;
            let (inner, next) = slice_delimited(
                stream,
                current,
                &TokenKind::LeftParen,
                &TokenKind::RightParen,
                span,
            )?;
            validate_template_tokens(&inner, true, single, repeated)?;
            let (_, _, next) = repetition_suffix(stream, next, span)?;
            current = next;
            cursor = stream.cursor_at(current);
            continue;
        }
        let (name, name_span) =
            expect_identifier(stream, &mut current, "expected capture name after `$`")?;
        if !single.contains(&name) && !repeated.contains(&name) {
            return Err(error(
                format!("unknown macro parameter or capture `${name}`"),
                name_span,
            ));
        }
        cursor = stream.cursor_at(current);
        if repeated.contains(&name) && !inside_repeat {
            return Err(error(
                format!("repeated capture `${name}` must be used inside a repetition"),
                name_span,
            ));
        }
    }
    Ok(())
}

pub(super) fn collect_capture_names(
    elements: &[MatcherElement],
    inside_repeat: bool,
    single: &mut HashSet<String>,
    repeated: &mut HashSet<String>,
) {
    for element in elements {
        match element {
            MatcherElement::Capture { name, .. } if inside_repeat => {
                repeated.insert(name.clone());
            }
            MatcherElement::Capture { name, .. } => {
                single.insert(name.clone());
            }
            MatcherElement::Repeat { elements, .. } => {
                collect_capture_names(elements, true, single, repeated);
            }
            MatcherElement::Group { elements, .. } => {
                collect_capture_names(elements, inside_repeat, single, repeated);
            }
            MatcherElement::Token(_) => {}
        }
    }
}

pub(super) fn contains_capture(elements: &[MatcherElement]) -> bool {
    elements.iter().any(|element| {
        matches!(element, MatcherElement::Capture { .. })
            || matches!(element, MatcherElement::Repeat { elements, .. } if contains_capture(elements))
            || matches!(element, MatcherElement::Group { elements, .. } if contains_capture(elements))
    })
}

pub(super) fn invocation_references(
    stream: &crate::cursor::TokenStream,
) -> HashMap<String, Vec<Span>> {
    let mut references: HashMap<String, Vec<Span>> = HashMap::new();

    fn visit(trees: &[crate::token_tree::TokenTree], references: &mut HashMap<String, Vec<Span>>) {
        for window in trees.windows(3) {
            if let [
                crate::token_tree::TokenTree::Token(Token {
                    kind: TokenKind::Identifier(name),
                    span,
                }),
                crate::token_tree::TokenTree::Token(Token {
                    kind: TokenKind::Bang,
                    ..
                }),
                crate::token_tree::TokenTree::Group {
                    delimiter: crate::token_tree::Delimiter::Parenthesis,
                    ..
                },
            ] = window
            {
                references.entry(name.clone()).or_default().push(*span);
            }
        }
        for tree in trees {
            if let crate::token_tree::TokenTree::Group { children, .. } = tree {
                visit(children, references);
            }
        }
    }

    visit(stream.trees(), &mut references);
    references
}
