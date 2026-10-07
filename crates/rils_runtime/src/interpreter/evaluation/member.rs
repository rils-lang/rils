use super::super::*;

impl Interpreter {
    pub(super) fn evaluate_member(
        &mut self,
        object: &Expr,
        name: &str,
        span: Span,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        if let Expr::Variable {
            name: variable_name,
            ..
        } = object
            && let Some(value) = environment.borrow().get(variable_name)
        {
            if matches!(value, Value::Reference(_)) {
                // A reference binding already carries receiver mutability and
                // provenance. Reusing it also gives fields priority over methods.
                return self.resolve_member(value, name, span);
            }
            if matches!(&value, Value::Dynamic(object) if object.descriptor().layout().record_field_index(name).is_some())
            {
                return self.resolve_member(value, name, span);
            }
            if matches!(&value, Value::Tuple(_)) && name.parse::<usize>().is_ok() {
                return self.resolve_member(value, name, span);
            }
            if let Value::HostObject(instance) = &value
                && instance.type_definition.methods.borrow().contains_key(name)
            {
                return self.resolve_member(value, name, span);
            }
            let builtin_borrow = super::super::call::builtin_runtime_member(&value, name)
                .map(|(_, receiver)| receiver)
                .or_else(|| super::super::call::builtin_iterator_default_receiver(&value, name))
                .and_then(|receiver| match receiver {
                    rils_builtins::ReceiverMode::Shared => Some(false),
                    rils_builtins::ReceiverMode::Mutable => Some(true),
                    rils_builtins::ReceiverMode::Owned => None,
                });
            if let Some(mutable) = builtin_borrow {
                let receiver =
                    self.reference_variable(variable_name, mutable, &environment, span)?;
                return self.resolve_member(receiver, name, span);
            }
            let method = selected_method(&value, name, span)?;
            if let Some(mutable) = method.as_ref().and_then(|method| {
                match method.parameters.first()?.type_annotation.as_ref()? {
                    Type::Reference { mutable, .. } => Some(*mutable),
                    _ => None,
                }
            }) {
                let receiver =
                    self.reference_variable(variable_name, mutable, &environment, span)?;
                return self.resolve_member(receiver, name, span);
            }
            if method.is_none()
                && name == "clone"
                && Type::of_value(&value)
                    .is_some_and(|ty| type_implements_trait(&ty, "Clone", &environment))
            {
                let receiver = self.reference_variable(variable_name, false, &environment, span)?;
                return self.resolve_member(receiver, name, span);
            }
        }
        if matches!(
            object,
            Expr::Member { .. }
                | Expr::Index { .. }
                | Expr::Unary {
                    operator: UnaryOp::Dereference,
                    ..
                }
        ) {
            let place = self.resolve_place(object, &environment, span)?;
            if let Some(owner) = place.native_owner(span)? {
                let layout = owner
                    .with_view(|view| view.layout())
                    .map_err(|message| RuntimeError::new(message, span))?
                    .map_err(|message| RuntimeError::new(message, span))?;
                let builtin = match layout.rils_type() {
                    Type::Option(_) => Some("Option"),
                    Type::Result(_, _) => Some("Result"),
                    Type::Named { name, .. } if layout.record_fields().is_none() => {
                        Some(name.as_str())
                    }
                    _ => None,
                }
                .and_then(|owner| rils_builtins::builtin_member(owner, name));
                if let Some(receiver) = builtin.and_then(|member| member.receiver) {
                    let receiver = match receiver {
                        rils_builtins::ReceiverMode::Owned => place.read(span)?,
                        rils_builtins::ReceiverMode::Shared => place.borrow(false, span)?,
                        rils_builtins::ReceiverMode::Mutable => place.borrow(true, span)?,
                    };
                    return self.resolve_member(receiver, name, span);
                }
            }
            if let Some(owner) = place.native_owner(span)?
                && owner
                    .with_view(|view| view.layout())
                    .map_err(|message| RuntimeError::new(message, span))?
                    .map_err(|message| RuntimeError::new(message, span))?
                    .record_field_index(name)
                    .is_some()
            {
                return owner
                    .field(name)
                    .and_then(|place| place.take())
                    .map_err(|message| RuntimeError::new(message, span));
            }
            if let Some(reference) = place.projection_guard(span)? {
                if let Some(projected) = reference
                    .project_native_field(name)
                    .map_err(|message| RuntimeError::new(message, span))?
                {
                    return projected
                        .copy_native()
                        .map_err(|message| RuntimeError::new(message, span))?
                        .ok_or_else(|| {
                            RuntimeError::new("projected field has no native storage", span)
                        });
                }
                let method = selected_method(&Value::Reference(reference), name, span)?;
                if let Some(method) = method {
                    let receiver = match method
                        .parameters
                        .first()
                        .and_then(|parameter| parameter.type_annotation.as_ref())
                    {
                        Some(Type::Reference { mutable, .. }) => place.borrow(*mutable, span)?,
                        _ => place.read(span)?,
                    };
                    return self.resolve_member(receiver, name, span);
                }
            }
            let value = place.projection_value(span)?;
            if matches!(&value, Value::Tuple(_) if name.parse::<usize>().is_ok()) {
                // The owner is only a projection. Transfer ownership at the final
                // field instead of moving every intermediate non-Copy record.
                return self.resolve_member(value, name, span);
            }
            let builtin_borrow = super::super::call::builtin_iterator_default_receiver(
                &value, name,
            )
            .and_then(|receiver| match receiver {
                rils_builtins::ReceiverMode::Shared => Some(false),
                rils_builtins::ReceiverMode::Mutable => Some(true),
                rils_builtins::ReceiverMode::Owned => None,
            });
            let method_borrow = selected_method(&value, name, span)?
                .as_ref()
                .and_then(
                    |method| match method.parameters.first()?.type_annotation.as_ref()? {
                        Type::Reference { mutable, .. } => Some(*mutable),
                        _ => None,
                    },
                );
            if let Some(mutable) = builtin_borrow.or(method_borrow) {
                let receiver = place.borrow(mutable, span)?;
                return self.resolve_member(receiver, name, span);
            }
        }
        let object = self.evaluate(object, environment)?;
        self.resolve_member(object, name, span)
    }
}

fn selected_method(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Rc<UserFunction>>, RuntimeError> {
    let definition = rils_execution::value::native_instance::value_definition(value)
        .map_err(|message| RuntimeError::new(message, span))?;
    match definition {
        Some(Value::StructType(definition)) => {
            super::super::call::select_method(&definition.methods, &definition.trait_methods, name)
        }
        Some(Value::EnumType(definition)) => {
            super::super::call::select_method(&definition.methods, &definition.trait_methods, name)
        }
        _ => Ok(None),
    }
    .map_err(|traits| {
        RuntimeError::new(
            format!(
                "method `{name}` is ambiguous; candidates come from traits {}",
                traits.join(", ")
            ),
            span,
        )
    })
}
