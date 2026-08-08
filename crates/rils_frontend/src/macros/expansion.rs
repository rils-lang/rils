use super::*;

pub(super) fn expand_sequence(
    tokens: &[Token],
    definitions: &HashMap<String, MacroTemplate>,
    stack: &mut Vec<String>,
) -> Result<Vec<Token>, ParseError> {
    let mut output = Vec::new();
    let mut current = 0;
    while current < tokens.len() {
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
            output.push(tokens[current].clone());
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

        let (input, next) = invocation_input(tokens, current + 2, call_span)?;
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
        let substituted = expand_template(&arm.template, &bindings, None)?;
        stack.push(name);
        let result = expand_sequence(&substituted, definitions, stack);
        stack.pop();
        output.extend(result?);
        current = next;
    }
    Ok(output)
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
