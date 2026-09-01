use super::*;

pub(super) fn call(id: rils_builtins::BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    use rils_builtins::BuiltinId;

    match id {
        BuiltinId::ResultIsOk => match import_receiver(&arguments[0])? {
            Value::Result { value, .. } => Ok(Value::Bool(value.is_ok())),
            value => Err(format!(
                "`is_ok` expects Result, found {}",
                value.type_name()
            )),
        },
        BuiltinId::ResultIsErr => match import_receiver(&arguments[0])? {
            Value::Result { value, .. } => Ok(Value::Bool(value.is_err())),
            value => Err(format!(
                "`is_err` expects Result, found {}",
                value.type_name()
            )),
        },
        BuiltinId::OptionIsSome => match import_receiver(&arguments[0])? {
            Value::Option { value, .. } => Ok(Value::Bool(value.is_some())),
            value => Err(format!(
                "`is_some` expects Option, found {}",
                value.type_name()
            )),
        },
        BuiltinId::OptionIsNone => match import_receiver(&arguments[0])? {
            Value::Option { value, .. } => Ok(Value::Bool(value.is_none())),
            value => Err(format!(
                "`is_none` expects Option, found {}",
                value.type_name()
            )),
        },
        BuiltinId::OptionUnwrap | BuiltinId::ResultUnwrap => match &arguments[0] {
            Value::Option {
                value: Some(value), ..
            }
            | Value::Result {
                value: Ok(value), ..
            } => value.clone_owned(),
            Value::Option { value: None, .. } => Err("called `unwrap` on `None`".into()),
            Value::Result {
                value: Err(value), ..
            } => Err(format!("called `unwrap` on Err({value})")),
            value => Err(format!(
                "`unwrap` expects Option or Result, found {}",
                value.type_name()
            )),
        },
        BuiltinId::OptionUnwrapOr | BuiltinId::ResultUnwrapOr => match &arguments[0] {
            Value::Option {
                value,
                element_type,
            } => {
                if let Some(expected) = element_type
                    && !expected.accepts(&arguments[1])
                {
                    return Err(format!(
                        "`unwrap_or` default must be {expected}, found {}",
                        arguments[1].type_name()
                    ));
                }
                value
                    .as_ref()
                    .map_or_else(|| arguments[1].clone_owned(), |value| value.clone_owned())
            }
            Value::Result { value, ok_type, .. } => {
                if let Some(expected) = ok_type
                    && !expected.accepts(&arguments[1])
                {
                    return Err(format!(
                        "`unwrap_or` default must be {expected}, found {}",
                        arguments[1].type_name()
                    ));
                }
                match value {
                    Ok(value) => value.clone_owned(),
                    Err(_) => arguments[1].clone_owned(),
                }
            }
            value => Err(format!(
                "`unwrap_or` expects Option or Result, found {}",
                value.type_name()
            )),
        },
        BuiltinId::OptionExpect | BuiltinId::ResultExpect => {
            let Value::String(message) = &arguments[1] else {
                return Err("expect message must be string".into());
            };
            match &arguments[0] {
                Value::Option {
                    value: Some(value), ..
                }
                | Value::Result {
                    value: Ok(value), ..
                } => value.clone_owned(),
                Value::Option { value: None, .. } => Err(message.to_string()),
                Value::Result {
                    value: Err(value), ..
                } => Err(format!("{message}: {value}")),
                value => Err(format!(
                    "expect requires Option or Result, found {}",
                    value.type_name()
                )),
            }
        }
        BuiltinId::ResultOk | BuiltinId::ResultErr => {
            let Value::Result {
                value,
                ok_type,
                error_type,
            } = &arguments[0]
            else {
                return Err("Result conversion receiver is not Result".into());
            };
            let (value, element_type) = match (id, value) {
                (BuiltinId::ResultOk, Ok(value)) => (Some(value.clone()), ok_type.clone()),
                (BuiltinId::ResultErr, Err(value)) => (Some(value.clone()), error_type.clone()),
                (BuiltinId::ResultOk, Err(_)) => (None, ok_type.clone()),
                (BuiltinId::ResultErr, Ok(_)) => (None, error_type.clone()),
                _ => unreachable!(),
            };
            Ok(Value::Option {
                value,
                element_type,
            })
        }
        BuiltinId::ResultUnwrapErr | BuiltinId::ResultExpectErr => {
            let Value::Result { value, .. } = &arguments[0] else {
                return Err("Result error extraction receiver is not Result".into());
            };
            match value {
                Err(value) => value.clone_owned(),
                Ok(value) => {
                    if id == BuiltinId::ResultExpectErr {
                        let Value::String(message) = &arguments[1] else {
                            return Err("expect_err message must be string".into());
                        };
                        Err(format!("{message}: {value}"))
                    } else {
                        Err(format!("called `unwrap_err` on Ok({value})"))
                    }
                }
            }
        }
        BuiltinId::OptionTake => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Option::take requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Option::take requires `&mut self`".into());
            }
            let Value::Option {
                value,
                element_type,
            } = reference.read()?
            else {
                return Err("Option::take receiver is not Option".into());
            };
            reference
                .write(Value::Option {
                    value: None,
                    element_type: element_type.clone(),
                })
                .map_err(assignment_error_message)?;
            Ok(Value::Option {
                value,
                element_type,
            })
        }
        BuiltinId::OptionOr | BuiltinId::OptionXor => {
            let Value::Option {
                value: left,
                element_type: left_type,
            } = &arguments[0]
            else {
                return Err("Option operation receiver is not Option".into());
            };
            let Value::Option {
                value: right,
                element_type: right_type,
            } = &arguments[1]
            else {
                return Err("Option operand must be Option".into());
            };
            let element_type = crate::types::merge_types(
                left_type.as_ref().unwrap_or(&Type::Unknown),
                right_type.as_ref().unwrap_or(&Type::Unknown),
            )
            .ok_or_else(|| "Option operand types do not match".to_string())?;
            let value = if id == BuiltinId::OptionOr {
                left.as_ref().or(right.as_ref()).cloned()
            } else {
                match (left, right) {
                    (Some(value), None) | (None, Some(value)) => Some(value.clone()),
                    _ => None,
                }
            };
            Ok(Value::Option {
                value,
                element_type: Some(element_type),
            })
        }
        BuiltinId::OptionReplace => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("replace receiver must be a reference".into());
            };
            let receiver = reference.read()?;
            if !reference.mutable {
                return Err("Option::replace requires `&mut self`".into());
            }
            let Value::Option {
                value: previous,
                element_type,
            } = receiver
            else {
                return Err("replace receiver is not Option".into());
            };
            let value = &arguments[1];
            if value.contains_reference() {
                return Err("Option cannot own local references".into());
            }
            let expected = element_type.clone().unwrap_or(Type::Unknown);
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            let resolved = crate::types::merge_types(&expected, &actual)
                .ok_or_else(|| format!("Option element type is `{expected}`, found `{actual}`"))?;
            reference
                .write(Value::Option {
                    value: Some(Rc::new(value.clone())),
                    element_type: Some(resolved.clone()),
                })
                .map_err(assignment_error_message)?;
            Ok(Value::Option {
                value: previous,
                element_type: Some(resolved),
            })
        }
        _ => unreachable!("option/result built-in was matched by the caller"),
    }
}
