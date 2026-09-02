use super::*;

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
                expect_identifier(&tokens, &mut current, "expected macro name")?;
            let arms = if take(&tokens, &mut current, &TokenKind::LeftParen) {
                vec![legacy_arm(&tokens, &mut current, start)?]
            } else if take(&tokens, &mut current, &TokenKind::LeftBrace) {
                branching_arms(&tokens, &mut current, start)?
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
            template: vec![
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
            ],
            legacy_arity: None,
        }],
    }
}

pub(super) fn legacy_arm(
    tokens: &[Token],
    current: &mut usize,
    start: Span,
) -> Result<MacroArm, ParseError> {
    let mut matcher = Vec::new();
    let mut names = HashSet::new();
    if !take(tokens, current, &TokenKind::RightParen) {
        loop {
            expect(
                tokens,
                current,
                &TokenKind::Dollar,
                "expected `$` before macro parameter",
            )?;
            let (name, span) = expect_identifier(tokens, current, "expected macro parameter name")?;
            if !names.insert(name.clone()) {
                return Err(error(format!("duplicate macro parameter `${name}`"), span));
            }
            matcher.push(MatcherElement::Capture {
                name,
                kind: FragmentKind::Tokens,
            });
            if take(tokens, current, &TokenKind::RightParen) {
                break;
            }
            expect(
                tokens,
                current,
                &TokenKind::Comma,
                "expected `,` between macro parameters",
            )?;
            matcher.push(MatcherElement::Token(TokenKind::Comma));
        }
    }
    expect(
        tokens,
        current,
        &TokenKind::LeftBrace,
        "expected `{` before macro body",
    )?;
    let template = delimited(
        tokens,
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
    tokens: &[Token],
    current: &mut usize,
    declaration_span: Span,
) -> Result<Vec<MacroArm>, ParseError> {
    let mut arms = Vec::new();
    while !take(tokens, current, &TokenKind::RightBrace) {
        let start = tokens
            .get(*current)
            .map_or(declaration_span, |token| token.span);
        expect(
            tokens,
            current,
            &TokenKind::LeftParen,
            "expected `(` before macro matcher",
        )?;
        let matcher_tokens = delimited(
            tokens,
            current,
            TokenKind::LeftParen,
            TokenKind::RightParen,
            start,
        )?;
        let mut names = HashSet::new();
        let matcher = parse_matcher(&matcher_tokens, false, &mut names)?;
        expect(
            tokens,
            current,
            &TokenKind::FatArrow,
            "expected `=>` after macro matcher",
        )?;
        expect(
            tokens,
            current,
            &TokenKind::LeftBrace,
            "expected `{` before macro expansion",
        )?;
        let template = delimited(
            tokens,
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
        take(tokens, current, &TokenKind::Comma);
        take(tokens, current, &TokenKind::Semicolon);
        if matches!(tokens.get(*current).map(|token| &token.kind), None) {
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
    tokens: &[Token],
    inside_repeat: bool,
    names: &mut HashSet<String>,
) -> Result<Vec<MatcherElement>, ParseError> {
    let mut elements = Vec::new();
    let mut current = 0;
    while current < tokens.len() {
        if !matches!(tokens[current].kind, TokenKind::Dollar) {
            elements.push(MatcherElement::Token(tokens[current].kind.clone()));
            current += 1;
            continue;
        }

        let dollar_span = tokens[current].span;
        current += 1;
        if matches!(
            tokens.get(current).map(|token| &token.kind),
            Some(TokenKind::LeftParen)
        ) {
            if inside_repeat {
                return Err(error(
                    "nested macro repetitions are not supported",
                    dollar_span,
                ));
            }
            current += 1;
            let (inner_tokens, next) = slice_delimited(
                tokens,
                current,
                &TokenKind::LeftParen,
                &TokenKind::RightParen,
                dollar_span,
            )?;
            current = next;
            let inner = parse_matcher(inner_tokens, true, names)?;
            if !contains_capture(&inner) {
                return Err(error(
                    "macro repetition must contain at least one capture",
                    dollar_span,
                ));
            }
            let (separator, one_or_more, next) = repetition_suffix(tokens, current, dollar_span)?;
            current = next;
            elements.push(MatcherElement::Repeat {
                elements: inner,
                separator,
                one_or_more,
            });
            continue;
        }

        let (name, span) =
            expect_identifier(tokens, &mut current, "expected capture name after `$`")?;
        if !names.insert(name.clone()) {
            return Err(error(format!("duplicate macro capture `${name}`"), span));
        }
        expect(
            tokens,
            &mut current,
            &TokenKind::Colon,
            "expected fragment type after macro capture",
        )?;
        let (fragment, fragment_span) =
            expect_identifier(tokens, &mut current, "expected macro fragment type")?;
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

pub(super) fn repetition_suffix(
    tokens: &[Token],
    current: usize,
    span: Span,
) -> Result<(Option<TokenKind>, bool, usize), ParseError> {
    match tokens.get(current).map(|token| &token.kind) {
        Some(TokenKind::Star) => Ok((None, false, current + 1)),
        Some(TokenKind::Plus) => Ok((None, true, current + 1)),
        Some(separator) => match tokens.get(current + 1).map(|token| &token.kind) {
            Some(TokenKind::Star) => Ok((Some(separator.clone()), false, current + 2)),
            Some(TokenKind::Plus) => Ok((Some(separator.clone()), true, current + 2)),
            _ => Err(error("expected `*` or `+` after macro repetition", span)),
        },
        None => Err(error("expected `*` or `+` after macro repetition", span)),
    }
}

pub(super) fn validate_template(
    template: &[Token],
    matcher: &[MatcherElement],
) -> Result<(), ParseError> {
    let mut single = HashSet::new();
    let mut repeated = HashSet::new();
    collect_capture_names(matcher, false, &mut single, &mut repeated);
    validate_template_tokens(template, false, &single, &repeated)
}

pub(super) fn validate_template_tokens(
    tokens: &[Token],
    inside_repeat: bool,
    single: &HashSet<String>,
    repeated: &HashSet<String>,
) -> Result<(), ParseError> {
    let mut current = 0;
    while current < tokens.len() {
        if !matches!(tokens[current].kind, TokenKind::Dollar) {
            current += 1;
            continue;
        }
        let span = tokens[current].span;
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
            validate_template_tokens(inner, true, single, repeated)?;
            let (_, _, next) = repetition_suffix(tokens, next, span)?;
            current = next;
            continue;
        }
        let (name, name_span) =
            expect_identifier(tokens, &mut current, "expected capture name after `$`")?;
        if !single.contains(&name) && !repeated.contains(&name) {
            return Err(error(
                format!("unknown macro parameter or capture `${name}`"),
                name_span,
            ));
        }
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
            MatcherElement::Token(_) => {}
        }
    }
}

pub(super) fn contains_capture(elements: &[MatcherElement]) -> bool {
    elements.iter().any(|element| {
        matches!(element, MatcherElement::Capture { .. })
            || matches!(element, MatcherElement::Repeat { elements, .. } if contains_capture(elements))
    })
}

pub(super) fn invocation_references(tokens: &[Token]) -> HashMap<String, Vec<Span>> {
    let mut references: HashMap<String, Vec<Span>> = HashMap::new();
    for window in tokens.windows(3) {
        if let [
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
        ] = window
        {
            references.entry(name.clone()).or_default().push(*span);
        }
    }
    references
}
