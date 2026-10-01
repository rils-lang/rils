use super::*;

impl VirtualMachine<'_> {
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
                let (structs, enums) = self.type_definitions();
                let storage =
                    rils_execution::value::storage::TypedStorageContext::new(&structs, &enums);
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
                        Ok((
                            field.name.clone(),
                            FieldSlot {
                                value: Some(value),
                                type_annotation: annotation,
                                references: 0,
                            },
                        ))
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
                let (structs, enums) = self.type_definitions();
                let storage =
                    rils_execution::value::storage::TypedStorageContext::new(&structs, &enums);
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
