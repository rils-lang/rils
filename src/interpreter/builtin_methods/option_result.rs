use super::*;

#[allow(non_upper_case_globals)]
impl Interpreter {
    pub(super) fn call_option_result_method(
        &mut self,
        id: rils_builtins::BuiltinId,
        receiver: &Value,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        use rils_builtins::builtin_ids::*;
        match id {
            ResultMap | ResultMapErr | ResultAndThen | ResultOrElse => {
                self.result_transform(id, receiver, arguments, span)
            }
            OptionMap | OptionAndThen | OptionOrElse => {
                self.option_transform(id, receiver, arguments, span)
            }
            _ => unreachable!("non callback Option/Result member routed to callback adapter"),
        }
    }

    fn option_transform(
        &mut self,
        id: rils_builtins::BuiltinId,
        receiver: &Value,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let Value::Option {
            value,
            element_type,
        } = receiver
        else {
            return Err(RuntimeError::new(
                "Option method receiver is not Option",
                span,
            ));
        };
        let function = arguments[0].clone();
        use rils_builtins::builtin_ids::*;
        match (id, value) {
            (OptionMap, Some(value)) => {
                let mapped = self.call(function, &[value.as_ref().clone()], span)?;
                if mapped.contains_reference() {
                    return Err(RuntimeError::new(
                        "Option cannot own local references",
                        span,
                    ));
                }
                Ok(Value::Option {
                    element_type: Type::of_value(&mapped),
                    value: Some(Rc::new(mapped)),
                })
            }
            (OptionMap | OptionAndThen, None) => Ok(Value::Option {
                value: None,
                element_type: None,
            }),
            (OptionAndThen, Some(value)) => {
                let mapped = self.call(function, &[value.as_ref().clone()], span)?;
                if !matches!(mapped, Value::Option { .. }) {
                    return Err(RuntimeError::new(
                        "Option::and_then callback must return Option",
                        span,
                    ));
                }
                Ok(mapped)
            }
            (OptionOrElse, Some(_)) => Ok(receiver.clone()),
            (OptionOrElse, None) => {
                let fallback = self.call(function, &[], span)?;
                let Value::Option {
                    element_type: fallback_type,
                    ..
                } = &fallback
                else {
                    return Err(RuntimeError::new(
                        "Option::or_else callback must return Option",
                        span,
                    ));
                };
                if merge_types(
                    element_type.as_ref().unwrap_or(&Type::Unknown),
                    fallback_type.as_ref().unwrap_or(&Type::Unknown),
                )
                .is_none()
                {
                    return Err(RuntimeError::new(
                        "Option::or_else callback returned an incompatible Option",
                        span,
                    ));
                }
                Ok(fallback)
            }
            _ => unreachable!(),
        }
    }

    fn result_transform(
        &mut self,
        id: rils_builtins::BuiltinId,
        receiver: &Value,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let Value::Result {
            value,
            ok_type,
            error_type,
        } = receiver
        else {
            return Err(RuntimeError::new(
                "Result method receiver is not Result",
                span,
            ));
        };
        let function = arguments[0].clone();
        use rils_builtins::builtin_ids::*;
        match (id, value) {
            (ResultMap, Ok(value)) => owned_result(
                Ok(self.call(function, &[value.as_ref().clone()], span)?),
                None,
                error_type.clone(),
                span,
            ),
            (ResultMap, Err(value)) => Ok(Value::Result {
                value: Err(value.clone()),
                ok_type: None,
                error_type: error_type.clone(),
            }),
            (ResultMapErr, Ok(value)) => Ok(Value::Result {
                value: Ok(value.clone()),
                ok_type: ok_type.clone(),
                error_type: None,
            }),
            (ResultMapErr, Err(value)) => owned_result(
                Err(self.call(function, &[value.as_ref().clone()], span)?),
                ok_type.clone(),
                None,
                span,
            ),
            (ResultAndThen, Ok(value)) => validate_result_callback(
                self.call(function, &[value.as_ref().clone()], span)?,
                error_type.as_ref(),
                false,
                span,
            ),
            (ResultAndThen, Err(value)) => Ok(Value::Result {
                value: Err(value.clone()),
                ok_type: None,
                error_type: error_type.clone(),
            }),
            (ResultOrElse, Ok(value)) => Ok(Value::Result {
                value: Ok(value.clone()),
                ok_type: ok_type.clone(),
                error_type: None,
            }),
            (ResultOrElse, Err(value)) => validate_result_callback(
                self.call(function, &[value.as_ref().clone()], span)?,
                ok_type.as_ref(),
                true,
                span,
            ),
            _ => unreachable!(),
        }
    }
}

fn owned_result(
    value: Result<Value, Value>,
    ok_type: Option<Type>,
    error_type: Option<Type>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let contained = match &value {
        Ok(value) | Err(value) => value,
    };
    if contained.contains_reference() {
        return Err(RuntimeError::new(
            "Result cannot own local references",
            span,
        ));
    }
    let (value, inferred_ok, inferred_error) = match value {
        Ok(value) => (
            Ok(Rc::new(value.clone())),
            Type::of_value(&value),
            error_type,
        ),
        Err(value) => (Err(Rc::new(value.clone())), ok_type, Type::of_value(&value)),
    };
    Ok(Value::Result {
        value,
        ok_type: inferred_ok,
        error_type: inferred_error,
    })
}

fn validate_result_callback(
    value: Value,
    preserved: Option<&Type>,
    preserve_ok: bool,
    span: Span,
) -> Result<Value, RuntimeError> {
    let Value::Result {
        ok_type,
        error_type,
        ..
    } = &value
    else {
        return Err(RuntimeError::new(
            "Result combinator callback must return Result",
            span,
        ));
    };
    let callback_type = if preserve_ok { ok_type } else { error_type };
    if merge_types(
        preserved.unwrap_or(&Type::Unknown),
        callback_type.as_ref().unwrap_or(&Type::Unknown),
    )
    .is_none()
    {
        return Err(RuntimeError::new(
            "Result combinator callback returned an incompatible Result",
            span,
        ));
    }
    Ok(value)
}
