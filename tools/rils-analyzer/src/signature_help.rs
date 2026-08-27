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
        let signatures = if let Some(signatures) = self.host_signatures_at_call(
            current_analysis,
            document.source_id,
            &document.text,
            context.open,
        ) {
            signatures
        } else if let Some((name, signature)) = current_analysis
            .and_then(|analysis| {
                semantic_signature_at_call(
                    analysis,
                    document.source_id,
                    &document.text,
                    context.open,
                )
            })
            .or_else(|| {
                current_analysis
                    .and_then(|analysis| signature_at_call(analysis, &document.text, context.open))
            })
            .or_else(|| {
                current_analysis.and_then(|analysis| {
                    builtin_signature_at_call(
                        analysis,
                        document.source_id,
                        &document.text,
                        context.open,
                    )
                })
            })
        {
            vec![(name, signature)]
        } else {
            return Ok(Value::Null);
        };
        let active_signature = signatures
            .iter()
            .position(|(_, signature)| {
                signature
                    .parameters
                    .as_ref()
                    .is_some_and(|parameters| context.argument < parameters.len())
            })
            .unwrap_or(0);
        let parameters = signatures[active_signature]
            .1
            .parameters
            .clone()
            .unwrap_or_default();
        let active_parameter = if parameters.is_empty() {
            0
        } else {
            context.argument.min(parameters.len() - 1)
        };
        let signature_items = signatures
            .iter()
            .map(|(name, signature)| {
                let labels = signature
                    .parameters
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                json!({
                    "label": signature_declaration(name, signature),
                    "parameters": labels.iter().map(|label| json!({ "label": label })).collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "signatures": signature_items,
            "activeSignature": active_signature,
            "activeParameter": active_parameter
        }))
    }

    fn host_signatures_at_call(
        &self,
        analysis: Option<&DocumentAnalysis>,
        source: rils_frontend::SourceId,
        text: &str,
        open: usize,
    ) -> Option<Vec<(String, FunctionSignature)>> {
        if let Some(analysis) = analysis
            && let Some((name, receiver_type)) = member_call_receiver(analysis, source, text, open)
        {
            let receiver_type = match receiver_type {
                Type::Reference { inner, .. } => inner.as_ref(),
                receiver_type => receiver_type,
            };
            if let Type::Named {
                name: receiver_type,
                arguments,
            } = receiver_type
                && arguments.is_empty()
            {
                let signatures = self
                    .host_contract
                    .receiver_methods(receiver_type)
                    .into_iter()
                    .filter(|function| {
                        function
                            .name
                            .rsplit_once("::")
                            .is_some_and(|(_, member)| member == name)
                    })
                    .map(|function| {
                        let mut signature = function.signature.clone();
                        if let Some(parameters) = signature.parameters.as_mut()
                            && !parameters.is_empty()
                        {
                            parameters.remove(0);
                        }
                        (name.clone(), signature)
                    })
                    .collect::<Vec<_>>();
                if !signatures.is_empty() {
                    return Some(signatures);
                }
            }
        }
        let path = qualified_path_before(text, open)?;
        let resolved = resolve_path_alias(text, &path);
        let name = resolved.rsplit("::").next()?.to_owned();
        let signatures = self
            .host_contract
            .functions_named(&resolved)
            .map(|function| {
                let mut signature = function.signature.clone();
                if function.receiver.is_some()
                    && let Some(parameters) = signature.parameters.as_mut()
                    && !parameters.is_empty()
                {
                    parameters.remove(0);
                }
                (name.clone(), signature)
            })
            .collect::<Vec<_>>();
        (!signatures.is_empty()).then_some(signatures)
    }
}

fn semantic_signature_at_call(
    analysis: &DocumentAnalysis,
    source: rils_frontend::SourceId,
    text: &str,
    open: usize,
) -> Option<(String, FunctionSignature)> {
    let (_, call) = analysis
        .typeck_results
        .resolved_call_containing(source, open)?;
    match call {
        rils_frontend::ResolvedCall::Definition(definition) => {
            let definition = analysis.def_map.definition(*definition)?;
            function_signature(definition.name.clone(), definition.inferred_type.clone()?)
        }
        rils_frontend::ResolvedCall::Builtin { id, kind, .. } => {
            let (_, receiver_type) = member_call_receiver(analysis, source, text, open)?;
            match kind {
                rils_frontend::BuiltinCallKind::Intrinsic => {
                    let intrinsic = rils_builtins::intrinsic(*id)?;
                    let member_type = match receiver_type {
                        Type::Integer(integer) => {
                            rils_frontend::standard_library::integer_intrinsic_type(
                                intrinsic, *integer,
                            )
                        }
                        Type::Float(float) => {
                            rils_frontend::standard_library::float_intrinsic_type(intrinsic, *float)
                        }
                        _ => return None,
                    };
                    function_signature(intrinsic.name.into(), member_type)
                }
                rils_frontend::BuiltinCallKind::Runtime => {
                    let (_, member) = rils_builtins::runtime_member(*id)?;
                    let member_type = rils_frontend::standard_library::builtin_member_type(
                        receiver_type,
                        member.name,
                    )?;
                    function_signature(member.name.into(), member_type)
                }
            }
        }
        rils_frontend::ResolvedCall::Host { .. } => None,
        rils_frontend::ResolvedCall::Import {
            name, signature, ..
        } => Some((
            name.rsplit("::").next().unwrap_or(name).to_owned(),
            signature.clone(),
        )),
    }
}

fn member_call_receiver<'a>(
    analysis: &'a DocumentAnalysis,
    source: rils_frontend::SourceId,
    text: &str,
    open: usize,
) -> Option<(String, &'a Type)> {
    let name = identifier_before(text, open)?.to_owned();
    let name_start = text[..open].trim_end().len().checked_sub(name.len())?;
    let dot = name_start.checked_sub(1)?;
    if text.as_bytes().get(dot) != Some(&b'.') {
        return None;
    }
    let receiver_type = analysis
        .typeck_results
        .expression_type_ending_at(source, dot)
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
    Some((name, receiver_type))
}

fn builtin_signature_at_call(
    analysis: &DocumentAnalysis,
    source: rils_frontend::SourceId,
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
        .typeck_results
        .expression_type_ending_at(source, dot)
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
    if let Type::Float(float) = receiver_type
        && let Some(intrinsic) = rils_builtins::float_method(&name)
    {
        return function_signature(
            name,
            rils_frontend::standard_library::float_intrinsic_type(intrinsic, *float),
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
        if let Ok(analysis) = analyze_with_host_and_source_id_and_external_exports(
            &source,
            document.source_id,
            &server.host_contract,
            &HashMap::new(),
        ) {
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
#[path = "../tests/unit/signature_help.rs"]
mod unit_tests;
