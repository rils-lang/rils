use super::*;

impl Analyzer {
    pub(super) fn collect_struct_fields(&mut self, statements: &[Stmt]) {
        fn visit(
            statements: &[Stmt],
            output: &mut HashMap<String, Vec<HashMap<String, StructFieldSymbol>>>,
        ) {
            for statement in statements {
                match statement {
                    Stmt::Struct { name, fields, .. } => {
                        output.entry(name.clone()).or_default().push(
                            fields
                                .iter()
                                .map(|field| {
                                    (
                                        field.name.clone(),
                                        StructFieldSymbol {
                                            span: field.span,
                                            ty: field.type_annotation.clone(),
                                            detail: format!(
                                                "field {}: {}",
                                                field.name, field.type_annotation
                                            ),
                                            owner: name.clone(),
                                        },
                                    )
                                })
                                .collect(),
                        );
                    }
                    Stmt::Module {
                        statements: Some(children),
                        ..
                    } => visit(children, output),
                    _ => {}
                }
            }
        }

        visit(statements, &mut self.struct_fields);
    }

    pub(super) fn enrich_member_symbols(
        &mut self,
        expression_types: &HashMap<crate::ExprId, Type>,
    ) {
        let mut updates = Vec::new();
        for (index, symbol) in self.result.symbols.iter().enumerate() {
            if symbol.is_definition {
                continue;
            }
            let Some(receiver) = self.member_receivers.get(&symbol.span) else {
                continue;
            };
            let Some(receiver_type) = expression_types.get(receiver) else {
                continue;
            };
            let receiver_type = match receiver_type {
                Type::Reference { inner, .. } => inner.as_ref(),
                receiver_type => receiver_type,
            };
            if let Type::Named { name, .. } = receiver_type
                && let Some(method) = self.inherent_methods.get(&symbol.name).and_then(|methods| {
                    let candidates = methods
                        .iter()
                        .filter(|method| method.owner == *name)
                        .collect::<Vec<_>>();
                    let [method] = candidates.as_slice() else {
                        return None;
                    };
                    Some((*method).clone())
                })
            {
                let definition_id = self
                    .result
                    .symbols
                    .iter()
                    .find(|candidate| {
                        candidate.is_definition
                            && candidate.kind == SymbolKind::Method
                            && candidate.span == method.span
                    })
                    .and_then(|candidate| candidate.symbol_id);
                updates.push((
                    index,
                    Some(method.span),
                    definition_id,
                    None,
                    Some(method.detail),
                    Some(SymbolContainer::Type(method.owner)),
                    Some(SymbolKind::Method),
                ));
                continue;
            }
            if symbol.kind == SymbolKind::Method
                && let Some(method_type) =
                    crate::standard_library::builtin_member_type(receiver_type, &symbol.name)
            {
                updates.push((
                    index,
                    None,
                    None,
                    Some(method_type.clone()),
                    Some(callable_detail(&symbol.name, &method_type)),
                    None,
                    None,
                ));
                continue;
            }
            if symbol.kind != SymbolKind::Field {
                continue;
            }
            let Type::Named { name, .. } = receiver_type else {
                continue;
            };
            let Some(definitions) = self.struct_fields.get(name) else {
                continue;
            };
            let candidates = definitions
                .iter()
                .filter_map(|fields| fields.get(&symbol.name))
                .collect::<Vec<_>>();
            let [field] = candidates.as_slice() else {
                continue;
            };
            let definition_id = self
                .result
                .symbols
                .iter()
                .find(|candidate| {
                    candidate.is_definition
                        && candidate.kind == SymbolKind::Field
                        && candidate.span == field.span
                })
                .and_then(|candidate| candidate.symbol_id);
            updates.push((
                index,
                Some(field.span),
                definition_id,
                Some(field.ty.clone()),
                Some(field.detail.clone()),
                Some(SymbolContainer::Type(field.owner.clone())),
                None,
            ));
        }
        for (index, definition_span, definition_id, inferred_type, detail, container, kind) in
            updates
        {
            let symbol = &mut self.result.symbols[index];
            symbol.definition_span = definition_span;
            symbol.definition_id = definition_id;
            symbol.inferred_type = inferred_type;
            symbol.detail = detail;
            if container.is_some() {
                symbol.container = container;
            }
            if let Some(kind) = kind {
                symbol.kind = kind;
            }
        }
    }

    pub(super) fn record_field_symbol(&mut self, type_name: Option<&str>, field: &RecordField) {
        let definition = type_name.and_then(|type_name| {
            let definitions = self.struct_fields.get(type_name)?;
            let candidates = definitions
                .iter()
                .filter_map(|fields| fields.get(&field.name))
                .collect::<Vec<_>>();
            let [field] = candidates.as_slice() else {
                return None;
            };
            Some((*field).clone())
        });
        let definition_id = definition.as_ref().and_then(|field| {
            self.result
                .symbols
                .iter()
                .find(|candidate| {
                    candidate.is_definition
                        && candidate.kind == SymbolKind::Field
                        && candidate.span == field.span
                })
                .and_then(|candidate| candidate.symbol_id)
        });
        self.result.symbols.push(SymbolOccurrence {
            name: field.name.clone(),
            span: field.name_span,
            definition_span: definition.as_ref().map(|field| field.span),
            symbol_id: None,
            definition_id,
            kind: SymbolKind::Field,
            is_definition: false,
            inferred_type: definition.as_ref().map(|field| field.ty.clone()),
            detail: definition.as_ref().map(|field| field.detail.clone()),
            container: definition.map(|field| SymbolContainer::Type(field.owner)),
        });
    }

    pub(super) fn collect_type_aliases(&mut self, statements: &[Stmt]) {
        for statement in statements {
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.collect_type_aliases(statements),
                Stmt::TypeAlias {
                    name,
                    generic_parameters,
                    target,
                    ..
                } => {
                    self.type_aliases.insert(
                        name.clone(),
                        TypeAliasDefinition {
                            parameters: generic_parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            target: target.clone(),
                        },
                    );
                }
                _ => {}
            }
        }
    }

    pub(super) fn collect_trait_members(&mut self, statements: &[Stmt]) {
        for statement in statements {
            if let Stmt::Module {
                statements: Some(statements),
                ..
            } = statement
            {
                self.collect_trait_members(statements);
                continue;
            }
            if let Stmt::Trait {
                name,
                associated_types,
                methods,
                ..
            } = statement
            {
                for associated in associated_types {
                    self.trait_members.insert(
                        (name.clone(), associated.name.clone()),
                        associated.name_span,
                    );
                }
                for method in methods {
                    self.trait_members
                        .insert((name.clone(), method.name.clone()), method.name_span);
                }
            }
        }
    }

    pub(super) fn collect_inherent_methods(&mut self, statements: &[Stmt]) {
        for statement in statements {
            if let Stmt::Module {
                statements: Some(statements),
                ..
            } = statement
            {
                self.collect_inherent_methods(statements);
                continue;
            }
            let Stmt::Impl {
                trait_name: None,
                target: Type::Named { name: owner, .. },
                methods,
                ..
            } = statement
            else {
                continue;
            };
            for method in methods {
                self.inherent_methods
                    .entry(method.name.clone())
                    .or_default()
                    .push(InherentMethod {
                        owner: owner.clone(),
                        span: method.name_span,
                        detail: impl_method_detail(method),
                    });
            }
        }
    }

    pub(super) fn collect_enum_variants(&mut self, statements: &[Stmt]) {
        fn visit(
            statements: &[Stmt],
            prefix: &mut Vec<String>,
            output: &mut HashMap<(String, String), EnumVariantSymbol>,
        ) {
            for statement in statements {
                if let Stmt::Module {
                    name,
                    statements: Some(statements),
                    ..
                } = statement
                {
                    prefix.push(name.clone());
                    visit(statements, prefix, output);
                    prefix.pop();
                    continue;
                }
                let Stmt::Enum { name, variants, .. } = statement else {
                    continue;
                };
                let owner = prefix
                    .iter()
                    .chain(std::iter::once(name))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::");
                for variant in variants {
                    let (variant_name, span) = enum_variant_name_and_span(variant);
                    output.insert(
                        (owner.clone(), variant_name.into()),
                        EnumVariantSymbol {
                            span: Some(span),
                            detail: enum_variant_declaration(name, variant),
                            owner: owner.clone(),
                        },
                    );
                }
            }
        }

        visit(statements, &mut Vec::new(), &mut self.enum_variants);
    }

    pub(super) fn collect_host_enum_variants(&mut self) {
        let Some(host) = self.host_contract.as_ref() else {
            return;
        };
        for declaration in host.types() {
            let Some(definition) = declaration.enum_definition.as_ref() else {
                continue;
            };
            let short_name = declaration
                .name
                .rsplit("::")
                .next()
                .unwrap_or(&declaration.name);
            for variant in definition.variants.keys() {
                self.enum_variants.insert(
                    (declaration.name.clone(), variant.clone()),
                    EnumVariantSymbol {
                        span: None,
                        detail: format!("{short_name}::{variant}"),
                        owner: declaration.name.clone(),
                    },
                );
            }
        }
    }
}
