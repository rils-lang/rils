use super::*;

pub(super) fn builtin_default_value(ty: &Type) -> Option<Value> {
    use rils_frontend::default::DefaultPlan;

    fn materialize(plan: &DefaultPlan) -> Option<Value> {
        let sequence = |values: Vec<(Value, Type)>| {
            Rc::new(IndexedStorage {
                active_iterators: std::cell::Cell::new(0),
                elements: RefCell::new(
                    values
                        .into_iter()
                        .map(|(value, type_annotation)| FieldSlot {
                            value: Some(value),
                            type_annotation,
                            references: 0,
                        })
                        .collect(),
                ),
                element_type: RefCell::new(None),
            })
        };
        Some(match plan {
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
            DefaultPlan::Tuple(elements) => Value::Tuple(sequence(
                elements
                    .iter()
                    .map(|element| {
                        let value = materialize(element)?;
                        let ty = Type::of_value(&value)?;
                        Some((value, ty))
                    })
                    .collect::<Option<Vec<_>>>()?,
            )),
            DefaultPlan::Array {
                element,
                element_type,
                length,
            } => {
                let values = (0..*length)
                    .map(|_| Some((materialize(element)?, element_type.clone())))
                    .collect::<Option<Vec<_>>>()?;
                let sequence = sequence(values);
                *sequence.element_type.borrow_mut() = Some(element_type.clone());
                Value::Array(sequence)
            }
            DefaultPlan::Option(inner) => Value::Option {
                value: None,
                element_type: Some(inner.clone()),
            },
            DefaultPlan::EmptyCollection { name, arguments } if name == "Vec" => {
                Value::Vec(Rc::new(IndexedStorage {
                    active_iterators: std::cell::Cell::new(0),
                    elements: RefCell::new(Vec::new()),
                    element_type: RefCell::new(Some(arguments[0].clone())),
                }))
            }
            DefaultPlan::EmptyCollection { name, arguments } if name == "HashMap" => {
                Value::HashMap(Rc::new(HashMapValue {
                    borrowed: std::cell::Cell::new(0),
                    entries: RefCell::new(std::collections::HashMap::new()),
                    key_type: RefCell::new(arguments[0].clone()),
                    value_type: RefCell::new(arguments[1].clone()),
                }))
            }
            DefaultPlan::EmptyCollection { name, arguments } if name == "HashSet" => {
                Value::HashSet(Rc::new(HashSetValue {
                    borrowed: std::cell::Cell::new(0),
                    entries: RefCell::new(std::collections::HashSet::new()),
                    element_type: RefCell::new(arguments[0].clone()),
                }))
            }
            DefaultPlan::EmptyCollection { .. } | DefaultPlan::TraitCall(_) => return None,
        })
    }
    materialize(&rils_frontend::default::default_plan(ty)?)
}

pub(crate) fn builtin_runtime_member(
    value: &Value,
    name: &str,
) -> Option<(BuiltinMethod, rils_builtins::ReceiverMode)> {
    let owner = match value {
        Value::Array(_) => "Vec",
        Value::Vec(_) => "Vec",
        Value::HashMap(_) => "HashMap",
        Value::BTreeMap(_) => "BTreeMap",
        Value::BTreeSet(_) => "BTreeSet",
        Value::HashSet(_) => "HashSet",
        Value::Rc(_) => "Rc",
        Value::Weak(_) => "Weak",
        Value::Cell(_) => "Cell",
        Value::RefCell(_) => "RefCell",
        Value::VecDeque(_) => "VecDeque",
        Value::BinaryHeap(_) => "BinaryHeap",
        Value::Native(object) => match object.descriptor().rils_type() {
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
    if matches!(value, Value::Array(_)) && !member.indexed_view {
        return None;
    }
    let method = if let Some(symbol) = member.native_symbol {
        BuiltinMethod::Native(symbol)
    } else {
        BuiltinMethod::Runtime(member.builtin_id?)
    };
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
    apply_type(
        Some(&signature.return_type),
        &value,
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
