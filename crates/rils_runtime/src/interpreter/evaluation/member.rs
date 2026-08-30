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
            if let Value::Struct(instance) = &value
                && instance.fields.borrow().contains_key(name)
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
            let builtin_borrow = super::super::call::builtin_runtime_member(&value, name).and_then(
                |(_, receiver)| match receiver {
                    rils_builtins::ReceiverMode::Shared => Some(false),
                    rils_builtins::ReceiverMode::Mutable => Some(true),
                    rils_builtins::ReceiverMode::Owned => None,
                },
            );
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
            let value = place.read(span)?;
            if let Some(mutable) =
                selected_method(&value, name, span)?
                    .as_ref()
                    .and_then(|method| {
                        match method.parameters.first()?.type_annotation.as_ref()? {
                            Type::Reference { mutable, .. } => Some(*mutable),
                            _ => None,
                        }
                    })
            {
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
    match value {
        Value::Struct(instance) => super::super::call::select_method(
            &instance.type_definition.methods,
            &instance.type_definition.trait_methods,
            name,
        ),
        Value::Enum(instance) => super::super::call::select_method(
            &instance.type_definition.methods,
            &instance.type_definition.trait_methods,
            name,
        ),
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
