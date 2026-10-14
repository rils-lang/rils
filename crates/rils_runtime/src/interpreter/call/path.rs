use super::*;

pub(super) fn resolve_associated_path(
    base: crate::value::Declaration,
    root: &str,
    member: &str,
    environment: EnvironmentRef,
    owner_environment: EnvironmentRef,
    span: Span,
) -> Result<Value, RuntimeError> {
    match base {
        crate::value::Declaration::BuiltinType(BuiltinType::Vec) => {
            native_collection_member("Vec", member, span)
        }
        crate::value::Declaration::BuiltinType(BuiltinType::HashMap) => {
            native_collection_member("HashMap", member, span)
        }
        crate::value::Declaration::BuiltinType(BuiltinType::HashSet) => {
            native_collection_member("HashSet", member, span)
        }
        crate::value::Declaration::BuiltinType(BuiltinType::Integer(target)) => {
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
            Ok(Value::from_callable(Rc::new(
                BuiltinFunction::IntegerIntrinsic {
                    symbol: intrinsic.symbol,
                    target,
                },
            )))
        }
        crate::value::Declaration::BuiltinType(BuiltinType::Float(target)) => {
            let constant = rils_builtins::float_constant(member).ok_or_else(|| {
                RuntimeError::new(
                    format!("{target} has no associated constant `{member}`"),
                    span,
                )
            })?;
            Ok(crate::numeric::float_constant(target, constant.id))
        }
        crate::value::Declaration::StructType(definition) => {
            if let Some(builtin) = rils_builtins::builtin_member(&definition.name, member)
                && builtin.kind == rils_builtins::BuiltinMemberKind::AssociatedFunction
                && let Some(symbol) = builtin.native_symbol
                && crate::runtime_builtins::requires_owned_native_call(symbol)
                && let Some(signature) =
                    rils_frontend::standard_library::erased_builtin_member_signature(builtin)
            {
                let arity = signature.parameters.as_ref().map_or(0, Vec::len);
                return Ok(Value::from_callable(Rc::new(NativeFunction {
                    binding_name: symbol,
                    name: symbol,
                    min_arity: arity,
                    max_arity: arity,
                    signature: Some(signature),
                    body: NativeFunctionBody::Symbol(symbol),
                })));
            }
            if let Some(builtin) = rils_builtins::builtin_member(&definition.name, member)
                && builtin.kind == rils_builtins::BuiltinMemberKind::AssociatedFunction
                && let Some(symbol) = builtin.native_symbol
                && let Some(signature) =
                    rils_frontend::standard_library::erased_builtin_member_signature(builtin)
            {
                let arity = signature.parameters.as_ref().map_or(0, Vec::len);
                return Ok(Value::from_callable(Rc::new(NativeFunction {
                    binding_name: symbol,
                    name: symbol,
                    min_arity: arity,
                    max_arity: arity,
                    signature: Some(signature),
                    body: NativeFunctionBody::Symbol(symbol),
                })));
            }
            let method = select_method(&definition.methods, &definition.trait_methods, member)
                .map_err(|traits| {
                    RuntimeError::new(
                        format!(
                            "method `{member}` is ambiguous for `{root}`: {}",
                            traits.join(", ")
                        ),
                        span,
                    )
                })?;
            method.map(Value::from_callable).ok_or_else(|| {
                RuntimeError::new(
                    format!("struct `{root}` has no associated function `{member}`"),
                    span,
                )
            })
        }
        crate::value::Declaration::EnumType(definition) => {
            if let Some(method) =
                select_method(&definition.methods, &definition.trait_methods, member).map_err(
                    |traits| {
                        RuntimeError::new(
                            format!(
                                "method `{member}` is ambiguous for `{root}`: {}",
                                traits.join(", ")
                            ),
                            span,
                        )
                    },
                )?
            {
                return Ok(Value::from_callable(method));
            }
            let variant = definition
                .variants
                .iter()
                .find(|variant| enum_variant_name(variant) == member)
                .ok_or_else(|| {
                    RuntimeError::new(format!("enum `{root}` has no variant `{member}`"), span)
                })?;
            match variant {
                EnumVariant::Unit { .. }
                | EnumVariant::Tuple { .. }
                | EnumVariant::Record { .. } => {
                    Ok(Value::from_callable(Rc::new(VariantConstructor {
                        type_definition: definition,
                        variant: member.into(),
                        environment: owner_environment,
                    })))
                }
            }
        }
        crate::value::Declaration::TraitType(definition) => {
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
            Ok(Value::from_callable(Rc::new(TraitMethodSelector {
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

fn native_collection_member(owner: &str, member: &str, span: Span) -> Result<Value, RuntimeError> {
    let declaration = rils_builtins::builtin_member(owner, member)
        .ok_or_else(|| RuntimeError::new(format!("{owner} has no `{member}` method"), span))?;
    let symbol = declaration.native_symbol.ok_or_else(|| {
        RuntimeError::new(format!("{owner}::{member} has no native symbol"), span)
    })?;
    let signature = rils_frontend::standard_library::erased_builtin_member_signature(declaration)
        .ok_or_else(|| {
        RuntimeError::new(format!("{owner}::{member} has no signature"), span)
    })?;
    let arity = signature.parameters.as_ref().map_or(0, Vec::len);
    Ok(Value::from_callable(Rc::new(NativeFunction {
        binding_name: symbol,
        name: symbol,
        min_arity: arity,
        max_arity: arity,
        signature: Some(signature),
        body: NativeFunctionBody::Symbol(symbol),
    })))
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
        .get_declaration(trait_name)
        .ok_or_else(|| RuntimeError::new(format!("unknown trait `{trait_name}`"), span))?;
    let crate::value::Declaration::TraitType(definition) = trait_value else {
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
    Ok(Value::from_callable(Rc::new(TraitMethodSelector {
        target: Some(expand_type_aliases(target, environment, span)?),
        trait_name: definition.name.clone(),
        method_name: member.into(),
        environment: environment.clone(),
    })))
}
