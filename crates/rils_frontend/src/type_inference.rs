use std::collections::{HashMap, HashSet};

use crate::{
    ast::{BinaryOp, Block, EnumVariant, Expr, Literal, Pattern, Program, Stmt, UnaryOp},
    source::Span,
    types::{FunctionSignature, Type, merge_types},
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
    generic_parameters: Vec<String>,
    fields: HashMap<String, Type>,
    variants: HashMap<String, VariantDefinition>,
    methods: HashMap<String, Type>,
    implemented_traits: HashSet<String>,
}

#[derive(Clone)]
enum VariantDefinition {
    Unit,
    Tuple(Vec<Type>),
    Record(HashMap<String, Type>),
}

pub(crate) fn infer_with_host_functions(
    program: &Program,
    host_functions: &HashMap<String, FunctionSignature>,
) -> InferenceResult {
    Inferencer::new(program, host_functions).run(program)
}

struct Inferencer {
    scopes: Vec<HashMap<String, Binding>>,
    types: HashMap<String, TypeDefinition>,
    variant_owners: HashMap<String, String>,
    result: InferenceResult,
    numeric_parents: HashMap<Span, Span>,
    numeric_fixed: HashMap<Span, Type>,
    host_functions: HashMap<String, FunctionSignature>,
}

impl Inferencer {
    fn new(program: &Program, host_functions: &HashMap<String, FunctionSignature>) -> Self {
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
        for builtin in rils_builtins::BUILTINS.iter().filter(|builtin| {
            builtin.kind == rils_builtins::BuiltinKind::Function && !builtin.path.contains("::")
        }) {
            if let Some(signature) =
                crate::standard_library::standard_function_signature(builtin.path)
            {
                globals.insert(
                    builtin.path.into(),
                    Binding {
                        ty: signature.as_type(),
                    },
                );
            }
        }
        for (name, signature) in host_functions {
            if !name.contains("::") {
                globals.insert(
                    name.clone(),
                    Binding {
                        ty: signature.as_type(),
                    },
                );
            }
        }
        for integer in crate::types::IntegerType::ALL {
            globals.insert(
                integer.name().into(),
                Binding {
                    ty: Type::Integer(integer),
                },
            );
        }
        for float in [crate::types::FloatType::F32, crate::types::FloatType::F64] {
            globals.insert(
                float.name().into(),
                Binding {
                    ty: Type::Float(float),
                },
            );
        }

        let mut inferencer = Self {
            scopes: vec![globals],
            types: HashMap::new(),
            variant_owners: HashMap::new(),
            result: InferenceResult::default(),
            numeric_parents: HashMap::new(),
            numeric_fixed: HashMap::new(),
            host_functions: host_functions.clone(),
        };
        inferencer.collect_type_definitions(&program.statements);
        inferencer
    }

    fn run(mut self, program: &Program) -> InferenceResult {
        let mut returns = Vec::new();
        self.statements(&program.statements, &mut returns);
        let binding_spans = self
            .result
            .binding_types
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for span in binding_spans {
            if let Some(ty) = self.result.binding_types.get(&span).cloned() {
                let resolved = self.resolve_type(&ty);
                self.result.binding_types.insert(span, resolved);
            }
        }
        let expression_spans = self
            .result
            .expression_types
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for span in expression_spans {
            if let Some(ty) = self.result.expression_types.get(&span).cloned() {
                let resolved = self.resolve_type(&ty);
                self.result.expression_types.insert(span, resolved);
            }
        }
        for index in 0..self.result.hints.len() {
            let ty = self.result.hints[index].ty.clone();
            self.result.hints[index].ty = self.resolve_type(&ty);
        }
        self.result
    }

    fn numeric_root(&mut self, span: Span) -> Span {
        let parent = *self.numeric_parents.entry(span).or_insert(span);
        if parent == span {
            span
        } else {
            let root = self.numeric_root(parent);
            self.numeric_parents.insert(span, root);
            root
        }
    }

    fn unify(&mut self, left: &Type, right: &Type) {
        match (left, right) {
            (Type::IntegerVariable(left), Type::IntegerVariable(right))
            | (Type::FloatVariable(left), Type::FloatVariable(right)) => {
                let left = self.numeric_root(*left);
                let right = self.numeric_root(*right);
                if left != right {
                    let fixed = self
                        .numeric_fixed
                        .remove(&left)
                        .or_else(|| self.numeric_fixed.remove(&right));
                    self.numeric_parents.insert(right, left);
                    if let Some(fixed) = fixed {
                        self.numeric_fixed.insert(left, fixed);
                    }
                }
            }
            (Type::IntegerVariable(variable), fixed @ Type::Integer(_))
            | (fixed @ Type::Integer(_), Type::IntegerVariable(variable))
            | (Type::FloatVariable(variable), fixed @ Type::Float(_))
            | (fixed @ Type::Float(_), Type::FloatVariable(variable)) => {
                let root = self.numeric_root(*variable);
                self.numeric_fixed
                    .entry(root)
                    .or_insert_with(|| fixed.clone());
            }
            (Type::Option(left), Type::Option(right)) => self.unify(left, right),
            (Type::Result(left_ok, left_error), Type::Result(right_ok, right_error)) => {
                self.unify(left_ok, right_ok);
                self.unify(left_error, right_error);
            }
            (Type::Tuple(left), Type::Tuple(right)) if left.len() == right.len() => {
                for (left, right) in left.iter().zip(right) {
                    self.unify(left, right);
                }
            }
            (Type::Array { element: left, .. }, Type::Array { element: right, .. }) => {
                self.unify(left, right)
            }
            _ => {}
        }
    }

    fn resolve_type(&mut self, ty: &Type) -> Type {
        match ty {
            Type::IntegerVariable(span) => {
                let root = self.numeric_root(*span);
                self.numeric_fixed.get(&root).cloned().unwrap_or(Type::I32)
            }
            Type::FloatVariable(span) => {
                let root = self.numeric_root(*span);
                self.numeric_fixed.get(&root).cloned().unwrap_or(Type::F64)
            }
            Type::Option(inner) => Type::Option(Box::new(self.resolve_type(inner))),
            Type::Result(ok, error) => Type::Result(
                Box::new(self.resolve_type(ok)),
                Box::new(self.resolve_type(error)),
            ),
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.resolve_type(element))
                    .collect(),
            ),
            Type::Array { element, length } => Type::Array {
                element: Box::new(self.resolve_type(element)),
                length: *length,
            },
            Type::Reference { mutable, inner } => Type::Reference {
                mutable: *mutable,
                inner: Box::new(self.resolve_type(inner)),
            },
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters.as_ref().map(|parameters| {
                    parameters
                        .iter()
                        .map(|parameter| self.resolve_type(parameter))
                        .collect()
                }),
                return_type: Box::new(self.resolve_type(return_type)),
            },
            Type::Named { name, arguments } => Type::Named {
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| self.resolve_type(argument))
                    .collect(),
            },
            Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => Type::Associated {
                base: Box::new(self.resolve_type(base)),
                trait_name: trait_name.clone(),
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| self.resolve_type(argument))
                    .collect(),
            },
            other => other.clone(),
        }
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
                Stmt::Struct {
                    name,
                    generic_parameters,
                    fields,
                    ..
                } => {
                    self.types.insert(
                        name.clone(),
                        TypeDefinition {
                            generic_parameters: generic_parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            fields: fields
                                .iter()
                                .map(|field| (field.name.clone(), field.type_annotation.clone()))
                                .collect(),
                            variants: HashMap::new(),
                            methods: HashMap::new(),
                            implemented_traits: HashSet::new(),
                        },
                    );
                }
                Stmt::Enum {
                    name,
                    generic_parameters,
                    variants,
                    ..
                } => {
                    let mut definition = TypeDefinition {
                        generic_parameters: generic_parameters
                            .iter()
                            .map(|parameter| parameter.name.clone())
                            .collect(),
                        ..TypeDefinition::default()
                    };
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
                    target,
                    trait_name,
                    methods,
                    ..
                } => {
                    let Type::Named { name, .. } = target else {
                        continue;
                    };
                    let Some(definition) = self.types.get_mut(name) else {
                        continue;
                    };
                    if let Some(trait_name) = trait_name {
                        definition.implemented_traits.insert(trait_name.clone());
                    }
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
            Stmt::Use { imports, .. } => {
                for import in imports {
                    if import.kind == crate::ast::UseImportKind::Glob {
                        let prefix = format!("{}::", import.path.join("::"));
                        let mut bindings = self
                            .host_functions
                            .iter()
                            .filter_map(|(path, signature)| {
                                let name = path.strip_prefix(&prefix)?;
                                (!name.contains("::"))
                                    .then(|| (name.to_owned(), signature.as_type()))
                            })
                            .collect::<Vec<_>>();
                        for builtin in rils_builtins::BUILTINS.iter().filter(|builtin| {
                            builtin.path.starts_with(&prefix)
                                && !builtin.path[prefix.len()..].contains("::")
                        }) {
                            let name = builtin.path[prefix.len()..].to_owned();
                            let ty =
                                crate::standard_library::standard_function_signature(builtin.path)
                                    .map_or(Type::Unknown, |signature| signature.as_type());
                            bindings.push((name, ty));
                        }
                        for (name, ty) in bindings {
                            self.scopes
                                .last_mut()
                                .expect("scope exists")
                                .insert(name, Binding { ty });
                        }
                        continue;
                    }
                    let name = import.binding_name().expect("single use import");
                    let name_span = import.alias_span.unwrap_or(import.name_span);
                    let path_name = import.path.join("::");
                    let ty = self
                        .host_functions
                        .get(&path_name)
                        .cloned()
                        .or_else(|| {
                            crate::standard_library::standard_function_signature(&path_name)
                        })
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
                }
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
                if let Some(expected) = type_annotation {
                    self.unify(&inferred, expected);
                }
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
            Expr::Literal { value, span } => literal_type(value, *span),
            Expr::Variable { name, .. } => self
                .lookup(name)
                .map_or(Type::Unknown, |binding| binding.ty.clone()),
            Expr::Path { segments, .. } => self
                .host_functions
                .get(&segments.join("::"))
                .cloned()
                .or_else(|| {
                    crate::standard_library::standard_function_signature(&segments.join("::"))
                })
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
                .or_else(|| {
                    let [type_name, member] = segments.as_slice() else {
                        return None;
                    };
                    if let Some(integer) = crate::types::IntegerType::from_name(type_name) {
                        if let Some(constant) = rils_builtins::integer_constant(member) {
                            return Some(match constant.value_type {
                                rils_builtins::TypePattern::SelfType => Type::Integer(integer),
                                rils_builtins::TypePattern::U32 => {
                                    Type::Integer(crate::types::IntegerType::U32)
                                }
                                _ => Type::Unknown,
                            });
                        }
                        let intrinsic = rils_builtins::integer_associated_function(member)?;
                        return Some(crate::standard_library::integer_intrinsic_type(
                            intrinsic, integer,
                        ));
                    }
                    let float = crate::types::FloatType::from_name(type_name)?;
                    rils_builtins::float_constant(member).map(|_| Type::Float(float))
                })
                .unwrap_or(Type::Unknown),
            Expr::QualifiedPath { .. } => Type::opaque_function(),
            Expr::Cast {
                operand, target, ..
            } => {
                self.expression(operand, returns);
                target.clone()
            }
            Expr::Member { object, name, .. } => {
                let object_type = self.expression(object, returns);
                self.field_type(&object_type, name)
            }
            Expr::Index { object, index, .. } => {
                let object = self.expression(object, returns);
                let index_type = self.expression(index, returns);
                self.unify(&index_type, &Type::USIZE);
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
                            value: Literal::I32(value),
                            ..
                        } => usize::try_from(*value).ok(),
                        Expr::Literal {
                            value: Literal::Integer(value),
                            ..
                        } => usize::try_from(*value).ok(),
                        Expr::Literal {
                            value: Literal::Usize(value),
                            ..
                        } => Some(*value),
                        _ => None,
                    })
                    .unwrap_or(elements.len());
                if let Some(repeat) = repeat {
                    let repeat_type = self.expression(repeat, returns);
                    self.unify(&repeat_type, &Type::USIZE);
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
                let actual_fields = fields
                    .iter()
                    .map(|(name, value)| (name, self.expression(value, returns)))
                    .collect::<Vec<_>>();
                let Some(name) = path.first() else {
                    return Type::Unknown;
                };
                let Some(definition) = self.types.get(name).cloned() else {
                    return Type::Named {
                        name: name.clone(),
                        arguments: Vec::new(),
                    };
                };
                let declared_fields = path
                    .get(1)
                    .and_then(|variant| definition.variants.get(variant))
                    .and_then(|variant| match variant {
                        VariantDefinition::Record(fields) => Some(fields),
                        _ => None,
                    })
                    .unwrap_or(&definition.fields);
                let mut substitutions = HashMap::new();
                for (field, actual) in actual_fields {
                    if let Some(expected) = declared_fields.get(field) {
                        infer_type_variables(expected, &actual, &mut substitutions);
                    }
                }
                Type::Named {
                    name: name.clone(),
                    arguments: definition
                        .generic_parameters
                        .iter()
                        .map(|parameter| {
                            substitutions
                                .get(parameter)
                                .cloned()
                                .unwrap_or(Type::Unknown)
                        })
                        .collect(),
                }
            }
            Expr::Assign { target, value, .. } => {
                let target = self.expression(target, returns);
                let value = self.expression(value, returns);
                self.unify(&target, &value);
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
                self.unify(&left, &right);
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
                let start = self.expression(start, returns);
                let end = self.expression(end, returns);
                self.unify(&start, &end);
                Type::Named {
                    name: "Range".into(),
                    arguments: vec![merge_types(&start, &end).unwrap_or(start)],
                }
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                let callee_type = self.expression(callee, returns);
                let argument_types = arguments
                    .iter()
                    .map(|argument| self.expression(argument, returns))
                    .collect::<Vec<_>>();
                if let Type::Function {
                    parameters: Some(parameters),
                    ..
                } = &callee_type
                {
                    for (parameter, argument) in parameters.iter().zip(&argument_types) {
                        self.unify(parameter, argument);
                    }
                }
                if let Expr::Path { segments, .. } = callee.as_ref() {
                    match segments.join("::").as_str() {
                        "Vec::new" | "std::collections::Vec::new" => {
                            return Type::Named {
                                name: "Vec".into(),
                                arguments: vec![Type::Unknown],
                            };
                        }
                        "HashMap::new" | "std::collections::HashMap::new" => {
                            return Type::Named {
                                name: "HashMap".into(),
                                arguments: vec![Type::Unknown, Type::Unknown],
                            };
                        }
                        "HashSet::new" | "std::collections::HashSet::new" => {
                            return Type::Named {
                                name: "HashSet".into(),
                                arguments: vec![Type::Unknown],
                            };
                        }
                        "Vec::from" | "std::collections::Vec::from" => {
                            let item = match argument_types.first() {
                                Some(Type::Array { element, .. }) => (**element).clone(),
                                _ => Type::Unknown,
                            };
                            return Type::Named {
                                name: "Vec".into(),
                                arguments: vec![item],
                            };
                        }
                        _ => {}
                    }
                }
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
        if let Type::Integer(integer) = object_type
            && let Some(intrinsic) = rils_builtins::integer_method(field)
        {
            return crate::standard_library::integer_intrinsic_type(intrinsic, *integer);
        }
        if let Type::Float(float) = object_type
            && let Some(intrinsic) = rils_builtins::float_method(field)
        {
            return crate::standard_library::float_intrinsic_type(intrinsic, *float);
        }
        if let Type::Tuple(elements) = object_type {
            return field
                .parse::<usize>()
                .ok()
                .and_then(|index| elements.get(index))
                .cloned()
                .unwrap_or(Type::Unknown);
        }
        if let Some(member) = crate::standard_library::builtin_member_type(object_type, field) {
            return member;
        }
        if let Type::Named { name, arguments } = object_type
            && arguments.is_empty()
            && let Some(signature) = self.host_functions.get(&format!("{name}::{field}"))
        {
            let parameters = signature
                .parameters
                .as_ref()
                .map(|parameters| parameters.iter().skip(1).cloned().collect())
                .unwrap_or_default();
            return FunctionSignature::fixed(parameters, signature.return_type.clone()).as_type();
        }
        let Type::Named { name, arguments } = object_type else {
            return Type::Unknown;
        };
        self.types.get(name).map_or(Type::Unknown, |definition| {
            let substitutions = definition
                .generic_parameters
                .iter()
                .cloned()
                .zip(arguments.iter().cloned())
                .collect::<HashMap<_, _>>();
            let member = definition
                .fields
                .get(field)
                .or_else(|| definition.methods.get(field))
                .cloned()
                .or_else(|| {
                    (definition.implemented_traits.contains("Iterator")
                        && rils_builtins::is_iterator_default_method(field))
                    .then(|| {
                        crate::standard_library::builtin_trait_member_type(
                            "Iterator",
                            object_type,
                            field,
                        )
                    })
                    .flatten()
                });
            member
                .map(|member| member.substitute(&substitutions))
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

fn literal_type(literal: &Literal, span: Span) -> Type {
    match literal {
        Literal::Unit => Type::Unit,
        Literal::Bool(_) => Type::Bool,
        Literal::I8(_) => Type::Integer(crate::types::IntegerType::I8),
        Literal::I16(_) => Type::Integer(crate::types::IntegerType::I16),
        Literal::I32(_) => Type::I32,
        Literal::I64(_) => Type::Integer(crate::types::IntegerType::I64),
        Literal::I128(_) => Type::Integer(crate::types::IntegerType::I128),
        Literal::Isize(_) => Type::Integer(crate::types::IntegerType::Isize),
        Literal::U8(_) => Type::Integer(crate::types::IntegerType::U8),
        Literal::U16(_) => Type::Integer(crate::types::IntegerType::U16),
        Literal::U32(_) => Type::Integer(crate::types::IntegerType::U32),
        Literal::U64(_) => Type::Integer(crate::types::IntegerType::U64),
        Literal::U128(_) => Type::Integer(crate::types::IntegerType::U128),
        Literal::Usize(_) => Type::USIZE,
        Literal::F32(_) => Type::Float(crate::types::FloatType::F32),
        Literal::F64(_) => Type::F64,
        Literal::Char(_) => Type::Char,
        Literal::Integer(_) => Type::IntegerVariable(span),
        Literal::Float(_) => Type::FloatVariable(span),
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
