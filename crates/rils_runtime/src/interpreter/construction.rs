use super::*;

pub(super) mod nominal;

impl Interpreter {
    pub(super) fn construct_contextual_empty(
        &self,
        expression: &Expr,
        expected: &Type,
        environment: &EnvironmentRef,
    ) -> Option<Result<Value, RuntimeError>> {
        let Type::Named {
            name,
            arguments: type_arguments,
        } = expected
        else {
            return None;
        };
        let Expr::Call {
            callee,
            arguments,
            span,
        } = expression
        else {
            return None;
        };
        if !arguments.is_empty() {
            return None;
        }
        let segments = match callee.as_ref() {
            Expr::Path { segments, .. } | Expr::GenericPath { segments, .. } => segments,
            _ => return None,
        };
        let [.., owner, member] = segments.as_slice() else {
            return None;
        };
        if owner != name
            || member != "new"
            || rils_frontend::standard_library::empty_native_constructor_symbol(owner).is_none()
        {
            return None;
        }
        if type_arguments.contains(&Type::Unknown) {
            if span.source.is_generated() {
                return None;
            }
            return Some(Err(RuntimeError::new(
                format!(
                    "cannot infer the type arguments of `{name}::new()`; add a type annotation or explicit type arguments"
                ),
                *span,
            )));
        }
        if !rils_frontend::standard_library::is_concrete_native_type(expected) {
            return None;
        }
        let context =
            crate::runtime_builtins::NativeOwnedContext::from_environment(&environment.borrow());
        Some(
            context
                .empty_collection(expected)
                .map_err(|message| RuntimeError::new(message, *span)),
        )
    }

    pub(super) fn construct_record(
        &self,
        path: &[String],
        values: HashMap<String, Value>,
        expected: Option<&Type>,
        span: Span,
        environment: &EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        let direct = self.resolve_path(path, environment, span).ok();
        if let Some(Value::StructType(definition)) = direct {
            let name = definition.name.as_str();
            if definition.opaque_native {
                return Err(RuntimeError::new(
                    format!("cannot construct opaque type `{name}` from fields"),
                    span,
                ));
            }
            let mut substitutions = generic_substitutions(&definition.generic_parameters);
            nominal::seed_arguments(
                expected,
                &definition.name,
                &definition.generic_parameters,
                &mut substitutions,
            );
            infer_named_fields(&definition.fields, &values, &mut substitutions, span, name)?;
            validate_generic_bounds(
                &definition.generic_parameters,
                &substitutions,
                None,
                environment,
                span,
            )?;
            let values = validate_named_fields(
                &definition.fields,
                values,
                span,
                name,
                &substitutions,
                environment,
            )?;
            let ty = Type::Named {
                name: definition.name.clone(),
                arguments: generic_arguments(&definition.generic_parameters, &substitutions),
            };
            let context = crate::runtime_builtins::NativeOwnedContext::from_environment(
                &environment.borrow(),
            );
            return context
                .storage()
                .construct_record(&ty, None, values)
                .map_err(|message| RuntimeError::new(message, span));
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
                nominal::seed_arguments(
                    expected,
                    &definition.name,
                    &definition.generic_parameters,
                    &mut substitutions,
                );
                infer_named_fields(fields, &values, &mut substitutions, span, variant_name)?;
                validate_generic_bounds(
                    &definition.generic_parameters,
                    &substitutions,
                    None,
                    environment,
                    span,
                )?;
                let values = validate_named_fields(
                    fields,
                    values,
                    span,
                    variant_name,
                    &substitutions,
                    environment,
                )?;
                let ty = Type::Named {
                    name: definition.name.clone(),
                    arguments: generic_arguments(&definition.generic_parameters, &substitutions),
                };
                let context = crate::runtime_builtins::NativeOwnedContext::from_environment(
                    &environment.borrow(),
                );
                return context
                    .storage()
                    .construct_record(&ty, Some(variant_name), values)
                    .map_err(|message| RuntimeError::new(message, span));
            }
        }
        Err(RuntimeError::new(
            format!("`{}` is not a record type or variant", path.join("::")),
            span,
        ))
    }

    pub(super) fn tick(&mut self, span: Span) -> Result<(), RuntimeError> {
        self.steps += 1;
        if self.steps > self.limits.max_steps {
            Err(RuntimeError::new(
                format!(
                    "execution exceeded the {} step limit",
                    self.limits.max_steps
                ),
                span,
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn condition_value(&self, value: &Value, span: Span) -> Result<bool, RuntimeError> {
        match Type::of_value(value) {
            Some(Type::Option(_)) => Err(RuntimeError::new(
                "Option cannot be used as a condition; use `is_some` or `is_none`",
                span,
            )),
            Some(Type::Unit) => Err(RuntimeError::new(
                "`()` cannot be used as a condition",
                span,
            )),
            _ => Ok(value.is_truthy()),
        }
    }
}
