use super::*;

pub(super) fn builtin_default_value(ty: &Type) -> Option<Value> {
    use rils_frontend::default::DefaultPlan;

    fn materialize(plan: &DefaultPlan) -> Option<Value> {
        let sequence = |values: Vec<(Value, Type)>| {
            Rc::new(SequenceValue {
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
            DefaultPlan::Integer(crate::IntegerType::I8) => Value::I8(0),
            DefaultPlan::Integer(crate::IntegerType::I16) => Value::I16(0),
            DefaultPlan::Integer(crate::IntegerType::I32) => Value::I32(0),
            DefaultPlan::Integer(crate::IntegerType::I64) => Value::I64(0),
            DefaultPlan::Integer(crate::IntegerType::I128) => Value::I128(0),
            DefaultPlan::Integer(crate::IntegerType::Isize) => Value::Isize(0),
            DefaultPlan::Integer(crate::IntegerType::U8) => Value::U8(0),
            DefaultPlan::Integer(crate::IntegerType::U16) => Value::U16(0),
            DefaultPlan::Integer(crate::IntegerType::U32) => Value::U32(0),
            DefaultPlan::Integer(crate::IntegerType::U64) => Value::U64(0),
            DefaultPlan::Integer(crate::IntegerType::U128) => Value::U128(0),
            DefaultPlan::Integer(crate::IntegerType::Usize) => Value::Usize(0),
            DefaultPlan::Float(crate::FloatType::F32) => Value::F32(0.0),
            DefaultPlan::Float(crate::FloatType::F64) => Value::F64(0.0),
            DefaultPlan::Char => Value::Char('\0'),
            DefaultPlan::String => Value::String(Rc::from("")),
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
                Value::Vec(Rc::new(SequenceValue {
                    elements: RefCell::new(Vec::new()),
                    element_type: RefCell::new(Some(arguments[0].clone())),
                }))
            }
            DefaultPlan::EmptyCollection { name, arguments } if name == "HashMap" => {
                Value::HashMap(Rc::new(HashMapValue {
                    entries: RefCell::new(std::collections::HashMap::new()),
                    key_type: RefCell::new(arguments[0].clone()),
                    value_type: RefCell::new(arguments[1].clone()),
                }))
            }
            DefaultPlan::EmptyCollection { name, arguments } if name == "HashSet" => {
                Value::HashSet(Rc::new(HashSetValue {
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
) -> Option<(rils_builtins::BuiltinId, rils_builtins::ReceiverMode)> {
    let owner = match value {
        Value::Array(_) => "Array",
        Value::String(_) => "string",
        Value::Vec(_) => "Vec",
        Value::HashMap(_) => "HashMap",
        Value::HashSet(_) => "HashSet",
        Value::Range(_) => "Range",
        Value::Option { .. } => "Option",
        Value::Result { .. } => "Result",
        Value::SequenceIterator(_) => "Iterator",
        Value::HostObject(object) if object.type_definition.name == "Formatter" => "Formatter",
        _ => return None,
    };
    let member = rils_builtins::builtin_member(owner, name)?;
    Some((member.builtin_id?, member.receiver?))
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
