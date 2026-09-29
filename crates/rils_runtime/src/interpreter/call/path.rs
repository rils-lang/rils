use super::*;

pub(super) fn resolve_associated_path(
    base: Value,
    root: &str,
    member: &str,
    environment: EnvironmentRef,
    owner_environment: EnvironmentRef,
    span: Span,
) -> Result<Value, RuntimeError> {
    match base {
        Value::BuiltinType(BuiltinType::Vec) => match member {
            "new" => Ok(Value::BuiltinFunction(BuiltinFunction::VecNew)),
            "from" => Ok(Value::BuiltinFunction(BuiltinFunction::VecFrom)),
            _ => Err(RuntimeError::new(
                format!("Vec has no associated function `{member}`"),
                span,
            )),
        },
        Value::BuiltinType(BuiltinType::HashMap) => match member {
            "new" => Ok(Value::BuiltinFunction(BuiltinFunction::HashMapNew)),
            _ => Err(RuntimeError::new(
                format!("HashMap has no associated function `{member}`"),
                span,
            )),
        },
        Value::BuiltinType(BuiltinType::HashSet) => match member {
            "new" => Ok(Value::BuiltinFunction(BuiltinFunction::HashSetNew)),
            _ => Err(RuntimeError::new(
                format!("HashSet has no associated function `{member}`"),
                span,
            )),
        },
        Value::BuiltinType(BuiltinType::Integer(target)) => {
            if let Some(constant) = rils_builtins::integer_constant(member) {
                return Ok(crate::numeric::integer_constant(target, constant.id));
            }
            let intrinsic =
                rils_builtins::integer_associated_function(member).ok_or_else(|| {
                    RuntimeError::new(
                        format!("{target} has no associated function `{member}`"),
                        span,
                    )
                })?;
            Ok(Value::BuiltinFunction(BuiltinFunction::IntegerIntrinsic {
                symbol: intrinsic
                    .id
                    .canonical_path()
                    .expect("integer intrinsic has a canonical symbol"),
                target,
            }))
        }
        Value::BuiltinType(BuiltinType::Float(target)) => {
            let constant = rils_builtins::float_constant(member).ok_or_else(|| {
                RuntimeError::new(
                    format!("{target} has no associated constant `{member}`"),
                    span,
                )
            })?;
            Ok(crate::numeric::float_constant(target, constant.id))
        }
        Value::StructType(definition) if definition.name == "BTreeMap" && member == "new" => {
            Ok(Value::BuiltinFunction(BuiltinFunction::BTreeMapNew))
        }
        Value::StructType(definition) if definition.name == "BTreeSet" && member == "new" => {
            Ok(Value::BuiltinFunction(BuiltinFunction::BTreeSetNew))
        }
        Value::StructType(definition) => {
            if let Some(builtin) = rils_builtins::builtin_member(&definition.name, member)
                && builtin.kind == rils_builtins::BuiltinMemberKind::AssociatedFunction
                && let Some(symbol) =
                    builtin
                        .native_symbol
                        .or(builtin.runtime_import)
                        .or_else(|| {
                            builtin
                                .builtin_id
                                .and_then(rils_builtins::BuiltinId::canonical_path)
                        })
                && crate::runtime_builtins::requires_owned_native_call(symbol)
                && let Some(signature) =
                    rils_frontend::standard_library::erased_builtin_member_signature(builtin)
            {
                let arity = signature.parameters.as_ref().map_or(0, Vec::len);
                return Ok(Value::NativeFunction(NativeFunction {
                    binding_name: symbol,
                    name: symbol,
                    min_arity: arity,
                    max_arity: arity,
                    signature: Some(signature),
                    body: NativeFunctionBody::Symbol(symbol),
                }));
            }
            if let Some(builtin) = rils_builtins::builtin_member(&definition.name, member)
                && builtin.kind == rils_builtins::BuiltinMemberKind::AssociatedFunction
                && let Some(symbol) = builtin.native_symbol
                && let Some(signature) =
                    rils_frontend::standard_library::erased_builtin_member_signature(builtin)
            {
                let arity = signature.parameters.as_ref().map_or(0, Vec::len);
                return Ok(Value::NativeFunction(NativeFunction {
                    binding_name: symbol,
                    name: symbol,
                    min_arity: arity,
                    max_arity: arity,
                    signature: Some(signature),
                    body: NativeFunctionBody::Symbol(symbol),
                }));
            }
            definition
                .methods
                .borrow()
                .get(member)
                .cloned()
                .map(Value::Function)
                .ok_or_else(|| {
                    RuntimeError::new(
                        format!("struct `{root}` has no associated function `{member}`"),
                        span,
                    )
                })
        }
        Value::EnumType(definition) => {
            if let Some(method) = definition.methods.borrow().get(member).cloned() {
                return Ok(Value::Function(method));
            }
            let variant = definition
                .variants
                .iter()
                .find(|variant| enum_variant_name(variant) == member)
                .ok_or_else(|| {
                    RuntimeError::new(format!("enum `{root}` has no variant `{member}`"), span)
                })?;
            match variant {
                EnumVariant::Unit { .. } => Ok(Value::Enum(Rc::new(EnumInstance {
                    type_arguments: vec![Type::Unknown; definition.generic_parameters.len()],
                    type_definition: definition,
                    variant: member.into(),
                    payload: EnumPayload::Unit,
                }))),
                EnumVariant::Tuple { .. } | EnumVariant::Record { .. } => {
                    Ok(Value::VariantConstructor(Rc::new(VariantConstructor {
                        type_definition: definition,
                        variant: member.into(),
                        environment: owner_environment,
                    })))
                }
            }
        }
        Value::TraitType(definition) => {
            if !definition
                .methods
                .iter()
                .any(|method| method.name == member)
            {
                return Err(RuntimeError::new(
                    format!("trait `{}` has no method `{member}`", definition.name),
                    span,
                ));
            }
            Ok(Value::TraitMethodSelector(Rc::new(TraitMethodSelector {
                target: None,
                trait_name: definition.name.clone(),
                method_name: member.into(),
                environment,
            })))
        }
        _ => Err(RuntimeError::new(
            format!("`{root}` is not a struct or enum type"),
            span,
        )),
    }
}

pub(super) fn resolve_qualified_path(
    target: &Type,
    trait_name: &str,
    member: &str,
    environment: &EnvironmentRef,
    span: Span,
) -> Result<Value, RuntimeError> {
    let trait_value = environment
        .borrow()
        .get(trait_name)
        .ok_or_else(|| RuntimeError::new(format!("unknown trait `{trait_name}`"), span))?;
    let Value::TraitType(definition) = trait_value else {
        return Err(RuntimeError::new(
            format!("`{trait_name}` is not a trait"),
            span,
        ));
    };
    if !definition
        .methods
        .iter()
        .any(|method| method.name == member)
    {
        return Err(RuntimeError::new(
            format!("trait `{trait_name}` has no method `{member}`"),
            span,
        ));
    }
    Ok(Value::TraitMethodSelector(Rc::new(TraitMethodSelector {
        target: Some(target.clone()),
        trait_name: trait_name.into(),
        method_name: member.into(),
        environment: environment.clone(),
    })))
}
