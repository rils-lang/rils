use super::*;
use rils_stdlib::stdlib::{option::Option as NativeOption, result::Result as NativeResult};

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
        let native = match value {
            Some(value) => NativeOption::Some(value.clone()),
            None => NativeOption::None,
        };
        let function = arguments[0].clone();
        use rils_builtins::builtin_ids::*;
        match id {
            OptionMap => {
                let mapped =
                    native.try_map(|value| self.call(function, &[value.as_ref().clone()], span))?;
                Ok(match mapped {
                    NativeOption::Some(value) => Value::Option {
                        element_type: Type::of_value(&value),
                        value: Some(Rc::new(value)),
                    },
                    NativeOption::None => Value::Option {
                        value: None,
                        element_type: None,
                    },
                })
            }
            OptionAndThen => {
                let mut mapped_type = None;
                let mapped = native.try_and_then(|value| {
                    let result = self.call(function, &[value.as_ref().clone()], span)?;
                    let Value::Option {
                        value,
                        element_type,
                    } = result
                    else {
                        return Err(RuntimeError::new(
                            "Option::and_then callback must return Option",
                            span,
                        ));
                    };
                    mapped_type = element_type;
                    Ok(match value {
                        Some(value) => NativeOption::Some(value),
                        None => NativeOption::None,
                    })
                })?;
                Ok(option_value(mapped, mapped_type))
            }
            OptionOrElse => {
                let mut fallback_type = None;
                let mapped = native.try_or_else(|| {
                    let result = self.call(function, &[], span)?;
                    let Value::Option {
                        value,
                        element_type: callback_element_type,
                    } = result
                    else {
                        return Err(RuntimeError::new(
                            "Option::or_else callback must return Option",
                            span,
                        ));
                    };
                    if merge_types(
                        element_type.as_ref().unwrap_or(&Type::Unknown),
                        callback_element_type.as_ref().unwrap_or(&Type::Unknown),
                    )
                    .is_none()
                    {
                        return Err(RuntimeError::new(
                            "Option::or_else callback returned an incompatible Option",
                            span,
                        ));
                    }
                    fallback_type = callback_element_type;
                    Ok(match value {
                        Some(value) => NativeOption::Some(value),
                        None => NativeOption::None,
                    })
                })?;
                let result_type = fallback_type.or_else(|| element_type.clone());
                Ok(option_value(mapped, result_type))
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
        let native = match value {
            Ok(value) => NativeResult::Ok(value.clone()),
            Err(value) => NativeResult::Err(value.clone()),
        };
        let function = arguments[0].clone();
        use rils_builtins::builtin_ids::*;
        match id {
            ResultMap => {
                let mapped =
                    native.try_map(|value| self.call(function, &[value.as_ref().clone()], span))?;
                Ok(match mapped {
                    NativeResult::Ok(value) => Value::Result {
                        ok_type: Type::of_value(&value),
                        value: Ok(Rc::new(value)),
                        error_type: error_type.clone(),
                    },
                    NativeResult::Err(value) => Value::Result {
                        value: Err(value),
                        ok_type: None,
                        error_type: error_type.clone(),
                    },
                })
            }
            ResultMapErr => {
                let mapped = native
                    .try_map_err(|value| self.call(function, &[value.as_ref().clone()], span))?;
                Ok(match mapped {
                    NativeResult::Ok(value) => Value::Result {
                        value: Ok(value),
                        ok_type: ok_type.clone(),
                        error_type: None,
                    },
                    NativeResult::Err(value) => Value::Result {
                        error_type: Type::of_value(&value),
                        value: Err(Rc::new(value)),
                        ok_type: ok_type.clone(),
                    },
                })
            }
            ResultAndThen => {
                let mut callback_types = None;
                let mapped = native.try_and_then(|value| {
                    let result = validate_result_callback(
                        self.call(function, &[value.as_ref().clone()], span)?,
                        error_type.as_ref(),
                        false,
                        span,
                    )?;
                    let Value::Result {
                        value,
                        ok_type,
                        error_type,
                    } = result
                    else {
                        unreachable!()
                    };
                    callback_types = Some((ok_type, error_type));
                    Ok(match value {
                        Ok(value) => NativeResult::Ok(value),
                        Err(value) => NativeResult::Err(value),
                    })
                })?;
                let (mapped_ok, mapped_error) =
                    callback_types.unwrap_or((None, error_type.clone()));
                Ok(result_value(mapped, mapped_ok, mapped_error))
            }
            ResultOrElse => {
                let mut callback_types = None;
                let mapped = native.try_or_else(|value| {
                    let result = validate_result_callback(
                        self.call(function, &[value.as_ref().clone()], span)?,
                        ok_type.as_ref(),
                        true,
                        span,
                    )?;
                    let Value::Result {
                        value,
                        ok_type,
                        error_type,
                    } = result
                    else {
                        unreachable!()
                    };
                    callback_types = Some((ok_type, error_type));
                    Ok(match value {
                        Ok(value) => NativeResult::Ok(value),
                        Err(value) => NativeResult::Err(value),
                    })
                })?;
                let (mapped_ok, mapped_error) = callback_types.unwrap_or((ok_type.clone(), None));
                Ok(result_value(mapped, mapped_ok, mapped_error))
            }
            _ => unreachable!(),
        }
    }
}

fn option_value(value: NativeOption<Rc<Value>>, element_type: Option<Type>) -> Value {
    Value::Option {
        value: match value {
            NativeOption::Some(value) => Some(value),
            NativeOption::None => None,
        },
        element_type,
    }
}

fn result_value(
    value: NativeResult<Rc<Value>, Rc<Value>>,
    ok_type: Option<Type>,
    error_type: Option<Type>,
) -> Value {
    Value::Result {
        value: match value {
            NativeResult::Ok(value) => Ok(value),
            NativeResult::Err(value) => Err(value),
        },
        ok_type,
        error_type,
    }
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
