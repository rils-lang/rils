use super::*;
use rils_frontend::token::TokenKind;

impl Server {
    pub(super) fn signature_help(&self, params: &Value) -> Result<Value, AnyError> {
        let (_, document, offset) = self.document_and_offset(params)?;
        let Some(context) = call_context(&document.text, offset) else {
            return Ok(Value::Null);
        };
        let recovered;
        let current_analysis = if let Some(analysis) = analysis(document) {
            Some(analysis)
        } else {
            recovered = recover_analysis(self, document, offset);
            recovered.as_ref()
        };
        let Some((name, signature)) = current_analysis
            .and_then(|analysis| signature_at_call(analysis, &document.text, context.open))
            .or_else(|| {
                current_analysis.and_then(|analysis| {
                    builtin_signature_at_call(analysis, &document.text, context.open)
                })
            })
            .or_else(|| self.host_signature_at_call(&document.text, context.open))
        else {
            return Ok(Value::Null);
        };
        let parameters = signature.parameters.clone().unwrap_or_default();
        let active_parameter = if parameters.is_empty() {
            0
        } else {
            context.argument.min(parameters.len() - 1)
        };
        let labels = parameters
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        Ok(json!({
            "signatures": [{
                "label": signature_declaration(&name, &signature),
                "parameters": labels.iter().map(|label| json!({ "label": label })).collect::<Vec<_>>()
            }],
            "activeSignature": 0,
            "activeParameter": active_parameter
        }))
    }

    fn host_signature_at_call(
        &self,
        text: &str,
        open: usize,
    ) -> Option<(String, FunctionSignature)> {
        let path = qualified_path_before(text, open)?;
        let resolved = resolve_path_alias(text, &path);
        let signature = self.host_functions.get(&resolved)?.clone();
        let name = resolved.rsplit("::").next()?.to_owned();
        Some((name, signature))
    }
}

fn builtin_signature_at_call(
    analysis: &DocumentAnalysis,
    text: &str,
    open: usize,
) -> Option<(String, FunctionSignature)> {
    let name = identifier_before(text, open)?.to_owned();
    let name_start = text[..open].trim_end().len().checked_sub(name.len())?;
    let dot = name_start.checked_sub(1)?;
    if text.as_bytes().get(dot) != Some(&b'.') {
        return None;
    }
    let receiver_type = analysis
        .expression_types
        .iter()
        .filter(|(span, _)| span.end == dot)
        .max_by_key(|(span, _)| span.start)
        .map(|(_, ty)| ty)
        .or_else(|| {
            let receiver = identifier_before(text, dot)?;
            analysis
                .symbols
                .iter()
                .filter(|symbol| {
                    symbol.name == receiver
                        && symbol.span.start < dot
                        && symbol.inferred_type.is_some()
                })
                .max_by_key(|symbol| symbol.span.start)
                .and_then(|symbol| symbol.inferred_type.as_ref())
        })?;
    if let Type::Integer(integer) = receiver_type
        && let Some(intrinsic) = rils_builtins::integer_method(&name)
    {
        return function_signature(
            name,
            rils_frontend::standard_library::integer_intrinsic_type(intrinsic, *integer),
        );
    }
    let member_type = rils_frontend::standard_library::builtin_member_type(receiver_type, &name)?;
    function_signature(name, member_type)
}

fn function_signature(name: String, member_type: Type) -> Option<(String, FunctionSignature)> {
    let Type::Function {
        parameters,
        return_type,
    } = member_type
    else {
        return None;
    };
    Some((
        name,
        FunctionSignature {
            parameters,
            return_type: *return_type,
        },
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CallContext {
    open: usize,
    argument: usize,
}

fn call_context(text: &str, offset: usize) -> Option<CallContext> {
    let tokens = lex(&text[..floor_char_boundary(text, offset.min(text.len()))]).ok()?;
    let mut stack = Vec::<Delimiter>::new();
    for token in tokens {
        match token.kind {
            TokenKind::LeftParen => stack.push(Delimiter::Call(CallContext {
                open: token.span.start,
                argument: 0,
            })),
            TokenKind::LeftBracket => stack.push(Delimiter::Bracket),
            TokenKind::LeftBrace => stack.push(Delimiter::Brace),
            TokenKind::Comma => {
                if let Some(Delimiter::Call(context)) = stack.last_mut() {
                    context.argument += 1;
                }
            }
            TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                stack.pop();
            }
            _ => {}
        }
    }
    stack
        .into_iter()
        .rev()
        .find_map(|delimiter| match delimiter {
            Delimiter::Call(context) => Some(context),
            Delimiter::Bracket | Delimiter::Brace => None,
        })
}

enum Delimiter {
    Call(CallContext),
    Bracket,
    Brace,
}

fn recover_analysis(
    server: &Server,
    document: &Document,
    offset: usize,
) -> Option<DocumentAnalysis> {
    for insertion in ["1i32)", "1i32);", "()", "());"] {
        let mut source = document.text.clone();
        source.insert_str(offset, insertion);
        if let Ok(analysis) =
            analyze_with_source_id(&source, document.source_id, &server.host_functions)
        {
            return Some(analysis);
        }
    }
    None
}

fn signature_at_call(
    analysis: &DocumentAnalysis,
    text: &str,
    open: usize,
) -> Option<(String, FunctionSignature)> {
    let symbol = analysis
        .symbols
        .iter()
        .filter(|symbol| {
            matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method)
                && symbol.span.end <= open
                && text[symbol.span.end..open]
                    .chars()
                    .all(|character| character.is_whitespace())
        })
        .max_by_key(|symbol| symbol.span.end)?;
    let Type::Function {
        parameters,
        return_type,
    } = symbol.inferred_type.as_ref()?
    else {
        return None;
    };
    Some((
        symbol.name.clone(),
        FunctionSignature {
            parameters: parameters.clone(),
            return_type: (**return_type).clone(),
        },
    ))
}

fn qualified_path_before(text: &str, open: usize) -> Option<String> {
    let before = &text[..floor_char_boundary(text, open.min(text.len()))];
    let end = before.trim_end().len();
    let mut start = end;
    for (index, character) in before[..end].char_indices().rev() {
        if character == ':' || character == '_' || character.is_alphanumeric() {
            start = index;
        } else {
            break;
        }
    }
    (start < end).then(|| before[start..end].trim_matches(':').to_owned())
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn finds_nested_active_call_and_argument() {
        let text = "outer(1, inner(2, 3";
        assert_eq!(
            call_context(text, text.len()),
            Some(CallContext {
                open: 14,
                argument: 1
            })
        );
    }

    #[test]
    fn ignores_commas_inside_nested_calls() {
        let text = "outer(inner(1, 2), 3";
        assert_eq!(
            call_context(text, text.len()),
            Some(CallContext {
                open: 5,
                argument: 1
            })
        );
    }

    #[test]
    fn ignores_commas_inside_collection_arguments() {
        let text = "outer([1, 2], (3, 4), ";
        assert_eq!(
            call_context(text, text.len()),
            Some(CallContext {
                open: 5,
                argument: 2
            })
        );
    }
}
