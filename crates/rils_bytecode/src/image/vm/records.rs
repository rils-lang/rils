use super::*;

impl VirtualMachine<'_> {
    pub(super) fn construct_tuple_variant(
        &mut self,
        type_id: usize,
        variant: String,
        fields: Vec<usize>,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let values = self.take_registers(fields, span)?;
        let RuntimeType::Enum(definition) = &self.module.types[type_id] else {
            return Err(BytecodeError::new("tuple variant requires enum type", span));
        };
        let field_types = definition
            .variants
            .iter()
            .find_map(|entry| match entry {
                crate::ast::EnumVariant::Tuple { name, fields, .. } if name == &variant => {
                    Some(fields)
                }
                _ => None,
            })
            .ok_or_else(|| {
                BytecodeError::new(format!("enum variant `{variant}` is not a tuple"), span)
            })?;
        if field_types.len() != values.len() {
            return Err(BytecodeError::new(
                format!(
                    "enum variant `{variant}` expects {} fields",
                    field_types.len()
                ),
                span,
            ));
        }
        let mut inferred = HashMap::new();
        for (field_type, value) in field_types.iter().zip(&values) {
            infer_type_arguments(field_type, value, &mut inferred);
        }
        let type_arguments = definition
            .generic_parameters
            .iter()
            .map(|parameter| inferred.remove(&parameter.name).unwrap_or(Type::Unknown))
            .collect::<Vec<_>>();
        let substitutions = definition
            .generic_parameters
            .iter()
            .zip(&type_arguments)
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect::<HashMap<_, _>>();
        let storage = self.native_context.storage();
        let values = field_types
            .iter()
            .zip(values)
            .map(|(field_type, value)| {
                storage
                    .apply_declared(value, &field_type.substitute(&substitutions))
                    .map_err(|message| BytecodeError::new(message, span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Enum(Rc::new(EnumInstance {
            type_definition: definition.clone(),
            variant,
            payload: EnumPayload::Tuple(values),
            type_arguments,
        })))
    }

    pub(super) fn construct_record(
        &mut self,
        type_id: usize,
        variant: Option<String>,
        fields: Vec<(String, usize)>,
        span: Span,
    ) -> Result<Value, BytecodeError> {
        let mut values = fields
            .into_iter()
            .map(|(name, register)| Ok((name, self.take_register(register, span)?)))
            .collect::<Result<HashMap<_, _>, BytecodeError>>()?;
        let value = match (&self.module.types[type_id], variant) {
            (RuntimeType::Struct(definition), None) => {
                if definition.opaque_native {
                    return Err(BytecodeError::new(
                        format!(
                            "cannot construct opaque type `{}` from fields",
                            definition.name
                        ),
                        span,
                    ));
                }
                let type_arguments = infer_generic_arguments(
                    &definition.generic_parameters,
                    &definition.fields,
                    &values,
                );
                let substitutions = definition
                    .generic_parameters
                    .iter()
                    .zip(&type_arguments)
                    .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
                    .collect::<HashMap<_, _>>();
                let storage = self.native_context.storage();
                let slots = definition
                    .fields
                    .iter()
                    .map(|field| {
                        let value = values.remove(&field.name).ok_or_else(|| {
                            BytecodeError::new(
                                format!("record constructor is missing field `{}`", field.name),
                                span,
                            )
                        })?;
                        let annotation = field.type_annotation.substitute(&substitutions);
                        let annotation = if matches!(annotation, Type::Variable(_)) {
                            Type::of_value(&value).unwrap_or(Type::Unknown)
                        } else {
                            annotation
                        };
                        let value = storage
                            .apply_declared(value, &annotation)
                            .map_err(|message| BytecodeError::new(message, span))?;
                        Ok((field.name.clone(), FieldSlot::new(annotation, value)))
                    })
                    .collect::<Result<HashMap<_, _>, BytecodeError>>()?;
                let fields = StructFields::from_map(definition.clone(), slots)
                    .map_err(|message| BytecodeError::new(message, span))?;
                Value::Struct(Rc::new(StructInstance {
                    type_definition: definition.clone(),
                    fields: RefCell::new(fields),
                    type_arguments,
                }))
            }
            (RuntimeType::Enum(definition), Some(variant)) => {
                let fields = definition
                    .variants
                    .iter()
                    .find_map(|entry| match entry {
                        crate::ast::EnumVariant::Record { name, fields, .. }
                            if name == &variant =>
                        {
                            Some(fields)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| {
                        BytecodeError::new(
                            format!("enum variant `{variant}` is not a record"),
                            span,
                        )
                    })?;
                let type_arguments =
                    infer_generic_arguments(&definition.generic_parameters, fields, &values);
                let substitutions = definition
                    .generic_parameters
                    .iter()
                    .zip(&type_arguments)
                    .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
                    .collect::<HashMap<_, _>>();
                let storage = self.native_context.storage();
                for field in fields {
                    let annotation = field.type_annotation.substitute(&substitutions);
                    if let Some(value) = values.remove(&field.name) {
                        values.insert(
                            field.name.clone(),
                            storage
                                .apply_declared(value, &annotation)
                                .map_err(|message| BytecodeError::new(message, span))?,
                        );
                    }
                }
                Value::Enum(Rc::new(EnumInstance {
                    type_definition: definition.clone(),
                    variant,
                    payload: EnumPayload::Record(values),
                    type_arguments,
                }))
            }
            _ => {
                return Err(BytecodeError::new(
                    "record constructor does not match its type",
                    span,
                ));
            }
        };
        Ok(value)
    }
}

fn infer_generic_arguments(
    parameters: &[rils_frontend::ast::GenericParameter],
    fields: &[rils_frontend::ast::NamedField],
    values: &HashMap<String, Value>,
) -> Vec<Type> {
    let mut inferred = HashMap::new();
    for field in fields {
        if let Some(value) = values.get(&field.name) {
            infer_type_arguments(&field.type_annotation, value, &mut inferred);
        }
    }
    parameters
        .iter()
        .map(|parameter| inferred.remove(&parameter.name).unwrap_or(Type::Unknown))
        .collect()
}

fn infer_type_arguments(expected: &Type, value: &Value, inferred: &mut HashMap<String, Type>) {
    match expected {
        Type::Variable(name) => {
            if let Some(actual) = Type::of_value(value) {
                inferred.entry(name.clone()).or_insert(actual);
            }
        }
        Type::Option(inner) => {
            if let Value::Option {
                value: Some(value), ..
            } = value
            {
                infer_type_arguments(inner, value, inferred);
            } else if let Some(Type::Option(actual)) = Type::of_value(value) {
                infer_type_from_types(inner, &actual, inferred);
            }
        }
        Type::Result(ok, error) => {
            if let Value::Result { value, .. } = value {
                match value {
                    Ok(value) => infer_type_arguments(ok, value, inferred),
                    Err(value) => infer_type_arguments(error, value, inferred),
                }
            }
            if let Some(Type::Result(actual_ok, actual_error)) = Type::of_value(value) {
                infer_type_from_types(ok, &actual_ok, inferred);
                infer_type_from_types(error, &actual_error, inferred);
            }
        }
        Type::Array { element, .. } => {
            if let Value::Array(sequence) = value {
                for slot in sequence.elements.borrow().iter() {
                    if let Some(value) = &slot.value {
                        infer_type_arguments(element, value, inferred);
                    }
                }
            } else if let Some(actual) = Type::of_value(value) {
                infer_type_from_types(expected, &actual, inferred);
            }
        }
        Type::Tuple(_) => {
            if let Some(actual) = Type::of_value(value) {
                infer_type_from_types(expected, &actual, inferred);
            }
        }
        Type::Named { arguments, .. } => {
            if let Some(Type::Named {
                arguments: actual, ..
            }) = Type::of_value(value)
            {
                for (expected, actual) in arguments.iter().zip(actual.iter()) {
                    infer_type_from_types(expected, actual, inferred);
                }
            }
        }
        _ => {}
    }
}

fn infer_type_from_types(expected: &Type, actual: &Type, inferred: &mut HashMap<String, Type>) {
    match expected {
        Type::Variable(name) => {
            if actual != &Type::Unknown {
                inferred
                    .entry(name.clone())
                    .or_insert_with(|| actual.clone());
            }
        }
        Type::Option(inner) => {
            if let Type::Option(actual) = actual {
                infer_type_from_types(inner, actual, inferred);
            }
        }
        Type::Result(ok, error) => {
            if let Type::Result(actual_ok, actual_error) = actual {
                infer_type_from_types(ok, actual_ok, inferred);
                infer_type_from_types(error, actual_error, inferred);
            }
        }
        Type::Tuple(elements) => {
            if let Type::Tuple(actual) = actual {
                for (expected, actual) in elements.iter().zip(actual) {
                    infer_type_from_types(expected, actual, inferred);
                }
            }
        }
        Type::Array { element, .. } => {
            if let Type::Array {
                element: actual, ..
            } = actual
            {
                infer_type_from_types(element, actual, inferred);
            }
        }
        Type::Named { arguments, .. } => {
            if let Type::Named {
                arguments: actual, ..
            } = actual
            {
                for (expected, actual) in arguments.iter().zip(actual.iter()) {
                    infer_type_from_types(expected, actual, inferred);
                }
            }
        }
        _ => {}
    }
}
