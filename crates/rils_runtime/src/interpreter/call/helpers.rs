use super::*;

pub(super) fn builtin_default_value(
    ty: &Type,
    context: &crate::runtime_builtins::NativeOwnedContext,
) -> Result<Option<Value>, String> {
    use rils_frontend::default::DefaultPlan;

    fn materialize(
        plan: &DefaultPlan,
        ty: &Type,
        context: &crate::runtime_builtins::NativeOwnedContext,
    ) -> Result<Option<Value>, String> {
        Ok(Some(match plan {
            DefaultPlan::Unit => Value::from(rils_execution::value::native_unit()),
            DefaultPlan::Bool => Value::from(rils_execution::value::native_bool(false)),
            DefaultPlan::Integer(crate::IntegerType::I8) => {
                Value::from(crate::numeric::native_i8(0))
            }
            DefaultPlan::Integer(crate::IntegerType::I16) => {
                Value::from(crate::numeric::native_i16(0))
            }
            DefaultPlan::Integer(crate::IntegerType::I32) => {
                Value::from(crate::numeric::native_i32(0))
            }
            DefaultPlan::Integer(crate::IntegerType::I64) => {
                Value::from(crate::numeric::native_i64(0))
            }
            DefaultPlan::Integer(crate::IntegerType::I128) => {
                Value::from(crate::numeric::native_i128(0))
            }
            DefaultPlan::Integer(crate::IntegerType::Isize) => {
                Value::from(crate::numeric::native_isize(0))
            }
            DefaultPlan::Integer(crate::IntegerType::U8) => {
                Value::from(crate::numeric::native_u8(0))
            }
            DefaultPlan::Integer(crate::IntegerType::U16) => {
                Value::from(crate::numeric::native_u16(0))
            }
            DefaultPlan::Integer(crate::IntegerType::U32) => {
                Value::from(crate::numeric::native_u32(0))
            }
            DefaultPlan::Integer(crate::IntegerType::U64) => {
                Value::from(crate::numeric::native_u64(0))
            }
            DefaultPlan::Integer(crate::IntegerType::U128) => {
                Value::from(crate::numeric::native_u128(0))
            }
            DefaultPlan::Integer(crate::IntegerType::Usize) => {
                Value::from(crate::numeric::native_usize(0))
            }
            DefaultPlan::Float(crate::FloatType::F32) => {
                Value::from(rils_execution::value::native_f32(0.0))
            }
            DefaultPlan::Float(crate::FloatType::F64) => {
                Value::from(rils_execution::value::native_f64(0.0))
            }
            DefaultPlan::Char => Value::from(rils_execution::value::native_char('\0')),
            DefaultPlan::String => Value::from(rils_execution::value::native_string("")),
            DefaultPlan::Tuple(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                let Type::Tuple(types) = ty else {
                    return Err("invalid tuple default type".into());
                };
                for (element, field_type) in elements.iter().zip(types) {
                    let Some(value) = materialize(element, field_type, context)? else {
                        return Ok(None);
                    };
                    values.push(value);
                }
                context.storage().construct_tuple(ty, values)?
            }
            DefaultPlan::Array {
                element,
                element_type,
                length,
            } => {
                let mut values = Vec::with_capacity(*length);
                for _ in 0..*length {
                    let Some(value) = materialize(element, element_type, context)? else {
                        return Ok(None);
                    };
                    values.push(value);
                }
                context.storage().construct_array(ty, values)?
            }
            DefaultPlan::Option(inner) => context.none(inner)?,
            DefaultPlan::EmptyCollection { name, arguments } => {
                let ty = Type::Named {
                    name: name.clone(),
                    arguments: arguments.clone(),
                };
                context.empty_collection(&ty)?
            }
            DefaultPlan::TraitCall(_) => return Ok(None),
        }))
    }
    let Some(plan) = rils_frontend::default::default_plan(ty) else {
        return Ok(None);
    };
    materialize(&plan, ty, context)
}

pub(crate) fn builtin_runtime_member(
    value: &Value,
    name: &str,
) -> Option<(BuiltinMethod, rils_builtins::ReceiverMode)> {
    if rils_execution::value::native_instance::record_definition(value)
        .ok()
        .flatten()
        .is_some()
    {
        return None;
    }
    let ty = receiver_type(value)?;
    if let Some(host) = value.host_type_definition().ok()? {
        if host.name != "Formatter" {
            return None;
        }
    }
    let owner = if crate::value::native_ops::is_iterator(value) {
        "Iterator"
    } else {
        match &ty {
            Type::Array { .. } => "Vec",
            Type::Option(_) => "Option",
            Type::Result(_, _) => "Result",
            Type::String => "string",
            Type::Named { name, .. } => name.as_str(),
            _ => return None,
        }
    };
    let member = rils_builtins::builtin_member(owner, name).or_else(|| {
        (owner == "Iter" && rils_builtins::is_iterator_default_method(name))
            .then(|| rils_builtins::builtin_member("Iterator", name))
            .flatten()
    })?;
    let array = matches!(ty, Type::Array { .. });
    if array && !rils_builtins::sequence_view_member(member, value.as_reference().is_none()) {
        return None;
    }
    let method = BuiltinMethod::Native(member.native_symbol?);
    Some((method, member.receiver?))
}

pub(super) fn validate_native_arguments(
    signature: Option<&FunctionSignature>,
    arguments: &[Value],
    span: Span,
) -> Result<(), RuntimeError> {
    let Some(parameters) = signature.and_then(|signature| signature.parameters.as_ref()) else {
        return Ok(());
    };
    for (index, (expected, argument)) in parameters.iter().zip(arguments).enumerate() {
        apply_type(
            Some(expected),
            argument,
            span,
            &format!("native argument {}", index + 1),
        )?;
    }
    Ok(())
}

pub(super) fn validate_native_return(
    signature: Option<&FunctionSignature>,
    value: Value,
    span: Span,
    name: &str,
) -> Result<Value, RuntimeError> {
    let Some(signature) = signature else {
        return Ok(value);
    };
    let actual = Type::of_value(&value).ok_or_else(|| {
        RuntimeError::new(
            format!("return value of `{name}` has no runtime type"),
            span,
        )
    })?;
    let expected = merge_types(&signature.return_type, &actual)
        .or_else(|| {
            signature
                .return_type
                .accepts(&value)
                .then(|| signature.return_type.clone())
        })
        .ok_or_else(|| {
            RuntimeError::new(
                format!(
                    "type mismatch for return value of `{name}`: expected {}, found {actual}",
                    signature.return_type
                ),
                span,
            )
        })?;
    apply_type_owned(
        Some(&expected),
        value,
        span,
        &format!("return value of `{name}`"),
    )
}

pub(crate) fn select_method(
    methods: &std::cell::RefCell<HashMap<String, Rc<UserFunction>>>,
    trait_methods: &std::cell::RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
    name: &str,
) -> Result<Option<Rc<UserFunction>>, Vec<String>> {
    if let Some(method) = methods.borrow().get(name).cloned() {
        return Ok(Some(method));
    }
    let mut candidates = trait_methods
        .borrow()
        .iter()
        .filter_map(|(trait_name, methods)| {
            methods
                .get(name)
                .cloned()
                .map(|method| (trait_name.clone(), method))
        })
        .collect::<Vec<_>>();
    match candidates.len() {
        0 => Ok(None),
        1 => Ok(Some(candidates.pop().expect("one candidate").1)),
        _ => {
            let mut traits = candidates
                .into_iter()
                .map(|(trait_name, _)| trait_name)
                .collect::<Vec<_>>();
            traits.sort();
            Err(traits)
        }
    }
}

pub(super) use rils_execution::runtime_type::receiver_type;
