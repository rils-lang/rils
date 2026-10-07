use super::*;

pub(super) fn builtin_default_value(
    ty: &Type,
    context: &crate::runtime_builtins::NativeOwnedContext,
) -> Result<Option<Value>, String> {
    use rils_frontend::default::DefaultPlan;

    fn materialize(
        plan: &DefaultPlan,
        context: &crate::runtime_builtins::NativeOwnedContext,
    ) -> Result<Option<Value>, String> {
        let sequence = |values: Vec<(Value, Type)>| {
            Rc::new(IndexedStorage {
                active_iterators: std::cell::Cell::new(0),
                elements: RefCell::new(
                    values
                        .into_iter()
                        .map(|(value, type_annotation)| FieldSlot::new(type_annotation, value))
                        .collect(),
                ),
                element_type: RefCell::new(None),
            })
        };
        Ok(Some(match plan {
            DefaultPlan::Unit => Value::Unit,
            DefaultPlan::Bool => Value::Bool(false),
            DefaultPlan::Integer(crate::IntegerType::I8) => crate::numeric::native_i8(0),
            DefaultPlan::Integer(crate::IntegerType::I16) => crate::numeric::native_i16(0),
            DefaultPlan::Integer(crate::IntegerType::I32) => crate::numeric::native_i32(0),
            DefaultPlan::Integer(crate::IntegerType::I64) => crate::numeric::native_i64(0),
            DefaultPlan::Integer(crate::IntegerType::I128) => crate::numeric::native_i128(0),
            DefaultPlan::Integer(crate::IntegerType::Isize) => crate::numeric::native_isize(0),
            DefaultPlan::Integer(crate::IntegerType::U8) => crate::numeric::native_u8(0),
            DefaultPlan::Integer(crate::IntegerType::U16) => crate::numeric::native_u16(0),
            DefaultPlan::Integer(crate::IntegerType::U32) => crate::numeric::native_u32(0),
            DefaultPlan::Integer(crate::IntegerType::U64) => crate::numeric::native_u64(0),
            DefaultPlan::Integer(crate::IntegerType::U128) => crate::numeric::native_u128(0),
            DefaultPlan::Integer(crate::IntegerType::Usize) => crate::numeric::native_usize(0),
            DefaultPlan::Float(crate::FloatType::F32) => Value::from_f32(0.0),
            DefaultPlan::Float(crate::FloatType::F64) => Value::from_f64(0.0),
            DefaultPlan::Char => rils_execution::value::native_char('\0'),
            DefaultPlan::String => rils_execution::value::native_string(""),
            DefaultPlan::Tuple(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    let Some(value) = materialize(element, context)? else {
                        return Ok(None);
                    };
                    let ty = Type::of_value(&value).ok_or("default value has no concrete type")?;
                    values.push((value, ty));
                }
                Value::Tuple(sequence(values))
            }
            DefaultPlan::Array {
                element,
                element_type,
                length,
            } => {
                let mut values = Vec::with_capacity(*length);
                for _ in 0..*length {
                    let Some(value) = materialize(element, context)? else {
                        return Ok(None);
                    };
                    values.push((value, element_type.clone()));
                }
                let sequence = sequence(values);
                *sequence.element_type.borrow_mut() = Some(element_type.clone());
                Value::Array(sequence)
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
    materialize(&plan, context)
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
    let reference_layout = match value {
        Value::Reference(reference) => reference.native_layout().ok().flatten(),
        _ => None,
    };
    let owner = match value {
        Value::Array(_) => "Vec",
        Value::Vec(_) => "Vec",
        Value::HashMap(_) => "HashMap",
        Value::BTreeMap(_) => "BTreeMap",
        Value::BTreeSet(_) => "BTreeSet",
        Value::HashSet(_) => "HashSet",
        Value::VecDeque(_) => "VecDeque",
        Value::BinaryHeap(_) => "BinaryHeap",
        Value::Native(object) => match object.descriptor().rils_type() {
            Type::String => "string",
            Type::Named { name, .. } => name.as_str(),
            _ => return None,
        },
        Value::Reference(_) => match reference_layout.as_ref()?.rils_type() {
            Type::Option(_) => "Option",
            Type::Result(_, _) => "Result",
            Type::String => "string",
            Type::Named { name, .. } => name.as_str(),
            _ => return None,
        },
        Value::Option { .. } => "Option",
        Value::Dynamic(object)
            if matches!(object.descriptor().layout().rils_type(), Type::Option(_)) =>
        {
            "Option"
        }
        Value::Dynamic(object)
            if matches!(object.descriptor().layout().rils_type(), Type::Result(_, _)) =>
        {
            "Result"
        }
        Value::Dynamic(object) => match object.descriptor().layout().rils_type() {
            Type::Named { name, .. } => name.as_str(),
            _ => return None,
        },
        Value::Result { .. } => "Result",
        Value::OwnedIterator(_) => "Iterator",
        Value::BorrowedIndexedIterator(_) => "Iter",
        Value::BorrowedMapIterator(_) | Value::BorrowedSetIterator(_) => "Iter",
        Value::HostObject(object) if object.type_definition.name == "Formatter" => "Formatter",
        _ => return None,
    };
    let member = rils_builtins::builtin_member(owner, name).or_else(|| {
        (owner == "Iter" && rils_builtins::is_iterator_default_method(name))
            .then(|| rils_builtins::builtin_member("Iterator", name))
            .flatten()
    })?;
    if matches!(value, Value::Array(_)) && !rils_builtins::sequence_view_member(member, true) {
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
