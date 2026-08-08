use std::collections::HashMap;

use crate::{
    ast::{BinaryOp, Block, EnumVariant, Expr, Literal, Pattern, Program, Stmt, UnaryOp},
    source::Span,
    types::{Type, merge_types},
};

#[derive(Clone, Debug)]
pub(crate) struct RawTypeHint {
    pub position: usize,
    pub span: Span,
    pub ty: Type,
    pub prefix: &'static str,
}

#[derive(Default)]
pub(crate) struct InferenceResult {
    pub binding_types: HashMap<Span, Type>,
    pub expression_types: HashMap<Span, Type>,
    pub hints: Vec<RawTypeHint>,
}

#[derive(Clone)]
struct Binding {
    ty: Type,
}

#[derive(Clone, Default)]
struct TypeDefinition {
    fields: HashMap<String, Type>,
    variants: HashMap<String, VariantDefinition>,
    methods: HashMap<String, Type>,
}

#[derive(Clone)]
enum VariantDefinition {
    Unit,
    Tuple(Vec<Type>),
    Record(HashMap<String, Type>),
}

pub(crate) fn infer(program: &Program) -> InferenceResult {
    Inferencer::new(program).run(program)
}

struct Inferencer {
    scopes: Vec<HashMap<String, Binding>>,
    types: HashMap<String, TypeDefinition>,
    variant_owners: HashMap<String, String>,
    result: InferenceResult,
}

impl Inferencer {
    fn new(program: &Program) -> Self {
        let mut globals = HashMap::new();
        for (name, return_type) in [
            ("#rils_native_print", Type::Unit),
            ("#rils_native_println", Type::Unit),
            ("type_of", Type::String),
            ("clone", Type::Unknown),
            ("#rils_native_assert", Type::Unit),
            ("is_some", Type::Bool),
            ("is_none", Type::Bool),
        ] {
            globals.insert(
                name.into(),
                Binding {
                    ty: Type::Function {
                        parameters: None,
                        return_type: Box::new(return_type),
                    },
                },
            );
        }

        let mut inferencer = Self {
            scopes: vec![globals],
            types: HashMap::new(),
            variant_owners: HashMap::new(),
            result: InferenceResult::default(),
        };
        inferencer.collect_type_definitions(&program.statements);
        inferencer
    }

    fn run(mut self, program: &Program) -> InferenceResult {
        let mut returns = Vec::new();
        self.statements(&program.statements, &mut returns);
        self.result
    }

    fn collect_type_definitions(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let statement = match statement {
                Stmt::Public { statement, .. } => statement.as_ref(),
                statement => statement,
            };
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.collect_type_definitions(statements),
                Stmt::Struct { name, fields, .. } => {
                    self.types.insert(
                        name.clone(),
                        TypeDefinition {
                            fields: fields
                                .iter()
                                .map(|field| (field.name.clone(), field.type_annotation.clone()))
                                .collect(),
                            variants: HashMap::new(),
                            methods: HashMap::new(),
                        },
                    );
                }
                Stmt::Enum { name, variants, .. } => {
                    let mut definition = TypeDefinition::default();
                    for variant in variants {
                        let (variant_name, payload) = match variant {
                            EnumVariant::Unit { name, .. } => (name, VariantDefinition::Unit),
                            EnumVariant::Tuple { name, fields, .. } => {
                                (name, VariantDefinition::Tuple(fields.clone()))
                            }
                            EnumVariant::Record { name, fields, .. } => (
                                name,
                                VariantDefinition::Record(
                                    fields
                                        .iter()
                                        .map(|field| {
                                            (field.name.clone(), field.type_annotation.clone())
                                        })
                                        .collect(),
                                ),
                            ),
                        };
                        definition.variants.insert(variant_name.clone(), payload);
                        self.variant_owners
                            .insert(variant_name.clone(), name.clone());
                    }
                    self.types.insert(name.clone(), definition);
                }
                Stmt::Impl {
                    target, methods, ..
                } => {
                    let Type::Named { name, .. } = target else {
                        continue;
                    };
                    let Some(definition) = self.types.get_mut(name) else {
                        continue;
                    };
                    for method in methods {
                        let parameters = method
                            .parameters
                            .iter()
                            .filter(|parameter| parameter.name != "self")
                            .map(|parameter| {
                                parameter.type_annotation.clone().unwrap_or(Type::Unknown)
                            })
                            .collect();
                        definition.methods.insert(
                            method.name.clone(),
                            Type::function(
                                parameters,
                                method.return_type.clone().unwrap_or(Type::Unknown),
                            ),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn statements(&mut self, statements: &[Stmt], returns: &mut Vec<Type>) -> Type {
        let mut result = Type::Unit;
        for statement in statements {
            result = self.statement(statement, returns);
            if !matches!(
                statement,
                Stmt::Expr {
                    terminated: false,
                    ..
                }
            ) {
                result = Type::Unit;
            }
        }
        result
    }

    fn statement(&mut self, statement: &Stmt, returns: &mut Vec<Type>) -> Type {
        match statement {
            Stmt::Public { statement, .. } => self.statement(statement, returns),
            Stmt::Module {
                name,
                name_span,
                statements,
                ..
            } => {
                self.define_binding(name, *name_span, Binding { ty: Type::Unknown });
                if let Some(statements) = statements {
                    self.with_scope_value(|inferencer| inferencer.statements(statements, returns));
                }
                Type::Unit
            }
            Stmt::Use {
                path,
                alias,
                alias_span,
                span,
            } => {
                let name = alias.as_ref().or_else(|| path.last()).expect("use path");
                let name_span = alias_span
                    .unwrap_or_else(|| Span::new(span.end - 1 - name.len(), span.end - 1));
                let path_name = path.join("::");
                let ty = crate::standard_library::standard_function_signature(&path_name)
                    .map_or_else(
                        || {
                            if name.chars().next().is_some_and(char::is_uppercase) {
                                Type::named(path_name)
                            } else {
                                Type::Unknown
                            }
                        },
                        |signature| signature.as_type(),
                    );
                self.define_binding(name, name_span, Binding { ty });
                Type::Unit
            }
            Stmt::Let {
                name,
                name_span,
                type_annotation,
                initializer,
                ..
            } => {
                let inferred = self.expression(initializer, returns);
                let ty = type_annotation.clone().unwrap_or(inferred);
                self.define_binding(name, *name_span, Binding { ty: ty.clone() });
                if type_annotation.is_none() {
                    self.type_hint(*name_span, ty, ": ");
                }
                Type::Unit
            }
            Stmt::Function {
                name,
                name_span,
                parameters,
                return_type,
                body,
                ..
            } => {
                let parameter_types = parameters
                    .iter()
                    .map(|parameter| parameter.type_annotation.clone().unwrap_or(Type::Unknown))
                    .collect::<Vec<_>>();
                self.scopes.last_mut().expect("scope exists").insert(
                    name.clone(),
                    Binding {
                        ty: Type::function(
                            parameter_types.clone(),
                            return_type.clone().unwrap_or(Type::Unknown),
                        ),
                    },
                );
                let resolved = self.with_scope_value(|inferencer| {
                    for parameter in parameters {
                        let ty = parameter.type_annotation.clone().unwrap_or(Type::Unknown);
                        inferencer.define_binding(&parameter.name, parameter.span, Binding { ty });
                    }
                    let mut explicit_returns = Vec::new();
                    let tail = inferencer.block_contents(body, &mut explicit_returns);
                    return_type
                        .clone()
                        .unwrap_or_else(|| inferred_return(explicit_returns, tail))
                });
                let signature = Type::function(parameter_types, resolved.clone());
                self.result
                    .binding_types
                    .insert(*name_span, signature.clone());
                if return_type.is_none() && is_known(&resolved) {
                    self.result.hints.push(RawTypeHint {
                        position: body.span.start,
                        span: *name_span,
                        ty: resolved.clone(),
                        prefix: " -> ",
                    });
                }
                if let Some(binding) = self.scopes.last_mut().and_then(|scope| scope.get_mut(name))
                {
                    binding.ty = signature;
                }
                Type::Unit
            }
            Stmt::Struct {
                name, name_span, ..
            }
            | Stmt::Enum {
                name, name_span, ..
            } => {
                self.scopes.last_mut().expect("scope exists").insert(
                    name.clone(),
                    Binding {
                        ty: Type::Named {
                            name: name.clone(),
                            arguments: Vec::new(),
                        },
                    },
                );
                self.result.binding_types.insert(
                    *name_span,
                    Type::Named {
                        name: name.clone(),
                        arguments: Vec::new(),
                    },
                );
                Type::Unit
            }
            Stmt::TypeAlias { .. } => Type::Unit,
            Stmt::Impl { methods, .. } => {
                for method in methods {
                    self.with_scope_value(|inferencer| {
                        let parameter_types = method
                            .parameters
                            .iter()
                            .map(|parameter| {
                                parameter.type_annotation.clone().unwrap_or(Type::Unknown)
                            })
                            .collect::<Vec<_>>();
                        for parameter in &method.parameters {
                            let ty = parameter.type_annotation.clone().unwrap_or(Type::Unknown);
                            inferencer.define_binding(
                                &parameter.name,
                                parameter.span,
                                Binding { ty },
                            );
                        }
                        let mut method_returns = Vec::new();
                        let tail = inferencer.block_contents(&method.body, &mut method_returns);
                        let resolved = method
                            .return_type
                            .clone()
                            .unwrap_or_else(|| inferred_return(method_returns, tail));
                        inferencer.result.binding_types.insert(
                            method.name_span,
                            Type::function(parameter_types, resolved.clone()),
                        );
                        if method.return_type.is_none() && is_known(&resolved) {
                            inferencer.result.hints.push(RawTypeHint {
                                position: method.body.span.start,
                                span: method.name_span,
                                ty: resolved,
                                prefix: " -> ",
                            });
                        }
                    });
                }
                Type::Unit
            }
            Stmt::Trait { methods, .. } => {
                for method in methods {
                    if let Some(return_type) = &method.return_type {
                        self.result.binding_types.insert(
                            method.name_span,
                            Type::function(
                                method
                                    .parameters
                                    .iter()
                                    .map(|parameter| {
                                        parameter.type_annotation.clone().unwrap_or(Type::Unknown)
                                    })
                                    .collect(),
                                return_type.clone(),
                            ),
                        );
                    }
                }
                Type::Unit
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expression(condition, returns);
                self.block(body, returns);
                Type::Unit
            }
            Stmt::Loop { body, .. } => {
                self.with_scope_value(|inferencer| inferencer.block_contents(body, returns));
                Type::Unknown
            }
            Stmt::For {
                binding,
                binding_span,
                iterable,
                body,
                ..
            } => {
                self.expression(iterable, returns);
                self.with_scope_value(|inferencer| {
                    inferencer.define_binding(
                        binding,
                        *binding_span,
                        Binding { ty: Type::Unknown },
                    );
                    inferencer.block_contents(body, returns);
                });
                Type::Unit
            }
            Stmt::Return { value, .. } => {
                let return_type = if let Some(value) = value {
                    self.expression(value, returns)
                } else {
                    Type::Unit
                };
                returns.push(return_type);
                Type::Unit
            }
            Stmt::Break { value, .. } => value
                .as_ref()
                .map(|value| self.expression(value, returns))
                .unwrap_or(Type::Unit),
            Stmt::Continue { .. } => Type::Unit,
            Stmt::Expr { expression, .. } => self.expression(expression, returns),
        }
    }

    fn expression(&mut self, expression: &Expr, returns: &mut Vec<Type>) -> Type {
        let ty = self.expression_inner(expression, returns);
        self.result
            .expression_types
            .insert(expression.span(), ty.clone());
        ty
    }

    fn expression_inner(&mut self, expression: &Expr, returns: &mut Vec<Type>) -> Type {
        match expression {
            Expr::Literal { value, .. } => literal_type(value),
            Expr::Variable { name, .. } => self
                .lookup(name)
                .map_or(Type::Unknown, |binding| binding.ty.clone()),
            Expr::Path { segments, .. } => {
                crate::standard_library::standard_function_signature(&segments.join("::"))
                    .map(|signature| signature.as_type())
                    .or_else(|| {
                        segments
                            .first()
                            .and_then(|name| {
                                self.types.contains_key(name).then(|| Type::Named {
                                    name: name.clone(),
                                    arguments: Vec::new(),
                                })
                            })
                            .or_else(|| {
                                segments.last().and_then(|variant| {
                                    self.variant_owners.get(variant).map(|owner| Type::Named {
                                        name: owner.clone(),
                                        arguments: Vec::new(),
                                    })
                                })
                            })
                    })
                    .unwrap_or(Type::Unknown)
            }
            Expr::QualifiedPath { .. } => Type::opaque_function(),
            Expr::Member { object, name, .. } => {
                let object_type = self.expression(object, returns);
                self.field_type(&object_type, name)
            }
            Expr::Index { object, index, .. } => {
                let object = self.expression(object, returns);
                self.expression(index, returns);
                match object {
                    Type::Array { element, .. } => *element,
                    Type::Named { name, arguments } if name == "Vec" => {
                        arguments.into_iter().next().unwrap_or(Type::Unknown)
                    }
                    _ => Type::Unknown,
                }
            }
            Expr::Tuple { elements, .. } => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.expression(element, returns))
                    .collect(),
            ),
            Expr::Array {
                elements, repeat, ..
            } => {
                let mut element_type = Type::Unknown;
                for element in elements {
                    let actual = self.expression(element, returns);
                    element_type = merge_types(&element_type, &actual).unwrap_or(Type::Unknown);
                }
                let length = repeat
                    .as_ref()
                    .and_then(|repeat| match repeat.as_ref() {
                        Expr::Literal {
                            value: Literal::Integer(value),
                            ..
                        } => usize::try_from(*value).ok(),
                        _ => None,
                    })
                    .unwrap_or(elements.len());
                if let Some(repeat) = repeat {
                    self.expression(repeat, returns);
                }
                Type::Array {
                    element: Box::new(element_type),
                    length,
                }
            }
            Expr::Try { operand, .. } => match self.expression(operand, returns) {
                Type::Result(ok, _) => *ok,
                _ => Type::Unknown,
            },
            Expr::RecordLiteral { path, fields, .. } => {
                for (_, value) in fields {
                    self.expression(value, returns);
                }
                path.first().map_or(Type::Unknown, |name| Type::Named {
                    name: name.clone(),
                    arguments: Vec::new(),
                })
            }
            Expr::Assign { target, value, .. } => {
                self.expression(target, returns);
                self.expression(value, returns);
                Type::Unit
            }
            Expr::Borrow {
                mutable, target, ..
            } => Type::Reference {
                mutable: *mutable,
                inner: Box::new(self.expression(target, returns)),
            },
            Expr::Unary {
                operator, operand, ..
            } => match operator {
                UnaryOp::Not => {
                    self.expression(operand, returns);
                    Type::Bool
                }
                UnaryOp::Negate => self.expression(operand, returns),
                UnaryOp::Dereference => match self.expression(operand, returns) {
                    Type::Reference { inner, .. } => *inner,
                    _ => Type::Unknown,
                },
            },
            Expr::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let left = self.expression(left, returns);
                let right = self.expression(right, returns);
                match operator {
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual => Type::Bool,
                    _ => merge_types(&left, &right).unwrap_or(Type::Unknown),
                }
            }
            Expr::Logical { left, right, .. } => {
                self.expression(left, returns);
                self.expression(right, returns);
                Type::Bool
            }
            Expr::Range { start, end, .. } => {
                self.expression(start, returns);
                self.expression(end, returns);
                Type::named("Range")
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                let callee_type = self.expression(callee, returns);
                let argument_types = arguments
                    .iter()
                    .map(|argument| self.expression(argument, returns))
                    .collect::<Vec<_>>();
                if let Expr::Variable { name, .. } = callee.as_ref() {
                    return match name.as_str() {
                        "Some" => Type::Option(Box::new(
                            argument_types.first().cloned().unwrap_or(Type::Unknown),
                        )),
                        "Ok" => Type::Result(
                            Box::new(argument_types.first().cloned().unwrap_or(Type::Unknown)),
                            Box::new(Type::Unknown),
                        ),
                        "Err" => Type::Result(
                            Box::new(Type::Unknown),
                            Box::new(argument_types.first().cloned().unwrap_or(Type::Unknown)),
                        ),
                        "is_ok" | "is_err" => Type::Bool,
                        "None" => Type::Option(Box::new(Type::Unknown)),
                        "unwrap" => value_inner(argument_types.first().cloned()),
                        "unwrap_or" => argument_types
                            .get(1)
                            .cloned()
                            .unwrap_or_else(|| option_inner(argument_types.first().cloned())),
                        "clone" => match argument_types.first() {
                            Some(Type::Reference { inner, .. }) => (**inner).clone(),
                            _ => Type::Unknown,
                        },
                        _ => function_call_result(&callee_type, &argument_types),
                    };
                }
                function_call_result(&callee_type, &argument_types)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expression(condition, returns);
                let then_type = self.block(then_branch, returns);
                let else_type = else_branch
                    .as_ref()
                    .map_or(Type::Unit, |branch| self.expression(branch, returns));
                merge_types(&then_type, &else_type).unwrap_or(Type::Unknown)
            }
            Expr::Match { value, arms, .. } => {
                let value_type = self.expression(value, returns);
                let mut arm_types = Vec::new();
                for arm in arms {
                    arm_types.push(self.with_scope_value(|inferencer| {
                        inferencer.pattern(&arm.pattern, &value_type);
                        inferencer.expression(&arm.expression, returns)
                    }));
                }
                merge_all(arm_types)
            }
            Expr::Block(block) => self.block(block, returns),
        }
    }

    fn pattern(&mut self, pattern: &Pattern, expected: &Type) {
        match pattern {
            Pattern::Binding { name, span } => {
                self.define_binding(
                    name,
                    *span,
                    Binding {
                        ty: expected.clone(),
                    },
                );
                self.type_hint(*span, expected.clone(), ": ");
            }
            Pattern::Some { inner, .. } => {
                self.pattern(inner, &option_inner(Some(expected.clone())));
            }
            Pattern::Ok { inner, .. } => {
                let ty = match expected {
                    Type::Result(ok, _) => (**ok).clone(),
                    _ => Type::Unknown,
                };
                self.pattern(inner, &ty);
            }
            Pattern::Err { inner, .. } => {
                let ty = match expected {
                    Type::Result(_, error) => (**error).clone(),
                    _ => Type::Unknown,
                };
                self.pattern(inner, &ty);
            }
            Pattern::TupleVariant { path, fields, .. } => {
                let payload = path
                    .last()
                    .and_then(|variant| {
                        self.variant_owners
                            .get(variant)
                            .and_then(|owner| self.types.get(owner))
                            .and_then(|definition| definition.variants.get(variant))
                    })
                    .and_then(|variant| match variant {
                        VariantDefinition::Tuple(fields) => Some(fields.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                for (field, ty) in fields.iter().zip(payload.iter()) {
                    self.pattern(field, ty);
                }
            }
            Pattern::Record { path, fields, .. } => {
                let record_fields = path.last().and_then(|name| {
                    if let Some(owner) = self.variant_owners.get(name) {
                        self.types
                            .get(owner)
                            .and_then(|definition| definition.variants.get(name))
                            .and_then(|variant| match variant {
                                VariantDefinition::Record(fields) => Some(fields.clone()),
                                _ => None,
                            })
                    } else if let Type::Named { name, .. } = expected {
                        self.types
                            .get(name)
                            .map(|definition| definition.fields.clone())
                    } else {
                        None
                    }
                });
                for (name, pattern) in fields {
                    let field_type = record_fields
                        .as_ref()
                        .and_then(|fields| fields.get(name))
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    self.pattern(pattern, &field_type);
                }
            }
            Pattern::Wildcard { .. }
            | Pattern::Literal { .. }
            | Pattern::None { .. }
            | Pattern::Path { .. } => {}
        }
    }

    fn block(&mut self, block: &Block, returns: &mut Vec<Type>) -> Type {
        self.with_scope_value(|inferencer| inferencer.block_contents(block, returns))
    }

    fn block_contents(&mut self, block: &Block, returns: &mut Vec<Type>) -> Type {
        self.statements(&block.statements, returns)
    }

    fn field_type(&self, object_type: &Type, field: &str) -> Type {
        if let Type::Tuple(elements) = object_type {
            return field
                .parse::<usize>()
                .ok()
                .and_then(|index| elements.get(index))
                .cloned()
                .unwrap_or(Type::Unknown);
        }
        if matches!(object_type, Type::Array { .. }) && field == "len" {
            return Type::function(Vec::new(), Type::Int);
        }
        if let Type::Named { name, arguments } = object_type
            && name == "Vec"
        {
            let item = arguments.first().cloned().unwrap_or(Type::Unknown);
            return match field {
                "len" => Type::function(Vec::new(), Type::Int),
                "push" => Type::function(vec![item.clone()], Type::Unit),
                "pop" => Type::function(Vec::new(), Type::Option(Box::new(item))),
                _ => Type::Unknown,
            };
        }
        let Type::Named { name, .. } = object_type else {
            return Type::Unknown;
        };
        self.types.get(name).map_or(Type::Unknown, |definition| {
            definition
                .fields
                .get(field)
                .or_else(|| definition.methods.get(field))
                .cloned()
                .unwrap_or(Type::Unknown)
        })
    }

    fn define_binding(&mut self, name: &str, span: Span, binding: Binding) {
        self.result.binding_types.insert(span, binding.ty.clone());
        self.scopes
            .last_mut()
            .expect("scope exists")
            .insert(name.into(), binding);
    }

    fn type_hint(&mut self, span: Span, ty: Type, prefix: &'static str) {
        if is_known(&ty) {
            self.result.hints.push(RawTypeHint {
                position: span.end,
                span,
                ty,
                prefix,
            });
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn with_scope_value<T>(&mut self, action: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push(HashMap::new());
        let result = action(self);
        self.scopes.pop();
        result
    }
}

fn literal_type(literal: &Literal) -> Type {
    match literal {
        Literal::Unit => Type::Unit,
        Literal::Bool(_) => Type::Bool,
        Literal::Integer(_) => Type::Int,
        Literal::Float(_) => Type::Float,
        Literal::String(_) => Type::String,
    }
}

fn option_inner(ty: Option<Type>) -> Type {
    match ty {
        Some(Type::Option(inner)) => *inner,
        _ => Type::Unknown,
    }
}

fn value_inner(ty: Option<Type>) -> Type {
    match ty {
        Some(Type::Option(inner)) | Some(Type::Result(inner, _)) => *inner,
        _ => Type::Unknown,
    }
}

fn merge_all(types: impl IntoIterator<Item = Type>) -> Type {
    types
        .into_iter()
        .reduce(|left, right| merge_types(&left, &right).unwrap_or(Type::Unknown))
        .unwrap_or(Type::Unit)
}

fn inferred_return(explicit_returns: Vec<Type>, tail: Type) -> Type {
    if explicit_returns.is_empty() {
        tail
    } else if tail == Type::Unit {
        merge_all(explicit_returns)
    } else {
        merge_all(explicit_returns.into_iter().chain([tail]))
    }
}

fn function_call_result(function: &Type, arguments: &[Type]) -> Type {
    let Type::Function {
        parameters,
        return_type,
    } = function
    else {
        return Type::Unknown;
    };
    let mut substitutions = HashMap::new();
    if let Some(parameters) = parameters {
        if parameters.len() != arguments.len() {
            return Type::Unknown;
        }
        for (expected, actual) in parameters.iter().zip(arguments) {
            infer_type_variables(expected, actual, &mut substitutions);
        }
    }
    return_type.substitute(&substitutions)
}

fn infer_type_variables(expected: &Type, actual: &Type, substitutions: &mut HashMap<String, Type>) {
    match (expected, actual) {
        (Type::Variable(name), actual) => {
            let inferred = substitutions
                .get(name)
                .and_then(|current| merge_types(current, actual))
                .unwrap_or_else(|| actual.clone());
            substitutions.insert(name.clone(), inferred);
        }
        (Type::Option(expected), Type::Option(actual)) => {
            infer_type_variables(expected, actual, substitutions);
        }
        (Type::Result(expected_ok, expected_error), Type::Result(actual_ok, actual_error)) => {
            infer_type_variables(expected_ok, actual_ok, substitutions);
            infer_type_variables(expected_error, actual_error, substitutions);
        }
        (
            Type::Reference {
                inner: expected, ..
            },
            Type::Reference { inner: actual, .. },
        ) => infer_type_variables(expected, actual, substitutions),
        (
            Type::Function {
                parameters: Some(expected_parameters),
                return_type: expected_return,
            },
            Type::Function {
                parameters: Some(actual_parameters),
                return_type: actual_return,
            },
        ) => {
            for (expected, actual) in expected_parameters.iter().zip(actual_parameters) {
                infer_type_variables(expected, actual, substitutions);
            }
            infer_type_variables(expected_return, actual_return, substitutions);
        }
        (
            Type::Named {
                name: expected_name,
                arguments: expected_arguments,
            },
            Type::Named {
                name: actual_name,
                arguments: actual_arguments,
            },
        ) if expected_name == actual_name => {
            for (expected, actual) in expected_arguments.iter().zip(actual_arguments) {
                infer_type_variables(expected, actual, substitutions);
            }
        }
        _ => {}
    }
}

fn is_known(ty: &Type) -> bool {
    match ty {
        Type::Unknown => false,
        Type::Option(inner) => is_known(inner),
        Type::Result(ok, error) => is_known(ok) && is_known(error),
        Type::Reference { inner, .. } => is_known(inner),
        Type::Function {
            parameters: Some(parameters),
            return_type,
        } => parameters.iter().all(is_known) && is_known(return_type),
        Type::Function {
            parameters: None, ..
        } => false,
        Type::Named { arguments, .. } => arguments.iter().all(is_known),
        _ => true,
    }
}
