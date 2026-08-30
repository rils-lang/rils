use std::collections::HashMap;

use crate::{
    PatternId, SourceId, Span, Type, TypeRefId,
    ast::{Block, EnumVariant, Expr, Pattern, Program, Stmt},
};

/// Maps type nodes in one immutable `Program` to source-scoped semantic IDs.
///
/// Nested types are assigned independently in AST preorder. The map is valid
/// while the program used to construct it remains alive; Span is retained only
/// as a diagnostic and source-query index.
#[derive(Default)]
pub struct TypeIdentityMap {
    by_node: HashMap<*const Type, TypeRefId>,
    by_span: HashMap<Span, Vec<TypeRefId>>,
    spans: HashMap<TypeRefId, Span>,
}

impl TypeIdentityMap {
    pub fn allocate(program: &Program, fallback_source: SourceId) -> Self {
        let mut maps = SyntaxIdentityMaps::default();
        maps.visit_program(program, fallback_source);
        maps.types
    }

    pub fn get(&self, ty: &Type) -> Option<TypeRefId> {
        self.by_node.get(&(ty as *const Type)).copied()
    }

    pub fn ids_at(&self, span: Span) -> &[TypeRefId] {
        self.by_span.get(&span).map_or(&[], Vec::as_slice)
    }

    pub fn span(&self, id: TypeRefId) -> Option<Span> {
        self.spans.get(&id).copied()
    }

    pub fn extend(&mut self, other: Self) {
        for (span, ids) in other.by_span {
            self.by_span.entry(span).or_default().extend(ids);
        }
        self.spans.extend(other.spans);
        self.by_node.extend(other.by_node);
    }
}

/// Maps pattern nodes in one immutable `Program` to source-scoped semantic IDs.
///
/// Nested patterns are assigned independently in AST preorder. Multiple nodes
/// may share a source range without sharing identity.
#[derive(Default)]
pub struct PatternIdentityMap {
    by_node: HashMap<*const Pattern, PatternId>,
    by_span: HashMap<Span, Vec<PatternId>>,
    spans: HashMap<PatternId, Span>,
}

impl PatternIdentityMap {
    pub fn allocate(program: &Program, fallback_source: SourceId) -> Self {
        let mut maps = SyntaxIdentityMaps::default();
        maps.visit_program(program, fallback_source);
        maps.patterns
    }

    pub fn get(&self, pattern: &Pattern) -> Option<PatternId> {
        self.by_node.get(&(pattern as *const Pattern)).copied()
    }

    pub fn ids_at(&self, span: Span) -> &[PatternId] {
        self.by_span.get(&span).map_or(&[], Vec::as_slice)
    }

    pub fn span(&self, id: PatternId) -> Option<Span> {
        self.spans.get(&id).copied()
    }

    pub fn extend(&mut self, other: Self) {
        for (span, ids) in other.by_span {
            self.by_span.entry(span).or_default().extend(ids);
        }
        self.spans.extend(other.spans);
        self.by_node.extend(other.by_node);
    }
}

#[derive(Default)]
struct SyntaxIdentityMaps {
    types: TypeIdentityMap,
    patterns: PatternIdentityMap,
    next_type_by_source: HashMap<SourceId, u32>,
    next_pattern_by_source: HashMap<SourceId, u32>,
}

impl SyntaxIdentityMaps {
    fn visit_program(&mut self, program: &Program, fallback_source: SourceId) {
        self.visit_statements(&program.statements, fallback_source);
    }

    fn visit_statements(&mut self, statements: &[Stmt], fallback_source: SourceId) {
        for statement in statements {
            let statement = match statement {
                Stmt::Public { statement, .. } => statement.as_ref(),
                statement => statement,
            };
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.visit_statements(statements, fallback_source),
                Stmt::Let {
                    type_annotation,
                    initializer,
                    span,
                    ..
                } => {
                    if let Some(ty) = type_annotation {
                        self.visit_type(ty, *span, fallback_source);
                    }
                    self.visit_expression(initializer, fallback_source);
                }
                Stmt::Function {
                    parameters,
                    return_type,
                    body,
                    span,
                    ..
                } => {
                    for parameter in parameters {
                        if let Some(ty) = &parameter.type_annotation {
                            self.visit_type(ty, parameter.span, fallback_source);
                        }
                    }
                    if let Some(ty) = return_type {
                        self.visit_type(ty, *span, fallback_source);
                    }
                    self.visit_block(body, fallback_source);
                }
                Stmt::Struct { fields, .. } => {
                    for field in fields {
                        self.visit_type(&field.type_annotation, field.span, fallback_source);
                    }
                }
                Stmt::Enum { variants, .. } => {
                    for variant in variants {
                        match variant {
                            EnumVariant::Unit { .. } => {}
                            EnumVariant::Tuple { fields, span, .. } => {
                                for field in fields {
                                    self.visit_type(field, *span, fallback_source);
                                }
                            }
                            EnumVariant::Record { fields, .. } => {
                                for field in fields {
                                    self.visit_type(
                                        &field.type_annotation,
                                        field.span,
                                        fallback_source,
                                    );
                                }
                            }
                        }
                    }
                }
                Stmt::TypeAlias { target, span, .. } => {
                    self.visit_type(target, *span, fallback_source)
                }
                Stmt::Impl {
                    target,
                    associated_types,
                    methods,
                    span,
                    ..
                } => {
                    self.visit_type(target, *span, fallback_source);
                    for associated in associated_types {
                        if let Some(ty) = &associated.value {
                            self.visit_type(ty, associated.span, fallback_source);
                        }
                    }
                    for method in methods {
                        for parameter in &method.parameters {
                            if let Some(ty) = &parameter.type_annotation {
                                self.visit_type(ty, parameter.span, fallback_source);
                            }
                        }
                        if let Some(ty) = &method.return_type {
                            self.visit_type(ty, method.span, fallback_source);
                        }
                        self.visit_block(&method.body, fallback_source);
                    }
                }
                Stmt::Trait {
                    associated_types,
                    methods,
                    ..
                } => {
                    for associated in associated_types {
                        if let Some(ty) = &associated.value {
                            self.visit_type(ty, associated.span, fallback_source);
                        }
                    }
                    for method in methods {
                        for parameter in &method.parameters {
                            if let Some(ty) = &parameter.type_annotation {
                                self.visit_type(ty, parameter.span, fallback_source);
                            }
                        }
                        if let Some(ty) = &method.return_type {
                            self.visit_type(ty, method.span, fallback_source);
                        }
                    }
                }
                Stmt::While {
                    condition, body, ..
                } => {
                    self.visit_expression(condition, fallback_source);
                    self.visit_block(body, fallback_source);
                }
                Stmt::Loop { body, .. } => self.visit_block(body, fallback_source),
                Stmt::For { iterable, body, .. } => {
                    self.visit_expression(iterable, fallback_source);
                    self.visit_block(body, fallback_source);
                }
                Stmt::Return { value, .. } | Stmt::Break { value, .. } => {
                    if let Some(value) = value {
                        self.visit_expression(value, fallback_source);
                    }
                }
                Stmt::Expr { expression, .. } => self.visit_expression(expression, fallback_source),
                Stmt::Module {
                    statements: None, ..
                }
                | Stmt::Use { .. }
                | Stmt::Continue { .. } => {}
                Stmt::Public { .. } => unreachable!("public statements were unwrapped"),
            }
        }
    }

    fn visit_block(&mut self, block: &Block, fallback_source: SourceId) {
        self.visit_statements(&block.statements, fallback_source);
    }

    fn visit_expression(&mut self, expression: &Expr, fallback_source: SourceId) {
        match expression {
            Expr::QualifiedPath { target, span, .. } | Expr::Cast { target, span, .. } => {
                self.visit_type(target, *span, fallback_source);
                if let Expr::Cast { operand, .. } = expression {
                    self.visit_expression(operand, fallback_source);
                }
            }
            Expr::Member { object, .. }
            | Expr::Try {
                operand: object, ..
            }
            | Expr::Borrow { target: object, .. }
            | Expr::Unary {
                operand: object, ..
            } => self.visit_expression(object, fallback_source),
            Expr::Index { object, index, .. }
            | Expr::Assign {
                target: object,
                value: index,
                ..
            }
            | Expr::Binary {
                left: object,
                right: index,
                ..
            }
            | Expr::Logical {
                left: object,
                right: index,
                ..
            }
            | Expr::Range {
                start: object,
                end: index,
                ..
            } => {
                self.visit_expression(object, fallback_source);
                self.visit_expression(index, fallback_source);
            }
            Expr::Tuple { elements, .. } => {
                for element in elements {
                    self.visit_expression(element, fallback_source);
                }
            }
            Expr::Array {
                elements, repeat, ..
            } => {
                for element in elements {
                    self.visit_expression(element, fallback_source);
                }
                if let Some(repeat) = repeat {
                    self.visit_expression(repeat, fallback_source);
                }
            }
            Expr::RecordLiteral { fields, .. } => {
                for field in fields {
                    self.visit_expression(&field.value, fallback_source);
                }
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                self.visit_expression(callee, fallback_source);
                for argument in arguments {
                    self.visit_expression(argument, fallback_source);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.visit_expression(condition, fallback_source);
                self.visit_block(then_branch, fallback_source);
                if let Some(else_branch) = else_branch {
                    self.visit_expression(else_branch, fallback_source);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.visit_expression(value, fallback_source);
                for arm in arms {
                    self.visit_pattern(&arm.pattern, fallback_source);
                    self.visit_expression(&arm.expression, fallback_source);
                }
            }
            Expr::Block(block) => self.visit_block(block, fallback_source),
            Expr::Literal { .. } | Expr::Variable { .. } | Expr::Path { .. } => {}
        }
    }

    fn visit_type(&mut self, ty: &Type, span: Span, fallback_source: SourceId) {
        let source = source_or_fallback(span, fallback_source);
        let next = self.next_type_by_source.entry(source).or_insert(0);
        let id = TypeRefId {
            source,
            local: *next,
        };
        *next = next.checked_add(1).expect("type reference id overflow");
        self.types.by_node.insert(ty as *const Type, id);
        self.types.by_span.entry(span).or_default().push(id);
        self.types.spans.insert(id, span);

        match ty {
            Type::Option(inner) => self.visit_type(inner, span, fallback_source),
            Type::Result(ok, error) => {
                self.visit_type(ok, span, fallback_source);
                self.visit_type(error, span, fallback_source);
            }
            Type::Tuple(elements) => {
                for element in elements {
                    self.visit_type(element, span, fallback_source);
                }
            }
            Type::Array { element, .. } | Type::Reference { inner: element, .. } => {
                self.visit_type(element, span, fallback_source)
            }
            Type::Function {
                parameters,
                return_type,
            } => {
                if let Some(parameters) = parameters {
                    for parameter in parameters {
                        self.visit_type(parameter, span, fallback_source);
                    }
                }
                self.visit_type(return_type, span, fallback_source);
            }
            Type::Named { arguments, .. } => {
                for argument in arguments {
                    self.visit_type(argument, span, fallback_source);
                }
            }
            Type::Associated {
                base, arguments, ..
            } => {
                self.visit_type(base, span, fallback_source);
                for argument in arguments {
                    self.visit_type(argument, span, fallback_source);
                }
            }
            _ => {}
        }
    }

    fn visit_pattern(&mut self, pattern: &Pattern, fallback_source: SourceId) {
        let span = pattern.span();
        let source = source_or_fallback(span, fallback_source);
        let next = self.next_pattern_by_source.entry(source).or_insert(0);
        let id = PatternId {
            source,
            local: *next,
        };
        *next = next.checked_add(1).expect("pattern id overflow");
        self.patterns.by_node.insert(pattern as *const Pattern, id);
        self.patterns.by_span.entry(span).or_default().push(id);
        self.patterns.spans.insert(id, span);

        match pattern {
            Pattern::Some { inner, .. }
            | Pattern::Ok { inner, .. }
            | Pattern::Err { inner, .. } => self.visit_pattern(inner, fallback_source),
            Pattern::TupleVariant { fields, .. } => {
                for field in fields {
                    self.visit_pattern(field, fallback_source);
                }
            }
            Pattern::Record { fields, .. } => {
                for (_, field) in fields {
                    self.visit_pattern(field, fallback_source);
                }
            }
            Pattern::Wildcard { .. }
            | Pattern::Binding { .. }
            | Pattern::Literal { .. }
            | Pattern::None { .. }
            | Pattern::Path { .. } => {}
        }
    }
}

fn source_or_fallback(span: Span, fallback: SourceId) -> SourceId {
    if span.source == SourceId::UNKNOWN {
        fallback
    } else {
        span.source
    }
}
