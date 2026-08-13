use super::*;

impl Interpreter {
    pub(super) fn call_option_result_method(
        &mut self,
        id: rils_builtins::RuntimeMemberId,
        receiver: &Value,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        use rils_builtins::RuntimeMemberId::*;
        match id {
            ResultIsOk | ResultIsErr => result_state(id, receiver, span),
            ResultUnwrap | ResultUnwrapOr | ResultExpect => {
                result_unwrap(id, receiver, arguments, span)
            }
            ResultOk | ResultErr => result_to_option(id, receiver, span),
            ResultUnwrapErr | ResultExpectErr => result_unwrap_error(id, receiver, arguments, span),
            ResultMap | ResultMapErr | ResultAndThen | ResultOrElse => {
                self.result_transform(id, receiver, arguments, span)
            }
            OptionIsSome | OptionIsNone => option_state(id, receiver, span),
            OptionUnwrap | OptionUnwrapOr | OptionExpect => {
                option_unwrap(id, receiver, arguments, span)
            }
            OptionTake => option_take(receiver, span),
            OptionOr | OptionXor => option_combine(id, receiver, arguments, span),
            OptionReplace => option_replace(receiver, arguments, span),
            OptionMap | OptionAndThen | OptionOrElse => {
                self.option_transform(id, receiver, arguments, span)
            }
            _ => unreachable!("non Option/Result member routed to Option/Result dispatcher"),
        }
    }

    fn option_transform(
        &mut self,
        id: rils_builtins::RuntimeMemberId,
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
        use rils_builtins::RuntimeMemberId::*;
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
            (OptionMap, None) => Ok(Value::Option {
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
            (OptionAndThen, None) => Ok(Value::Option {
                value: None,
                element_type: None,
            }),
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
        id: rils_builtins::RuntimeMemberId,
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
        use rils_builtins::RuntimeMemberId::*;
        match (id, value) {
            (ResultMap, Ok(value)) => {
                let mapped = self.call(function, &[value.as_ref().clone()], span)?;
                owned_result(Ok(mapped), None, error_type.clone(), span)
            }
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
            (ResultMapErr, Err(value)) => {
                let mapped = self.call(function, &[value.as_ref().clone()], span)?;
                owned_result(Err(mapped), ok_type.clone(), None, span)
            }
            (ResultAndThen, Ok(value)) => {
                let mapped = self.call(function, &[value.as_ref().clone()], span)?;
                validate_result_callback(mapped, error_type.as_ref(), false, span)
            }
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
            (ResultOrElse, Err(value)) => {
                let mapped = self.call(function, &[value.as_ref().clone()], span)?;
                validate_result_callback(mapped, ok_type.as_ref(), true, span)
            }
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

fn result_state(
    id: rils_builtins::RuntimeMemberId,
    receiver: &Value,
    span: Span,
) -> Result<Value, RuntimeError> {
    let receiver = read_receiver(receiver, span)?;
    let Value::Result { value, .. } = receiver else {
        return Err(RuntimeError::new(
            "Result method receiver is not Result",
            span,
        ));
    };
    Ok(Value::Bool(match id {
        rils_builtins::RuntimeMemberId::ResultIsOk => value.is_ok(),
        rils_builtins::RuntimeMemberId::ResultIsErr => value.is_err(),
        _ => unreachable!(),
    }))
}

fn result_unwrap(
    id: rils_builtins::RuntimeMemberId,
    receiver: &Value,
    arguments: &[Value],
    span: Span,
) -> Result<Value, RuntimeError> {
    let Value::Result { value, ok_type, .. } = receiver else {
        return Err(RuntimeError::new(
            "Result method receiver is not Result",
            span,
        ));
    };
    if id == rils_builtins::RuntimeMemberId::ResultUnwrapOr {
        let default = &arguments[0];
        if let Some(expected) = ok_type
            && !expected.accepts(default)
        {
            return Err(RuntimeError::new(
                format!(
                    "`unwrap_or` default must be {expected}, found {}",
                    default.type_name()
                ),
                span,
            ));
        }
        return Ok(match value {
            Ok(value) => (**value).clone(),
            Err(_) => default.clone(),
        });
    }
    match value {
        Ok(value) => Ok((**value).clone()),
        Err(value) => {
            let message = if id == rils_builtins::RuntimeMemberId::ResultExpect {
                let Value::String(message) = &arguments[0] else {
                    return Err(RuntimeError::new(
                        "Result::expect message must be string",
                        span,
                    ));
                };
                format!("{message}: {value}")
            } else {
                format!("called `unwrap` on Err({value})")
            };
            Err(RuntimeError::new(message, span))
        }
    }
}

fn result_to_option(
    id: rils_builtins::RuntimeMemberId,
    receiver: &Value,
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
    let (value, element_type) = match (id, value) {
        (rils_builtins::RuntimeMemberId::ResultOk, Ok(value)) => {
            (Some(value.clone()), ok_type.clone())
        }
        (rils_builtins::RuntimeMemberId::ResultErr, Err(value)) => {
            (Some(value.clone()), error_type.clone())
        }
        (rils_builtins::RuntimeMemberId::ResultOk, Err(_)) => (None, ok_type.clone()),
        (rils_builtins::RuntimeMemberId::ResultErr, Ok(_)) => (None, error_type.clone()),
        _ => unreachable!(),
    };
    Ok(Value::Option {
        value,
        element_type,
    })
}

fn result_unwrap_error(
    id: rils_builtins::RuntimeMemberId,
    receiver: &Value,
    arguments: &[Value],
    span: Span,
) -> Result<Value, RuntimeError> {
    let Value::Result { value, .. } = receiver else {
        return Err(RuntimeError::new(
            "Result error extraction receiver is not Result",
            span,
        ));
    };
    match value {
        Err(value) => value
            .as_ref()
            .clone_owned()
            .map_err(|message| RuntimeError::new(message, span)),
        Ok(value) => {
            let message = if id == rils_builtins::RuntimeMemberId::ResultExpectErr {
                let Value::String(message) = &arguments[0] else {
                    return Err(RuntimeError::new("expect_err message must be string", span));
                };
                format!("{message}: {value}")
            } else {
                format!("called `unwrap_err` on Ok({value})")
            };
            Err(RuntimeError::new(message, span))
        }
    }
}

fn option_state(
    id: rils_builtins::RuntimeMemberId,
    receiver: &Value,
    span: Span,
) -> Result<Value, RuntimeError> {
    let receiver = read_receiver(receiver, span)?;
    let Value::Option { value, .. } = receiver else {
        return Err(RuntimeError::new(
            "Option method receiver is not Option",
            span,
        ));
    };
    Ok(Value::Bool(match id {
        rils_builtins::RuntimeMemberId::OptionIsSome => value.is_some(),
        rils_builtins::RuntimeMemberId::OptionIsNone => value.is_none(),
        _ => unreachable!(),
    }))
}

fn option_unwrap(
    id: rils_builtins::RuntimeMemberId,
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
    if id == rils_builtins::RuntimeMemberId::OptionUnwrapOr {
        let default = &arguments[0];
        if let Some(expected) = element_type
            && !expected.accepts(default)
        {
            return Err(RuntimeError::new(
                format!(
                    "`unwrap_or` default must be {expected}, found {}",
                    default.type_name()
                ),
                span,
            ));
        }
        return Ok(value
            .as_ref()
            .map_or_else(|| default.clone(), |value| (**value).clone()));
    }
    value
        .as_ref()
        .map(|value| (**value).clone())
        .ok_or_else(|| {
            if id == rils_builtins::RuntimeMemberId::OptionExpect {
                match &arguments[0] {
                    Value::String(message) => RuntimeError::new(message.to_string(), span),
                    _ => RuntimeError::new("Option::expect message must be string", span),
                }
            } else {
                RuntimeError::new("called `unwrap` on `None`", span)
            }
        })
}

fn option_take(receiver: &Value, span: Span) -> Result<Value, RuntimeError> {
    let Value::Reference(reference) = receiver else {
        return Err(RuntimeError::new(
            "Option::take requires a mutable binding",
            span,
        ));
    };
    if !reference.mutable {
        return Err(RuntimeError::new("Option::take requires `&mut self`", span));
    }
    let Value::Option {
        value,
        element_type,
    } = reference
        .read()
        .map_err(|message| RuntimeError::new(message, span))?
    else {
        return Err(RuntimeError::new(
            "Option::take receiver is not Option",
            span,
        ));
    };
    reference
        .write(Value::Option {
            value: None,
            element_type: element_type.clone(),
        })
        .map_err(|error| super::evaluation::assignment_error(error, "Option", span))?;
    Ok(Value::Option {
        value,
        element_type,
    })
}

fn option_combine(
    id: rils_builtins::RuntimeMemberId,
    receiver: &Value,
    arguments: &[Value],
    span: Span,
) -> Result<Value, RuntimeError> {
    let Value::Option {
        value: left,
        element_type: left_type,
    } = receiver
    else {
        return Err(RuntimeError::new(
            "Option operation receiver is not Option",
            span,
        ));
    };
    let Value::Option {
        value: right,
        element_type: right_type,
    } = &arguments[0]
    else {
        return Err(RuntimeError::new("Option operand must be Option", span));
    };
    let element_type = merge_types(
        left_type.as_ref().unwrap_or(&Type::Unknown),
        right_type.as_ref().unwrap_or(&Type::Unknown),
    )
    .ok_or_else(|| RuntimeError::new("Option operand types do not match", span))?;
    let value = match id {
        rils_builtins::RuntimeMemberId::OptionOr => left.as_ref().or(right.as_ref()).cloned(),
        rils_builtins::RuntimeMemberId::OptionXor => match (left, right) {
            (Some(value), None) | (None, Some(value)) => Some(value.clone()),
            _ => None,
        },
        _ => unreachable!(),
    };
    Ok(Value::Option {
        value,
        element_type: Some(element_type),
    })
}

fn option_replace(
    receiver: &Value,
    arguments: &[Value],
    span: Span,
) -> Result<Value, RuntimeError> {
    let Value::Reference(reference) = receiver else {
        return Err(RuntimeError::new(
            "Option::replace requires a mutable binding",
            span,
        ));
    };
    if !reference.mutable {
        return Err(RuntimeError::new(
            "Option::replace requires `&mut self`",
            span,
        ));
    }
    let Value::Option {
        value: previous,
        element_type,
    } = reference
        .read()
        .map_err(|message| RuntimeError::new(message, span))?
    else {
        return Err(RuntimeError::new("replace receiver is not Option", span));
    };
    let value = &arguments[0];
    if value.contains_reference() {
        return Err(RuntimeError::new(
            "Option cannot own local references",
            span,
        ));
    }
    let expected = element_type.clone().unwrap_or(Type::Unknown);
    let actual = Type::of_value(value).unwrap_or(Type::Unknown);
    let resolved = merge_types(&expected, &actual).ok_or_else(|| {
        RuntimeError::new(
            format!("Option element type is `{expected}`, found `{actual}`"),
            span,
        )
    })?;
    reference
        .write(Value::Option {
            value: Some(Rc::new(value.clone())),
            element_type: Some(resolved.clone()),
        })
        .map_err(|error| super::evaluation::assignment_error(error, "Option", span))?;
    Ok(Value::Option {
        value: previous,
        element_type: Some(resolved),
    })
}

fn read_receiver(value: &Value, span: Span) -> Result<Value, RuntimeError> {
    match value {
        Value::Reference(reference) => reference
            .read()
            .map_err(|message| RuntimeError::new(message, span)),
        value => Ok(value.clone()),
    }
}
