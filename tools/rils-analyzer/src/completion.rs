use super::*;

impl Server {
    pub(super) fn completion(&self, params: &Value) -> Result<Value, AnyError> {
        let (uri, document, offset) = self.document_and_offset(params)?;
        if let Some((dot_offset, member_prefix)) = method_completion_target(&document.text, offset)
        {
            let recovered;
            let current_analysis = if let Some(analysis) = analysis(document) {
                Some(analysis)
            } else {
                let mut source = document.text.clone();
                source.insert_str(offset, "__rils_completion");
                recovered = analyze_with_host_and_source_id_and_external_exports(
                    &source,
                    document.source_id,
                    &self.host_contract,
                    &HashMap::new(),
                )
                .ok()
                .or_else(|| {
                    recover_member_completion_analysis(
                        &document.text,
                        dot_offset,
                        document.source_id,
                        &self.host_contract,
                    )
                });
                recovered.as_ref()
            };
            if let Some(receiver_type) = current_analysis.and_then(|analysis| {
                analysis
                    .typeck_results
                    .expression_type_ending_at(document.source_id, dot_offset)
                    .map(|(_, ty)| ty)
                    .or_else(|| {
                        let receiver = identifier_before(&document.text, dot_offset)?;
                        analysis
                            .symbols
                            .iter()
                            .filter(|symbol| {
                                symbol.name == receiver
                                    && symbol.span.start < offset
                                    && symbol.inferred_type.is_some()
                            })
                            .max_by_key(|symbol| symbol.span.start)
                            .and_then(|symbol| symbol.inferred_type.as_ref())
                    })
            }) {
                let mut inherent_items = current_analysis
                    .map(|analysis| {
                        inherent_method_completions(
                            analysis,
                            &document.text,
                            receiver_type,
                            &member_prefix,
                        )
                    })
                    .unwrap_or_default();
                if let Type::Named { name, arguments } = receiver_type
                    && arguments.is_empty()
                    && (name == "HostHandle" || self.host_contract.host_type(name).is_some())
                {
                    inherent_items.extend(
                        self
                        .host_contract
                        .receiver_methods(name)
                        .into_iter()
                        .filter_map(|function| {
                            let (_, name) = function.name.rsplit_once("::")?;
                            name.starts_with(&member_prefix).then(|| {
                                let detail = signature_declaration(name, &function.signature);
                                json!({
                                    "label": detail,
                                    "filterText": name,
                                    "insertText": name,
                                    "kind": 2,
                                    "detail": detail,
                                    "documentation": {
                                        "kind": "markdown",
                                        "value": format!("Host method receiver: `{}`\\n\\nCapability: `{}`", function.receiver.unwrap().as_str(), function.capability)
                                    }
                                })
                            })
                        }),
                    );
                    inherent_items.sort_by(|left, right| {
                        left["label"].as_str().cmp(&right["label"].as_str())
                    });
                    inherent_items.dedup_by(|left, right| left["label"] == right["label"]);
                    return Ok(json!(inherent_items));
                }
                if receiver_type.is_integer() {
                    let items = rils_builtins::INTEGER_INTRINSICS
                        .iter()
                        .filter(|item| {
                            item.kind == rils_builtins::IntrinsicKind::Method
                                && item.name.starts_with(&member_prefix)
                        })
                        .map(integer_intrinsic_completion)
                        .collect::<Vec<_>>();
                    return Ok(json!(items));
                }
                if receiver_type.is_float() {
                    let items = rils_builtins::FLOAT_INTRINSICS
                        .iter()
                        .filter(|item| item.name.starts_with(&member_prefix))
                        .map(integer_intrinsic_completion)
                        .collect::<Vec<_>>();
                    return Ok(json!(items));
                }
                let owner = rils_frontend::standard_library::builtin_owner_name(receiver_type)
                    .or_else(|| {
                        implements_iterator_at_completion(&document.text, offset, receiver_type)
                            .then_some("Iterator")
                    });
                if let Some(owner) = owner {
                    inherent_items.extend(
                        rils_builtins::builtin(owner)
                            .into_iter()
                            .flat_map(|declaration| declaration.members)
                            .filter(|member| {
                                member.kind == rils_builtins::BuiltinMemberKind::Method
                                    && member.name.starts_with(&member_prefix)
                                    && (owner != "Iterator"
                                        || rils_frontend::standard_library::builtin_owner_name(
                                            receiver_type,
                                        )
                                        .is_some()
                                        || rils_builtins::is_iterator_default_method(member.name))
                            })
                            .map(|member| builtin_member_completion(receiver_type, member)),
                    );
                    inherent_items.sort_by(|left, right| {
                        left["label"].as_str().cmp(&right["label"].as_str())
                    });
                    inherent_items.dedup_by(|left, right| left["label"] == right["label"]);
                    return Ok(json!(inherent_items));
                }
                if !inherent_items.is_empty() {
                    inherent_items.sort_by(|left, right| {
                        left["label"].as_str().cmp(&right["label"].as_str())
                    });
                    inherent_items.dedup_by(|left, right| left["label"] == right["label"]);
                    return Ok(json!(inherent_items));
                }
            }
        }
        let Some((qualifier, member_prefix)) = use_tree_completion_target(&document.text, offset)
            .or_else(|| completion_target(&document.text, offset))
        else {
            let recovered = analysis(document).is_none().then(|| {
                recover_member_completion_analysis(
                    &document.text,
                    offset,
                    document.source_id,
                    &self.host_contract,
                )
            });
            return Ok(json!(unqualified_completions(
                analysis(document).or_else(|| recovered.as_ref().and_then(Option::as_ref)),
                document.source_id,
                &document.text,
                offset,
            )));
        };
        if rils_builtins::IntegerType::from_name(&qualifier).is_some() {
            let mut items = rils_builtins::INTEGER_CONSTANTS
                .iter()
                .filter(|item| item.name.starts_with(&member_prefix))
                .map(integer_constant_completion)
                .collect::<Vec<_>>();
            items.extend(
                rils_builtins::INTEGER_INTRINSICS
                    .iter()
                    .filter(|item| {
                        item.kind == rils_builtins::IntrinsicKind::AssociatedFunction
                            && item.name.starts_with(&member_prefix)
                    })
                    .map(integer_intrinsic_completion),
            );
            return Ok(json!(items));
        }
        if rils_frontend::FloatType::from_name(&qualifier).is_some() {
            let items = rils_builtins::FLOAT_CONSTANTS
                .iter()
                .filter(|item| item.name.starts_with(&member_prefix))
                .map(float_constant_completion)
                .collect::<Vec<_>>();
            return Ok(json!(items));
        }
        let builtin_qualifier = qualifier.rsplit_once("::").map_or_else(
            || Some(qualifier.as_str()),
            |(module, name)| {
                rils_builtins::builtin_module_members(module)
                    .contains(&name)
                    .then_some(name)
            },
        );
        if let Some((builtin_name, declaration)) = builtin_qualifier
            .and_then(|name| rils_builtins::builtin(name).map(|declaration| (name, declaration)))
            && matches!(
                declaration.kind,
                rils_builtins::BuiltinKind::Primitive
                    | rils_builtins::BuiltinKind::Struct
                    | rils_builtins::BuiltinKind::Enum
            )
        {
            let ty = Type::Named {
                name: builtin_name.into(),
                arguments: declaration
                    .type_parameters
                    .iter()
                    .map(|_| Type::Unknown)
                    .collect(),
            };
            let items = declaration
                .members
                .iter()
                .filter(|member| {
                    member.kind == rils_builtins::BuiltinMemberKind::AssociatedFunction
                        && member.name.starts_with(&member_prefix)
                })
                .map(|member| builtin_member_completion(&ty, member))
                .collect::<Vec<_>>();
            return Ok(json!(items));
        }
        let qualifier = resolve_path_alias(&document.text, &qualifier);
        if let Some(declaration) = self.host_contract.host_type(&qualifier)
            && let Some(enum_definition) = &declaration.enum_definition
        {
            let items = enum_definition
                .variants
                .iter()
                .filter(|(name, _)| name.starts_with(&member_prefix))
                .map(|(name, raw)| {
                    let enum_kind = if enum_definition.flags {
                        format!(
                            "{} flags enum ({}, BitFlags)",
                            qualifier, enum_definition.underlying_type
                        )
                    } else {
                        format!("{} enum ({})", qualifier, enum_definition.underlying_type)
                    };
                    json!({
                        "label": name,
                        "filterText": name,
                        "insertText": name,
                        "kind": 20,
                        "detail": format!("{qualifier}::{name} = 0x{raw:x}"),
                        "documentation": {
                            "kind": "markdown",
                            "value": format!("`{enum_kind}`\n\nRaw value: `0x{raw:x}`")
                        },
                        "sortText": format!("0_{name}")
                    })
                })
                .collect::<Vec<_>>();
            return Ok(json!(items));
        }
        let nested_prefix = format!("{qualifier}::");
        let mut module_names = HashSet::new();
        let mut items = Vec::new();

        for child in rils_builtins::builtin_module_members(&qualifier) {
            if child.starts_with(&member_prefix) && module_names.insert((*child).to_owned()) {
                let kind = rils_builtins::builtin(child).map_or(9, |declaration| {
                    if declaration.kind == rils_builtins::BuiltinKind::Module {
                        9
                    } else {
                        7
                    }
                });
                items.push(json!({
                    "label": child,
                    "kind": kind,
                    "detail": format!("built-in {}::{child}", qualifier),
                    "sortText": format!("0_{child}")
                }));
            }
        }

        for module in self.host_contract.modules() {
            let Some(remainder) = module.name.strip_prefix(&nested_prefix) else {
                continue;
            };
            let child = remainder.split("::").next().unwrap_or(remainder);
            if child.starts_with(&member_prefix) && module_names.insert(child.to_owned()) {
                let full_name = format!("{qualifier}::{child}");
                items.push(json!({
                    "label": child,
                    "kind": 9,
                    "detail": format!("host module {full_name}"),
                    "sortText": format!("0_{child}")
                }));
            }
        }
        for declaration in self.host_contract.types() {
            let Some(remainder) = declaration.name.strip_prefix(&nested_prefix) else {
                continue;
            };
            let child = remainder.split("::").next().unwrap_or(remainder);
            if !child.starts_with(&member_prefix) || !module_names.insert(child.to_owned()) {
                continue;
            }
            let is_nested_module = remainder.contains("::");
            let full_name = format!("{qualifier}::{child}");
            items.push(json!({
                "label": child,
                "kind": if is_nested_module { 9 } else { 13 },
                "detail": if is_nested_module {
                    format!("host module {full_name}")
                } else {
                    format!("host type {full_name}")
                },
                "sortText": format!("0_{child}")
            }));
        }
        for function in self.host_contract.functions() {
            let Ok((module, name)) = split_qualified_name(&function.name) else {
                continue;
            };
            if module != qualifier || !name.starts_with(&member_prefix) {
                continue;
            }
            let declaration = signature_declaration(name, &function.signature);
            items.push(json!({
                "label": declaration,
                "filterText": name,
                "insertText": name,
                "kind": 3,
                "detail": declaration,
                "documentation": {
                    "kind": "markdown",
                    "value": format!(
                        "```rils\n{}\n```\n\nHost capability: `{}`",
                        signature_declaration(&function.name, &function.signature),
                        function.capability
                    )
                },
                "sortText": format!("1_{name}")
            }));
        }
        self.add_project_completions(
            &uri,
            &qualifier,
            &member_prefix,
            &mut module_names,
            &mut items,
        );
        items.sort_by(|left, right| left["sortText"].as_str().cmp(&right["sortText"].as_str()));
        items.dedup_by(|left, right| left["label"] == right["label"]);
        Ok(json!(items))
    }
}

fn unqualified_completions(
    document_analysis: Option<&DocumentAnalysis>,
    source_id: SourceId,
    source: &str,
    offset: usize,
) -> Vec<Value> {
    let prefix = completion_prefix(source, offset);
    let mut items = Vec::new();
    let mut names = HashSet::new();

    for keyword in [
        "as", "break", "const", "continue", "crate", "else", "enum", "fn", "for", "if", "impl",
        "in", "let", "loop", "match", "mod", "mut", "pub", "return", "self", "struct", "super",
        "trait", "type", "use", "while",
    ] {
        if keyword.starts_with(&prefix) && names.insert(keyword.to_owned()) {
            items.push(json!({
                "label": keyword,
                "filterText": keyword,
                "insertText": keyword,
                "kind": 14,
                "detail": "Rils keyword",
                "sortText": format!("0_{keyword}"),
            }));
        }
    }
    for macro_name in ["assert!", "print!", "println!"] {
        if macro_name
            .trim_end_matches('!')
            .starts_with(prefix.trim_end_matches('!'))
            && names.insert(macro_name.to_owned())
        {
            items.push(json!({
                "label": macro_name,
                "filterText": macro_name,
                "insertText": macro_name,
                "kind": 3,
                "detail": "built-in macro",
                "sortText": format!("1_{macro_name}"),
            }));
        }
    }
    if let Some(analysis) = document_analysis {
        for symbol in analysis.visible_names(source_id, offset) {
            if !symbol.name.starts_with(&prefix) || !names.insert(symbol.name.clone()) {
                continue;
            }
            let label = if symbol.kind == SymbolKind::Macro {
                format!("{}!", symbol.name)
            } else {
                symbol.name.clone()
            };
            items.push(json!({
                "label": label,
                "filterText": symbol.name,
                "insertText": label,
                "kind": completion_kind(symbol.kind),
                "detail": symbol.detail.clone().unwrap_or_else(|| kind_label(symbol.kind).into()),
                "sortText": format!("2_{}", symbol.name),
            }));
        }
    }
    items.sort_by(|left, right| left["sortText"].as_str().cmp(&right["sortText"].as_str()));
    items
}

fn completion_prefix(source: &str, offset: usize) -> String {
    let end = floor_char_boundary(source, offset.min(source.len()));
    let before = &source[..end];
    let start = before
        .char_indices()
        .rev()
        .take_while(|(_, character)| *character == '_' || character.is_alphanumeric())
        .last()
        .map_or(before.len(), |(index, _)| index);
    before[start..].to_owned()
}

fn completion_kind(kind: SymbolKind) -> u32 {
    match kind {
        SymbolKind::Variable | SymbolKind::Parameter => 6,
        SymbolKind::Function | SymbolKind::Macro => 3,
        SymbolKind::Type => 7,
        SymbolKind::Trait => 8,
        SymbolKind::Method => 2,
        SymbolKind::Field => 5,
        SymbolKind::Variant => 20,
        SymbolKind::Module => 9,
    }
}

fn inherent_method_completions(
    analysis: &DocumentAnalysis,
    source: &str,
    receiver_type: &Type,
    prefix: &str,
) -> Vec<Value> {
    let Type::Named { name, .. } = receiver_type else {
        return Vec::new();
    };
    analysis
        .symbols
        .iter()
        .filter(|symbol| {
            symbol.is_definition
                && matches!(symbol.kind, SymbolKind::Method | SymbolKind::Field)
                && symbol.name.starts_with(prefix)
                && symbol.container.as_ref().is_some_and(|container| {
                    let SymbolContainer::Type(owner) = container else {
                        return false;
                    };
                    owner == name || resolve_path_alias(source, owner) == *name
                })
        })
        .map(|symbol| {
            let is_field = symbol.kind == SymbolKind::Field;
            let detail = symbol.detail.clone().unwrap_or_else(|| {
                if is_field {
                    format!("field {}", symbol.name)
                } else {
                    format!("fn {}", symbol.name)
                }
            });
            let documentation = if is_field {
                format!("Field of `{name}`")
            } else {
                format!("Rils method implemented for `{name}`")
            };
            json!({
                "label": detail,
                "filterText": symbol.name,
                "insertText": symbol.name,
                "kind": if is_field { 5 } else { 2 },
                "detail": detail,
                "documentation": {
                    "kind": "markdown",
                    "value": documentation
                }
            })
        })
        .collect()
}

fn recover_member_completion_analysis(
    source: &str,
    dot_offset: usize,
    source_id: SourceId,
    host_contract: &HostContract,
) -> Option<DocumentAnalysis> {
    source[..dot_offset]
        .match_indices(';')
        .map(|(index, _)| index + 1)
        .rev()
        .find_map(|end| {
            let candidate = close_open_delimiters(&source[..end]);
            analyze_with_host_and_source_id_and_external_exports(
                &candidate,
                source_id,
                host_contract,
                &HashMap::new(),
            )
            .ok()
        })
}

fn close_open_delimiters(source: &str) -> String {
    let mut delimiters = Vec::new();
    let mut in_string = None;
    let mut escaped = false;
    for character in source.chars() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                in_string = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => in_string = Some(character),
            '(' => delimiters.push(')'),
            '[' => delimiters.push(']'),
            '{' => delimiters.push('}'),
            ')' | ']' | '}' if delimiters.last() == Some(&character) => {
                delimiters.pop();
            }
            _ => {}
        }
    }
    let mut completed = source.to_owned();
    completed.extend(delimiters.into_iter().rev());
    completed
}

fn implements_iterator_at_completion(text: &str, offset: usize, receiver: &Type) -> bool {
    let Type::Named { name, .. } = receiver else {
        return false;
    };
    let mut source = text.to_owned();
    source.insert_str(offset, "__rils_completion");
    let Ok(tokens) = lex(&source) else {
        return false;
    };
    let Ok(program) = parse(tokens) else {
        return false;
    };
    statements_implement_iterator(&program.statements, name)
}

fn statements_implement_iterator(statements: &[Stmt], target_name: &str) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Impl {
            trait_name: Some(trait_name),
            target: Type::Named { name, .. },
            ..
        } => trait_name == "Iterator" && name == target_name,
        Stmt::Module {
            statements: Some(statements),
            ..
        } => statements_implement_iterator(statements, target_name),
        _ => false,
    })
}
