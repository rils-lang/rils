use super::*;
use rils_stdlib::stdlib::{option::Option as NativeOption, result::Result as NativeResult};

type OptionValue = NativeOption<Rc<Value>>;
type ResultValue = NativeResult<Rc<Value>, Rc<Value>>;

fn option_state(value: &Value) -> Result<(OptionValue, Option<Type>), String> {
    match import_receiver(value)? {
        Value::Option {
            value: Some(value),
            element_type,
        } => Ok((NativeOption::Some(value), element_type)),
        Value::Option {
            value: None,
            element_type,
        } => Ok((NativeOption::None, element_type)),
        value => Err(format!("expected Option, found {}", value.type_name())),
    }
}

fn result_state(value: &Value) -> Result<(ResultValue, Option<Type>, Option<Type>), String> {
    match import_receiver(value)? {
        Value::Result {
            value: Ok(value),
            ok_type,
            error_type,
        } => Ok((NativeResult::Ok(value), ok_type, error_type)),
        Value::Result {
            value: Err(value),
            ok_type,
            error_type,
        } => Ok((NativeResult::Err(value), ok_type, error_type)),
        value => Err(format!("expected Result, found {}", value.type_name())),
    }
}

fn option_value(value: OptionValue, element_type: Option<Type>) -> Value {
    Value::Option {
        value: match value {
            NativeOption::Some(value) => Some(value),
            NativeOption::None => None,
        },
        element_type,
    }
}

pub(super) fn call(id: rils_builtins::BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    use rils_builtins::BuiltinId;

    match id {
        BuiltinId::OptionUnwrap => {
            if matches!(import_receiver(&arguments[0])?, Value::Result { .. }) {
                return call(BuiltinId::ResultUnwrap, arguments);
            }
            let (value, _) = option_state(&arguments[0])?;
            if value.is_none() {
                return Err("called `unwrap` on `None`".into());
            }
            value.unwrap().clone_owned()
        }
        BuiltinId::ResultUnwrap => {
            let (value, _, _) = result_state(&arguments[0])?;
            if let NativeResult::Err(error) = &value {
                return Err(format!("called `unwrap` on Err({error})"));
            }
            value.unwrap().clone_owned()
        }
        BuiltinId::OptionUnwrapOr => {
            if matches!(import_receiver(&arguments[0])?, Value::Result { .. }) {
                return call(BuiltinId::ResultUnwrapOr, arguments);
            }
            let (value, element_type) = option_state(&arguments[0])?;
            let default = &arguments[1];
            if let Some(expected) = element_type
                && !expected.accepts(default)
            {
                return Err(format!(
                    "`unwrap_or` default must be {expected}, found {}",
                    default.type_name()
                ));
            }
            value.unwrap_or(Rc::new(default.clone())).clone_owned()
        }
        BuiltinId::ResultUnwrapOr => {
            let (value, ok_type, _) = result_state(&arguments[0])?;
            let default = &arguments[1];
            if let Some(expected) = ok_type
                && !expected.accepts(default)
            {
                return Err(format!(
                    "`unwrap_or` default must be {expected}, found {}",
                    default.type_name()
                ));
            }
            value.unwrap_or(Rc::new(default.clone())).clone_owned()
        }
        BuiltinId::OptionExpect => {
            if matches!(import_receiver(&arguments[0])?, Value::Result { .. }) {
                return call(BuiltinId::ResultExpect, arguments);
            }
            let Value::String(message) = &arguments[1] else {
                return Err("expect message must be string".into());
            };
            let (value, _) = option_state(&arguments[0])?;
            if value.is_none() {
                return Err(message.to_string());
            }
            value.expect(message.to_string()).clone_owned()
        }
        BuiltinId::ResultExpect => {
            let Value::String(message) = &arguments[1] else {
                return Err("expect message must be string".into());
            };
            let (value, _, _) = result_state(&arguments[0])?;
            if let NativeResult::Err(error) = &value {
                return Err(format!("{message}: {error}"));
            }
            value.expect(message.to_string()).clone_owned()
        }
        BuiltinId::ResultUnwrapErr => {
            let (value, _, _) = result_state(&arguments[0])?;
            if let NativeResult::Ok(ok) = &value {
                return Err(format!("called `unwrap_err` on Ok({ok})"));
            }
            value.unwrap_err().clone_owned()
        }
        BuiltinId::ResultExpectErr => {
            let (value, _, _) = result_state(&arguments[0])?;
            if let NativeResult::Ok(ok) = &value {
                let Value::String(message) = &arguments[1] else {
                    return Err("expect_err message must be string".into());
                };
                return Err(format!("{message}: {ok}"));
            }
            let Value::String(message) = &arguments[1] else {
                return Err("expect_err message must be string".into());
            };
            value.expect_err(message.to_string()).clone_owned()
        }
        BuiltinId::OptionTake => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Option::take requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Option::take requires `&mut self`".into());
            }
            let (mut value, element_type) = option_state(&arguments[0])?;
            let previous = value.take();
            reference
                .write(option_value(value, element_type.clone()))
                .map_err(assignment_error_message)?;
            Ok(option_value(previous, element_type))
        }
        BuiltinId::OptionOr | BuiltinId::OptionXor => {
            let (left, left_type) = option_state(&arguments[0])?;
            let (right, right_type) = option_state(&arguments[1])?;
            let element_type = crate::types::merge_types(
                left_type.as_ref().unwrap_or(&Type::Unknown),
                right_type.as_ref().unwrap_or(&Type::Unknown),
            )
            .ok_or_else(|| "Option operand types do not match".to_string())?;
            let result = if id == BuiltinId::OptionOr {
                left.or(right)
            } else {
                left.xor(right)
            };
            Ok(option_value(result, Some(element_type)))
        }
        BuiltinId::OptionReplace => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("replace receiver must be a reference".into());
            };
            if !reference.mutable {
                return Err("Option::replace requires `&mut self`".into());
            }
            let (mut receiver, element_type) = option_state(&arguments[0])?;
            let value = &arguments[1];
            let expected = element_type.clone().unwrap_or(Type::Unknown);
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            let resolved = crate::types::merge_types(&expected, &actual)
                .ok_or_else(|| format!("Option element type is `{expected}`, found `{actual}`"))?;
            let previous = receiver.replace(Rc::new(value.clone()));
            reference
                .write(option_value(receiver, Some(resolved.clone())))
                .map_err(assignment_error_message)?;
            Ok(option_value(previous, Some(resolved)))
        }
        _ => unreachable!("option/result built-in was matched by the caller"),
    }
}
