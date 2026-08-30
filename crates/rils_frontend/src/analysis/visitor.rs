use super::*;

impl Analyzer {
    pub(super) fn macros(&mut self, program: &Program) {
        for definition in &program.macros {
            let definition_id =
                self.definition_only(&definition.name, definition.name_span, SymbolKind::Macro);
            for span in &definition.references {
                self.result.symbols.push(SymbolOccurrence {
                    name: definition.name.clone(),
                    span: *span,
                    definition_span: Some(definition.name_span),
                    symbol_id: None,
                    definition_id: Some(definition_id),
                    kind: SymbolKind::Macro,
                    is_definition: false,
                    inferred_type: None,
                    detail: None,
                    container: None,
                });
            }
        }
    }

    pub(super) fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    pub(super) fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Public { statement, .. } => {
                let first_symbol = self.result.symbols.len();
                self.statement(statement);
                if let Some(symbol) = self.result.symbols.get_mut(first_symbol)
                    && symbol.is_definition
                {
                    if let Some(detail) = &mut symbol.detail {
                        *detail = format!("pub {detail}");
                    } else if symbol.kind == SymbolKind::Module {
                        symbol.detail = Some(format!("pub mod {}", symbol.name));
                    }
                }
            }
            Stmt::Module {
                name,
                name_span,
                statements,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Module);
                if let Some(statements) = statements {
                    self.module_path.push(name.clone());
                    self.with_scope(|analyzer| analyzer.statements(statements));
                    self.module_path.pop();
                }
            }
            Stmt::Use { imports, .. } => imports::analyze(self, imports),
            Stmt::Let {
                name,
                name_span,
                initializer,
                ..
            } => {
                self.expression(initializer);
                self.define(name, *name_span, SymbolKind::Variable);
            }
            Stmt::Function {
                name,
                name_span,
                generic_parameters,
                parameters,
                return_type,
                body,
                ..
            } => {
                let definition = self.define(name, *name_span, SymbolKind::Function);
                self.owner_ids.record_body(definition, body.span);
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                self.set_last_detail(function_detail(
                    name,
                    generic_parameters,
                    parameters,
                    return_type.as_ref(),
                ));
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                self.with_scope(|analyzer| {
                    for parameter in parameters {
                        analyzer.define(&parameter.name, parameter.span, SymbolKind::Parameter);
                    }
                    analyzer.block_contents(body);
                });
            }
            Stmt::Struct {
                name,
                name_span,
                generic_parameters,
                fields,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Type);
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                self.set_last_detail(struct_detail(name, generic_parameters, fields));
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for field in fields {
                    self.definition_only(&field.name, field.span, SymbolKind::Field);
                    self.set_last_detail(format!(
                        "field {}: {}",
                        field.name, field.type_annotation
                    ));
                    self.set_last_container(SymbolContainer::Type(name.clone()));
                    if let Some(symbol) = self.result.symbols.last_mut() {
                        symbol.inferred_type = Some(field.type_annotation.clone());
                    }
                }
            }
            Stmt::Enum {
                name,
                name_span,
                generic_parameters,
                variants,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Type);
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                self.set_last_detail(enum_detail(name, generic_parameters, variants));
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for variant in variants {
                    let (variant_name, span) = enum_variant_name_and_span(variant);
                    self.definition_only(variant_name, span, SymbolKind::Variant);
                    self.set_last_detail(enum_variant_declaration(name, variant));
                    self.set_last_container(SymbolContainer::Type(name.clone()));
                    if let EnumVariant::Record { fields, .. } = variant {
                        for field in fields {
                            self.definition_only(&field.name, field.span, SymbolKind::Field);
                            self.set_last_detail(format!(
                                "field {}: {}",
                                field.name, field.type_annotation
                            ));
                            self.set_last_container(SymbolContainer::Type(format!(
                                "{name}::{variant_name}"
                            )));
                        }
                    }
                }
            }
            Stmt::Trait {
                name,
                name_span,
                bounds,
                associated_types,
                methods,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Trait);
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                self.set_last_detail(trait_detail(name, bounds, associated_types, methods));
                for associated in associated_types {
                    self.definition_only(&associated.name, associated.name_span, SymbolKind::Type);
                    self.set_last_detail(associated_type_detail(associated));
                    for parameter in &associated.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                }
                for method in methods {
                    self.definition_only(&method.name, method.name_span, SymbolKind::Method);
                    self.set_last_detail(trait_method_detail(method));
                    for parameter in &method.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                }
            }
            Stmt::TypeAlias {
                name,
                name_span,
                generic_parameters,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Type);
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                let arguments = generic_parameters
                    .iter()
                    .map(|parameter| Type::Variable(parameter.name.clone()))
                    .collect::<Vec<_>>();
                let detail = self.type_alias_detail(name, &arguments);
                self.result
                    .symbols
                    .last_mut()
                    .expect("type alias definition symbol")
                    .detail = detail;
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
            }
            Stmt::Impl {
                generic_parameters,
                target,
                associated_types,
                methods,
                span,
                ..
            } => {
                self.owner_ids.allocate_impl(*span, self.source_id);
                let self_type = match target {
                    Type::Named { name, .. } => Some(name.clone()),
                    _ => None,
                };
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for associated in associated_types {
                    self.definition_only(&associated.name, associated.name_span, SymbolKind::Type);
                    self.set_last_detail(associated_type_detail(associated));
                    for parameter in &associated.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                }
                for method in methods {
                    let definition =
                        self.definition_only(&method.name, method.name_span, SymbolKind::Method);
                    self.owner_ids.record_body(definition, method.body.span);
                    self.set_last_detail(impl_method_detail(method));
                    if let Some(owner) = &self_type {
                        self.set_last_container(SymbolContainer::Type(owner.clone()));
                    }
                    for parameter in &method.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                    self.with_scope(|analyzer| {
                        analyzer.self_types.push(self_type.clone());
                        if let Some(self_type) = &self_type
                            && let Some(definition) = analyzer.lookup(self_type).cloned()
                        {
                            analyzer
                                .scopes
                                .last_mut()
                                .expect("scope exists")
                                .insert("Self".into(), definition);
                        }
                        for parameter in &method.parameters {
                            analyzer.define(&parameter.name, parameter.span, SymbolKind::Parameter);
                        }
                        analyzer.block_contents(&method.body);
                        analyzer.self_types.pop();
                    });
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expression(condition);
                self.block(body);
            }
            Stmt::Loop { body, .. } => self.block(body),
            Stmt::For {
                binding,
                binding_span,
                iterable,
                body,
                ..
            } => {
                self.expression(iterable);
                self.with_scope(|analyzer| {
                    analyzer.define(binding, *binding_span, SymbolKind::Variable);
                    analyzer.block_contents(body);
                });
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            Stmt::Continue { .. } => {}
            Stmt::Expr { expression, .. } => self.expression(expression),
        }
    }

    pub(super) fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Literal { .. } => {}
            Expr::Variable { name, span } => {
                if !name.starts_with("#rils_native_") {
                    self.reference(name, *span, SymbolKind::Variable);
                }
            }
            Expr::Path { segments, span } => {
                let semantic_segments = self
                    .expression_ids
                    .get(expression)
                    .and_then(|id| self.host_type_resolutions.expression_path(id))
                    .map(<[String]>::to_vec);
                let semantic_segments = semantic_segments.as_deref().unwrap_or(segments);
                // Host type resolution canonicalizes imported paths (for
                // example `Color::new` becomes
                // `unity_engine::Color::new`). Record the type segment at its
                // actual source position so hover does not select the module
                // segment for an imported host type.
                if segments.len() > 1 {
                    for end in (1..segments.len()).rev() {
                        let candidate = segments[..=end].join("::");
                        if self.host_types.contains(&candidate) {
                            let type_span = source_path_segment_span(segments, end, *span);
                            let type_name = segments[end].clone();
                            self.result.symbols.push(SymbolOccurrence {
                                name: type_name.clone(),
                                span: type_span,
                                definition_span: None,
                                symbol_id: None,
                                definition_id: None,
                                kind: SymbolKind::Type,
                                is_definition: false,
                                inferred_type: Some(Type::named(candidate)),
                                detail: None,
                                container: None,
                            });
                            break;
                        }
                    }
                }
                if let Some(name) = segments.first() {
                    self.reference(
                        name,
                        Span::in_source(span.source, span.start, span.start + name.len()),
                        SymbolKind::Type,
                    );
                }
                if segments.len() > 1
                    && let Some(export) =
                        imports::path_export(&self.module_exports, &self.module_path, segments)
                {
                    let member = segments.last().expect("non-empty path");
                    self.result.symbols.push(SymbolOccurrence {
                        name: member.clone(),
                        span: member_name_span(*span, member),
                        definition_span: Some(export.span),
                        symbol_id: None,
                        definition_id: export.definition_id,
                        kind: export.kind,
                        is_definition: false,
                        inferred_type: export.inferred_type,
                        detail: export.detail,
                        container: Some(SymbolContainer::Module(export.module_path)),
                    });
                }
                let qualified_name = semantic_segments.join("::");
                if let (Some(member), Some(signature)) =
                    (segments.last(), self.host_functions.get(&qualified_name))
                {
                    let parameters = signature
                        .parameters
                        .as_ref()
                        .map(|parameters| {
                            parameters
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_else(|| "...".into());
                    self.result.symbols.push(SymbolOccurrence {
                        name: member.clone(),
                        span: member_name_span(*span, member),
                        definition_span: None,
                        symbol_id: None,
                        definition_id: None,
                        kind: SymbolKind::Function,
                        is_definition: false,
                        inferred_type: Some(signature.as_type()),
                        detail: Some(format!(
                            "host fn {qualified_name}({parameters}) -> {}",
                            signature.return_type
                        )),
                        container: None,
                    });
                }
                if let [trait_name, member] = segments.as_slice() {
                    let definition_span = self
                        .trait_members
                        .get(&(trait_name.clone(), member.clone()))
                        .copied();
                    if definition_span.is_some()
                        || self
                            .lookup(trait_name)
                            .is_some_and(|definition| definition.kind == SymbolKind::Trait)
                    {
                        self.result.symbols.push(SymbolOccurrence {
                            name: member.clone(),
                            span: member_name_span(*span, member),
                            definition_span,
                            symbol_id: None,
                            definition_id: None,
                            kind: SymbolKind::Method,
                            is_definition: false,
                            inferred_type: None,
                            detail: None,
                            container: None,
                        });
                    }
                }
                if let [type_name, member] = semantic_segments {
                    let owner = if type_name == "Self" {
                        self.self_types.last().and_then(Clone::clone)
                    } else {
                        Some(type_name.clone())
                    };
                    if let Some(method) = owner.and_then(|owner| {
                        self.inherent_methods
                            .get(member)
                            .and_then(|methods| methods.iter().find(|method| method.owner == owner))
                            .cloned()
                    }) {
                        self.result.symbols.push(SymbolOccurrence {
                            name: member.clone(),
                            span: member_name_span(*span, member),
                            definition_span: Some(method.span),
                            symbol_id: None,
                            definition_id: None,
                            kind: SymbolKind::Method,
                            is_definition: false,
                            inferred_type: None,
                            detail: Some(method.detail),
                            container: Some(SymbolContainer::Type(method.owner)),
                        });
                    }
                }
                if !segments.is_empty() {
                    self.variant_symbol_for_path(semantic_segments, *span, true);
                }
            }
            Expr::QualifiedPath {
                trait_name,
                member,
                span,
                ..
            } => self.result.symbols.push(SymbolOccurrence {
                name: member.clone(),
                span: member_name_span(*span, member),
                definition_span: self
                    .trait_members
                    .get(&(trait_name.clone(), member.clone()))
                    .copied(),
                symbol_id: None,
                definition_id: None,
                kind: SymbolKind::Method,
                is_definition: false,
                inferred_type: None,
                detail: None,
                container: None,
            }),
            Expr::Member { object, name, span } => {
                self.expression(object);
                if let Some(receiver) = self.expression_ids.get(object) {
                    self.member_receivers
                        .insert(member_name_span(*span, name), receiver);
                }
                self.member_symbol(name, *span, SymbolKind::Field);
            }
            Expr::Index { object, index, .. } => {
                self.expression(object);
                self.expression(index);
            }
            Expr::Tuple { elements, .. } => {
                for element in elements {
                    self.expression(element);
                }
            }
            Expr::Array {
                elements, repeat, ..
            } => {
                for element in elements {
                    self.expression(element);
                }
                if let Some(repeat) = repeat {
                    self.expression(repeat);
                }
            }
            Expr::Try { operand, .. } => self.expression(operand),
            Expr::RecordLiteral { path, fields, span } => {
                let semantic_path = self
                    .expression_ids
                    .get(expression)
                    .and_then(|id| self.host_type_resolutions.expression_path(id))
                    .map(<[String]>::to_vec);
                let semantic_path = semantic_path.as_deref().unwrap_or(path);
                if let Some(name) = path.first() {
                    self.reference(
                        name,
                        Span::new(span.start, span.start + name.len()),
                        SymbolKind::Type,
                    );
                }
                self.variant_symbol_for_path(semantic_path, *span, false);
                for field in fields {
                    self.record_field_symbol(path.last().map(String::as_str), field);
                    self.expression(&field.value);
                }
            }
            Expr::Assign { target, value, .. } => {
                self.expression(target);
                self.expression(value);
            }
            Expr::Borrow { target, .. } => self.expression(target),
            Expr::Unary { operand, .. } => self.expression(operand),
            Expr::Cast { operand, .. } => self.expression(operand),
            Expr::Binary { left, right, .. }
            | Expr::Logical { left, right, .. }
            | Expr::Range {
                start: left,
                end: right,
                ..
            } => {
                self.expression(left);
                self.expression(right);
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                if let Expr::Member { object, name, span } = callee.as_ref() {
                    self.expression(object);
                    if let Some(receiver) = self.expression_ids.get(object) {
                        self.member_receivers
                            .insert(member_name_span(*span, name), receiver);
                    }
                    self.member_symbol(name, *span, SymbolKind::Method);
                } else {
                    self.expression(callee);
                    if let Expr::Path { segments, span } = callee.as_ref()
                        && segments.len() > 1
                        && let Some(member) = segments.last()
                    {
                        let member_span = member_name_span(*span, member);
                        if !self
                            .result
                            .symbols
                            .iter()
                            .any(|symbol| symbol.span == member_span && symbol.name == *member)
                        {
                            self.result.symbols.push(SymbolOccurrence {
                                name: member.clone(),
                                span: member_span,
                                definition_span: None,
                                symbol_id: None,
                                definition_id: None,
                                kind: SymbolKind::Function,
                                is_definition: false,
                                inferred_type: None,
                                detail: None,
                                container: None,
                            });
                        }
                    }
                }
                for argument in arguments {
                    self.expression(argument);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expression(condition);
                self.block(then_branch);
                if let Some(else_branch) = else_branch {
                    self.expression(else_branch);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.expression(value);
                for arm in arms {
                    self.with_scope(|analyzer| {
                        analyzer.pattern(&arm.pattern);
                        analyzer.expression(&arm.expression);
                    });
                }
            }
            Expr::Block(block) => self.block(block),
        }
    }

    pub(super) fn pattern(&mut self, pattern: &Pattern) {
        let semantic_path = self
            .pattern_ids
            .get(pattern)
            .and_then(|id| self.host_type_resolutions.pattern_path(id))
            .map(<[String]>::to_vec);
        match pattern {
            Pattern::Wildcard { .. } | Pattern::Literal { .. } | Pattern::None { .. } => {}
            Pattern::Path { path, span } => {
                self.pattern_variant_symbols(semantic_path.as_deref().unwrap_or(path), *span, true)
            }
            Pattern::Binding { name, span } => {
                self.define(name, *span, SymbolKind::Variable);
            }
            Pattern::Some { inner, .. } => self.pattern(inner),
            Pattern::Ok { inner, .. } | Pattern::Err { inner, .. } => self.pattern(inner),
            Pattern::TupleVariant { path, fields, span } => {
                self.pattern_variant_symbols(
                    semantic_path.as_deref().unwrap_or(path),
                    *span,
                    false,
                );
                for field in fields {
                    self.pattern(field);
                }
            }
            Pattern::Record { path, fields, span } => {
                self.pattern_variant_symbols(
                    semantic_path.as_deref().unwrap_or(path),
                    *span,
                    false,
                );
                for (_, pattern) in fields {
                    self.pattern(pattern);
                }
            }
        }
    }

    pub(super) fn block(&mut self, block: &Block) {
        self.with_scope(|analyzer| analyzer.block_contents(block));
    }

    pub(super) fn block_contents(&mut self, block: &Block) {
        self.statements(&block.statements);
    }
}
