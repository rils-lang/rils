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
                recovered =
                    analyze_with_source_id(&source, document.source_id, &self.host_functions).ok();
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
                if let Some(owner) =
                    rils_frontend::standard_library::builtin_owner_name(receiver_type)
                {
                    let items = rils_builtins::builtin(owner)
                        .into_iter()
                        .flat_map(|declaration| declaration.members)
                        .filter(|member| {
                            member.kind == rils_builtins::BuiltinMemberKind::Method
                                && member.name.starts_with(&member_prefix)
                        })
                        .map(|member| builtin_member_completion(receiver_type, member))
                        .collect::<Vec<_>>();
                    return Ok(json!(items));
                }
            }
        }
        let Some((qualifier, member_prefix)) = completion_target(&document.text, offset) else {
            return Ok(json!([]));
        };
        if rils_builtins::IntegerType::from_name(&qualifier).is_some() {
            let items = rils_builtins::INTEGER_INTRINSICS
                .iter()
                .filter(|item| {
                    item.kind == rils_builtins::IntrinsicKind::AssociatedFunction
                        && item.name.starts_with(&member_prefix)
                })
                .map(integer_intrinsic_completion)
                .collect::<Vec<_>>();
            return Ok(json!(items));
        }
        let qualifier = resolve_path_alias(&document.text, &qualifier);
        let nested_prefix = format!("{qualifier}::");
        let mut module_names = HashSet::new();
        let mut items = Vec::new();

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
            if let Some(item) = public_completion_item(statement, member_prefix) {
                items.push(item);
            }
        }
    }
}
