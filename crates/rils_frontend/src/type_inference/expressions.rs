//! Infer expressions and propagate concrete contexts before storage construction.

use super::*;

impl Inferencer<'_> {
    pub(super) fn expression(&mut self, expression: &Expr, returns: &mut Vec<Type>) -> Type {
        let id = self.expression_ids.id(expression);
        let mut ty = self.expression_inner(expression, id, returns);
        if let Expr::GenericPath {
            segments,
            arguments,
            ..
        } = expression
            && let Some((member, owner)) = segments.split_last()
            && let Some(owner) = crate::standard_library::builtin_type_name(&owner.join("::"))
            && let Some(declaration) = rils_builtins::builtin(owner)
            && declaration.type_parameters.len() == arguments.len()
            && crate::standard_library::builtin_associated_function_signature(owner, member)
                .is_some()
        {
            let substitutions = declaration
                .type_parameters
                .iter()
                .zip(arguments)
                .map(|(parameter, argument)| {
                    (
                        (*parameter).to_owned(),
                        self.host_types.resolved_type(argument),
                    )
                })
                .collect();
            ty = ty.substitute(&substitutions);
        }
        let ty = self.declaration_types.resolve(&ty, &self.module_path);
        self.result.expression_types_by_id.insert(id, ty.clone());
        ty
    }

    fn expression_inner(&mut self, expression: &Expr, id: ExprId, returns: &mut Vec<Type>) -> Type {
        match expression {
            Expr::Literal { value, .. } => literal_type(value, id),
            Expr::Variable { name, .. } if name == "None" => Type::Option(Box::new(Type::Unknown)),
            Expr::Variable { name, .. } => self
                .lookup(name)
                .map_or(Type::Unknown, |binding| binding.ty.clone()),
            Expr::Path { segments, .. } | Expr::GenericPath { segments, .. } => {
                let segments = self
                    .host_types
                    .resolved_expression_path(expression)
                    .unwrap_or(segments);
                self.host_functions
                    .get(&segments.join("::"))
                    .cloned()
                    .or_else(|| {
                        // Host manifests expose associated functions under a
                        // snake-case module (`unity_engine::color::new`) while
                        // source paths use the host type (`Color::new`).
                        let type_index = segments.len().checked_sub(2)?;
                        let member = segments.last()?;
                        let module = segments[..type_index].join("::");
                        let type_module = snake_case(&segments[type_index]);
                        let qualified = if module.is_empty() {
                            format!("{type_module}::{member}")
                        } else {
                            format!("{module}::{type_module}::{member}")
                        };
                        self.host_functions.get(&qualified).cloned()
                    })
                    .or_else(|| {
                        crate::standard_library::standard_function_signature(&segments.join("::"))
                    })
                    .map(|signature| signature.as_type())
                    .or_else(|| {
                        let (member, owner) = segments.split_last()?;
                        crate::standard_library::builtin_associated_function_signature(
                            &owner.join("::"),
                            member,
                        )
                        .map(|signature| signature.as_type())
                    })
                    .or_else(|| {
                        let [type_name, member] = segments else {
                            return None;
                        };
                        let owner = if type_name == "Self" {
                            match self.lookup("Self").map(|binding| &binding.ty) {
                                Some(Type::Named { name, .. }) => name.as_str(),
                                _ => return None,
                            }
                        } else {
                            type_name.as_str()
                        };
                        let definition = self.types.get(owner)?;
                        let method = definition.methods.get(member)?.clone();
                        if let Some(receiver) = definition.method_receivers.get(member)
                            && let Type::Function {
                                parameters: Some(parameters),
                                return_type,
                            } = method
                        {
                            let mut parameters = parameters;
                            parameters.insert(0, receiver.clone());
                            Some(Type::function(parameters, *return_type))
                        } else {
                            Some(method)
                        }
                    })
                    .or_else(|| self.nominal_variant_type(expression))
                    .or_else(|| {
                        segments
                            .first()
                            .and_then(|name| {
                                if name == "Self" {
                                    self.lookup("Self").map(|binding| binding.ty.clone())
                                } else {
                                    self.types.contains_key(name).then(|| Type::Named {
                                        name: name.clone(),
                                        arguments: Vec::new(),
                                    })
                                }
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
                        let [type_name, member] = segments else {
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
                    .unwrap_or(Type::Unknown)
            }
            Expr::QualifiedPath {
                target,
                trait_name,
                member,
                ..
            } if trait_name.rsplit("::").next() == Some("IntoIterator")
                && member == "into_iter" =>
            {
                let target = self.syntax_type(target);
                Type::function(vec![target.clone()], target)
            }
            Expr::QualifiedPath {
                target,
                trait_name,
                member,
                ..
            } => {
                let target = self.syntax_type(target);
                let trait_name = trait_name.rsplit("::").next().unwrap_or(trait_name);
                let Some(Type::Function {
                    parameters: Some(mut parameters),
                    return_type,
                }) =
                    crate::standard_library::builtin_trait_member_type(trait_name, &target, member)
                else {
                    return Type::opaque_function();
                };
                if let Some(receiver) = rils_builtins::builtin_member(trait_name, member)
                    .and_then(|member| member.receiver)
                {
                    parameters.insert(
                        0,
                        match receiver {
                            rils_builtins::ReceiverMode::Owned => target,
                            rils_builtins::ReceiverMode::Shared
                            | rils_builtins::ReceiverMode::Mutable => Type::Reference {
                                mutable: receiver == rils_builtins::ReceiverMode::Mutable,
                                inner: Box::new(target),
                            },
                        },
                    );
                }
                Type::function(parameters, *return_type)
            }
            Expr::Cast {
                operand, target, ..
            } => {
                self.expression(operand, returns);
                self.syntax_type(target)
            }
            Expr::Member { object, name, .. } => {
                let object_type = self.expression(object, returns);
                self.field_type(&object_type, name)
            }
            Expr::Index { object, index, .. } => {
                let object = self.expression(object, returns);
                let index_type = self.expression(index, returns);
                self.unify(&index_type, &Type::USIZE);
                let object = match object {
                    Type::Reference { inner, .. } => *inner,
                    object => object,
                };
                match object {
                    Type::Array { element, .. } | Type::Slice(element) => *element,
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
                let mut element_type: Option<Type> = None;
                for element in elements {
                    let actual = self.expression(element, returns);
                    element_type = Some(if let Some(current) = element_type {
                        if (current.is_integer() && actual.is_integer())
                            || (current.is_float() && actual.is_float())
                        {
                            self.unify(&current, &actual);
                            current
                        } else {
                            merge_types(&current, &actual).unwrap_or(Type::Unknown)
                        }
                    } else {
                        actual
                    });
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
                if let Some(element_type) = &element_type {
                    for element in elements {
                        self.apply_expected_type(element, element_type);
                    }
                }
                Type::Array {
                    element: Box::new(element_type.unwrap_or(Type::Unknown)),
                    length,
                }
            }
            Expr::Try { operand, .. } => match self.expression(operand, returns) {
                Type::Result(ok, error) => {
                    returns.push(Type::Result(Box::new(Type::Unknown), error));
                    *ok
                }
                _ => Type::Unknown,
            },
            Expr::RecordLiteral { path, fields, .. } => self.record_literal(path, fields, returns),
            Expr::Assign { target, value, .. } => {
                let target = self.expression(target, returns);
                let value_type = self.expression(value, returns);
                self.unify(&target, &value_type);
                self.apply_expected_type(value, &target);
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
                let left_type = self.expression(left, returns);
                let right_type = self.expression(right, returns);
                self.unify(&left_type, &right_type);
                if let Some(expected) = merge_types(
                    &self.resolve_type(&left_type),
                    &self.resolve_type(&right_type),
                ) {
                    self.apply_expected_type(left, &expected);
                    self.apply_expected_type(right, &expected);
                }
                match operator {
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual => Type::Bool,
                    _ => merge_types(&left_type, &right_type).unwrap_or(Type::Unknown),
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
                let callee_type = self
                    .partial_member_call_type(callee, &argument_types, id)
                    .or_else(|| self.trait_call_type(callee, &argument_types))
                    .unwrap_or(callee_type);
                if let Type::Function {
                    parameters: Some(parameters),
                    ..
                } = &callee_type
                {
                    let mut substitutions = HashMap::new();
                    for (parameter, argument_type) in parameters.iter().zip(&argument_types) {
                        infer_type_variables(parameter, argument_type, &mut substitutions);
                    }
                    for ((parameter, argument_type), argument) in
                        parameters.iter().zip(&argument_types).zip(arguments)
                    {
                        self.unify(parameter, argument_type);
                        self.apply_expected_type(argument, &parameter.substitute(&substitutions));
                    }
                }
                if let Some(ty) = self.tuple_variant_type(callee, arguments, &argument_types) {
                    return ty;
                }
                if let Some(name) = self.sum_constructor(callee) {
                    self.result.sum_constructors.insert(id, name);
                    let payload =
                        Box::new(argument_types.first().cloned().unwrap_or(Type::Unknown));
                    return match name {
                        "Some" => Type::Option(payload),
                        "Ok" => Type::Result(payload, Box::new(Type::Unknown)),
                        "Err" => Type::Result(Box::new(Type::Unknown), payload),
                        _ => unreachable!(),
                    };
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
                        "BTreeMap::new" | "std::collections::BTreeMap::new" => {
                            return Type::Named {
                                name: "BTreeMap".into(),
                                arguments: vec![Type::Unknown, Type::Unknown],
                            };
                        }
                        "HashSet::new" | "std::collections::HashSet::new" => {
                            return Type::Named {
                                name: "HashSet".into(),
                                arguments: vec![Type::Unknown],
                            };
                        }
                        "BTreeSet::new" | "std::collections::BTreeSet::new" => {
                            return Type::Named {
                                name: "BTreeSet".into(),
                                arguments: vec![Type::Unknown],
                            };
                        }
                        _ => {}
                    }
                }
                if let Expr::Variable { name, .. } = callee.as_ref() {
                    return match name.as_str() {
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
                let merged = merge_types(&then_type, &else_type).unwrap_or(Type::Unknown);
                self.apply_expected_block_tail(then_branch, &merged);
                if let Some(branch) = else_branch {
                    self.apply_expected_type(branch, &merged);
                }
                merged
            }
            Expr::Match { value, arms, .. } => {
                let value_type = self.expression(value, returns);
                let mut arm_types = Vec::new();
                for arm in arms {
                    arm_types.push(self.with_scope_value(|inferencer| {
                        inferencer.pattern(
                            &arm.pattern,
                            match &value_type {
                                Type::Reference { inner, .. } => inner,
                                ty => ty,
                            },
                            matches!(value_type, Type::Reference { .. }),
                        );
                        inferencer.expression(&arm.expression, returns)
                    }));
                }
                let merged = merge_all(arm_types);
                for arm in arms {
                    self.apply_expected_type(&arm.expression, &merged);
                }
                merged
            }
            Expr::Block(block) => self.block(block, returns),
        }
    }
}
