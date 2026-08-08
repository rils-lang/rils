use super::*;

impl Interpreter {
    pub(super) fn construct_record(
        &self,
        path: &[String],
        values: HashMap<String, Value>,
        span: Span,
        environment: &EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        if values.values().any(Value::contains_reference) {
            return Err(RuntimeError::new(
                "references cannot be stored in struct or enum fields",
                span,
            ));
        }
        let direct = self.resolve_path(path, environment, span).ok();
        if let Some(Value::StructType(definition)) = direct {
            let name = definition.name.as_str();
            let mut substitutions = generic_substitutions(&definition.generic_parameters);
            infer_named_fields(&definition.fields, &values, &mut substitutions, span, name)?;
            validate_generic_bounds(
                &definition.generic_parameters,
                &substitutions,
                environment,
                span,
            )?;
            let values =
                validate_named_fields(&definition.fields, values, span, name, &substitutions)?;
            return Ok(Value::Struct(Rc::new(StructInstance {
                type_arguments: generic_arguments(&definition.generic_parameters, &substitutions),
                type_definition: definition,
                fields: RefCell::new(
                    values
                        .into_iter()
                        .map(|(name, value)| {
                            let type_annotation = Type::of_value(&value).unwrap_or(Type::Unknown);
                            (
                                name,
                                FieldSlot {
                                    value: Some(value),
                                    type_annotation,
                                    references: 0,
                                },
                            )
                        })
                        .collect(),
                ),
            })));
        }
        if path.len() >= 2 {
            let variant_name = path.last().expect("record path has variant");
            let enum_path = &path[..path.len() - 1];
            if let Ok(Value::EnumType(definition)) = self.resolve_path(enum_path, environment, span)
            {
                let enum_name = &definition.name;
                let variant = definition
                    .variants
                    .iter()
                    .find(|variant| enum_variant_name(variant) == variant_name)
                    .ok_or_else(|| {
                        RuntimeError::new(
                            format!("enum `{enum_name}` has no variant `{variant_name}`"),
                            span,
                        )
                    })?;
                let EnumVariant::Record { fields, .. } = variant else {
                    return Err(RuntimeError::new(
                        format!("`{enum_name}::{variant_name}` is not a record variant"),
                        span,
                    ));
                };
                let mut substitutions = generic_substitutions(&definition.generic_parameters);
                infer_named_fields(fields, &values, &mut substitutions, span, variant_name)?;
                validate_generic_bounds(
                    &definition.generic_parameters,
                    &substitutions,
                    environment,
                    span,
                )?;
                let values =
                    validate_named_fields(fields, values, span, variant_name, &substitutions)?;
                return Ok(Value::Enum(Rc::new(EnumInstance {
                    type_arguments: generic_arguments(
                        &definition.generic_parameters,
                        &substitutions,
                    ),
                    type_definition: definition,
                    variant: variant_name.clone(),
                    payload: EnumPayload::Record(values),
                })));
            }
        }
        Err(RuntimeError::new(
            format!("`{}` is not a record type or variant", path.join("::")),
            span,
        ))
    }

    pub(super) fn tick(&mut self, span: Span) -> Result<(), RuntimeError> {
        self.steps += 1;
        if self.steps > self.max_steps {
            Err(RuntimeError::new(
                format!("execution exceeded the {} step limit", self.max_steps),
                span,
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn condition_value(&self, value: &Value, span: Span) -> Result<bool, RuntimeError> {
        match value {
            Value::Option { .. } => Err(RuntimeError::new(
                "Option cannot be used as a condition; use `is_some` or `is_none`",
                span,
            )),
            Value::Unit => Err(RuntimeError::new(
                "`()` cannot be used as a condition",
                span,
            )),
            value => Ok(value.is_truthy()),
        }
    }
}
