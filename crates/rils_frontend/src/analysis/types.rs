use super::*;

impl Analyzer {
    pub(super) fn type_references(&mut self, program: &Program) {
        for reference in &program.type_references {
            let resolved_name = self
                .self_type_references
                .get(&reference.span)
                .map_or(reference.name.as_str(), String::as_str);
            let resolved = reference.definition_span.or_else(|| {
                self.result
                    .symbols
                    .iter()
                    .find(|symbol| {
                        symbol.is_definition
                            && symbol.name == resolved_name
                            && matches!(symbol.kind, SymbolKind::Type | SymbolKind::Trait)
                    })
                    .map(|symbol| symbol.span)
            });
            let definition_id = self
                .result
                .symbols
                .iter()
                .find(|symbol| {
                    symbol.is_definition
                        && symbol.name == resolved_name
                        && matches!(symbol.kind, SymbolKind::Type | SymbolKind::Trait)
                })
                .and_then(|symbol| symbol.symbol_id);
            if resolved.is_none()
                && !reference.is_builtin
                && !self.host_type_segments.contains(&reference.name)
            {
                self.result.diagnostics.push(AnalysisDiagnostic::error(
                    format!("undefined type or trait `{}`", reference.name),
                    reference.span,
                ));
            }
            let key_type = match reference.name.as_str() {
                "HashMap" => reference.arguments.first(),
                "HashSet" => reference.arguments.first(),
                _ => None,
            };
            if let Some(key_type) = key_type
                && !hash_key_type_supported(&self.expand_type(key_type, &mut HashSet::new()))
            {
                self.result.diagnostics.push(AnalysisDiagnostic::error(
                    format!("type `{key_type}` does not implement Eq + Hash"),
                    reference.span,
                ));
            }
            self.result.symbols.push(SymbolOccurrence {
                name: reference.name.clone(),
                span: reference.span,
                definition_span: resolved,
                symbol_id: None,
                definition_id,
                kind: SymbolKind::Type,
                is_definition: false,
                inferred_type: None,
                detail: self.type_alias_detail(&reference.name, &reference.arguments),
                container: self
                    .lookup(resolved_name)
                    .and_then(|definition| definition.container.clone()),
            });
        }
    }

    pub(super) fn type_alias_detail(&self, name: &str, arguments: &[Type]) -> Option<String> {
        let alias = self.type_aliases.get(name)?;
        if alias.parameters.len() != arguments.len() {
            return None;
        }
        let expanded = self.expand_type_alias(name, arguments, &mut HashSet::new())?;
        let arguments = if arguments.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                arguments
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        Some(format!("type {name}{arguments} = {expanded}"))
    }

    pub(super) fn set_last_detail(&mut self, detail: String) {
        self.result
            .symbols
            .last_mut()
            .expect("definition symbol")
            .detail = Some(detail);
    }

    pub(super) fn set_last_container(&mut self, container: SymbolContainer) {
        let symbol = self.result.symbols.last_mut().expect("definition symbol");
        symbol.container = Some(container.clone());
        if symbol.is_definition
            && let Some(definition) = self
                .scopes
                .last_mut()
                .and_then(|scope| scope.get_mut(&symbol.name))
        {
            definition.container = Some(container);
        }
    }

    pub(super) fn module_path_for_definition(&self, span: Span) -> String {
        if let Some(module) = self.definition_modules.get(&span) {
            return module.clone();
        }
        if self.module_path.is_empty() {
            "crate".into()
        } else {
            self.module_path.join("::")
        }
    }

    pub(super) fn expand_type_alias(
        &self,
        name: &str,
        arguments: &[Type],
        visiting: &mut HashSet<String>,
    ) -> Option<Type> {
        let alias = self.type_aliases.get(name)?;
        if alias.parameters.len() != arguments.len() || !visiting.insert(name.into()) {
            return None;
        }
        let substitutions = alias
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        let expanded = self.expand_type(&alias.target.substitute(&substitutions), visiting);
        visiting.remove(name);
        Some(expanded)
    }

    pub(super) fn expand_type(&self, ty: &Type, visiting: &mut HashSet<String>) -> Type {
        match ty {
            Type::Named { name, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expand_type(argument, visiting))
                    .collect::<Vec<_>>();
                self.expand_type_alias(name, &arguments, visiting)
                    .unwrap_or_else(|| Type::Named {
                        name: name.clone(),
                        arguments,
                    })
            }
            Type::Option(inner) => Type::Option(Box::new(self.expand_type(inner, visiting))),
            Type::Result(ok, error) => Type::Result(
                Box::new(self.expand_type(ok, visiting)),
                Box::new(self.expand_type(error, visiting)),
            ),
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.expand_type(element, visiting))
                    .collect(),
            ),
            Type::Array { element, length } => Type::Array {
                element: Box::new(self.expand_type(element, visiting)),
                length: *length,
            },
            Type::Reference { mutable, inner } => Type::Reference {
                mutable: *mutable,
                inner: Box::new(self.expand_type(inner, visiting)),
            },
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters.as_ref().map(|parameters| {
                    parameters
                        .iter()
                        .map(|parameter| self.expand_type(parameter, visiting))
                        .collect()
                }),
                return_type: Box::new(self.expand_type(return_type, visiting)),
            },
            Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => Type::Associated {
                base: Box::new(self.expand_type(base, visiting)),
                trait_name: trait_name.clone(),
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| self.expand_type(argument, visiting))
                    .collect(),
            },
            other => other.clone(),
        }
    }

    pub(super) fn lookup(&self, name: &str) -> Option<&Definition> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    pub(super) fn with_scope(&mut self, action: impl FnOnce(&mut Self)) {
        self.scopes.push(HashMap::new());
        self.glob_imports.push(false);
        action(self);
        self.glob_imports.pop();
        self.scopes.pop();
    }
}
