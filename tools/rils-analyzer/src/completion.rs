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
                recovered = analyze_with_source_id_and_external_exports_and_host_types(
                    &source,
                    document.source_id,
                    &self.host_functions,
                    &self.host_types,
                    &HashMap::new(),
                )
                .ok();
                recovered.as_ref()
            };
            if let Some(receiver_type) = current_analysis.and_then(|analysis| {
                analysis
                    .expression_types
                    .iter()
                    .filter(|(span, _)| span.end == dot_offset)
                    .max_by_key(|(span, _)| span.start)
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
                if let Type::Named { name, arguments } = receiver_type
                    && arguments.is_empty()
                    && (name == "HostHandle" || self.host_contract.host_type(name).is_some())
                {
                    let mut items = self
                        .host_contract
                        .receiver_methods(name)
                        .into_iter()
                        .filter_map(|function| {
                            let (_, name) = function.name.rsplit_once("::")?;
                            name.starts_with(&member_prefix).then(|| {
                                json!({
                                    "label": name,
                                    "kind": 2,
                                    "detail": signature_declaration(name, &function.signature),
                                    "documentation": {
                                        "kind": "markdown",
                                        "value": format!("Host method receiver: `{}`\\n\\nCapability: `{}`", function.receiver.unwrap().as_str(), function.capability)
                                    }
                                })
                            })
                        })
                        .collect::<Vec<_>>();
                    items.sort_by(|left, right| {
                        left["label"].as_str().cmp(&right["label"].as_str())
                    });
                    return Ok(json!(items));
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
                    let items = rils_builtins::builtin(owner)
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
                        .map(|member| builtin_member_completion(receiver_type, member))
                        .collect::<Vec<_>>();
                    return Ok(json!(items));
                }
            }
        }
        let Some((qualifier, member_prefix)) = use_tree_completion_target(&document.text, offset)
            .or_else(|| completion_target(&document.text, offset))
        else {
            return Ok(json!([]));
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
        for function in self.host_contract.functions() {
            let Ok((module, name)) = split_qualified_name(&function.name) else {
                continue;
            };
            if module != qualifier || !name.starts_with(&member_prefix) {
                continue;
            }
            let declaration = signature_declaration(name, &function.signature);
            items.push(json!({
                "label": name,
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

    fn add_project_completions(
        &self,
        uri: &str,
        qualifier: &str,
        member_prefix: &str,
        module_names: &mut HashSet<String>,
        items: &mut Vec<Value>,
    ) {
        let Some(path) = file_uri_to_path(uri) else {
            return;
        };
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.module_for_file(&path).is_some())
        else {
            return;
        };
        let current = project
            .module_for_file(&path)
            .map(|file| file.module_path.as_str())
            .unwrap_or_default();
        let Some(module_path) = resolve_project_path(current, qualifier) else {
            return;
        };
        let nested_prefix = if module_path.is_empty() {
            String::new()
        } else {
            format!("{module_path}::")
        };
        for file in project.modules() {
            if file.module_path == module_path {
                continue;
            }
            let Some(remainder) = file.module_path.strip_prefix(&nested_prefix) else {
                continue;
            };
            if remainder.is_empty() {
                continue;
            }
            let child = remainder.split("::").next().unwrap_or(remainder);
            if child.starts_with(member_prefix) && module_names.insert(child.to_owned()) {
                items.push(json!({
                    "label": child,
                    "kind": 9,
                    "detail": format!("module {}", join_module_path(&module_path, child)),
                    "sortText": format!("0_{child}")
                }));
            }
        }
        let Some(file) = project.module(&module_path) else {
            return;
        };
        let owned_source;
        let source = if let Some(document) = self.documents.get(&path_to_file_uri(&file.path)) {
            document.text.as_str()
        } else {
            let Ok(text) = fs::read_to_string(&file.path) else {
                return;
            };
            owned_source = text;
            &owned_source
        };
        let Ok(tokens) = lex(source) else {
            return;
        };
        let Ok(program) = parse(tokens) else {
            return;
        };
        for statement in &program.statements {
            let Stmt::Public { statement, .. } = statement else {
                continue;
            };
            items.extend(public_completion_items(statement, member_prefix));
        }
    }
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
    statements.iter().any(|statement| {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        match statement {
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
        }
    })
}
