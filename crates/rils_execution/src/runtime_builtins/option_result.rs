use super::*;
use rils_stdlib::stdlib::{option::Option as NativeOption, result::Result as NativeResult};
use rils_value::{DynamicType, DynamicValue};

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

pub(super) fn call_owned(
    owner: &str,
    method: &str,
    mut arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    let owner = if owner == "Option"
        && arguments
            .first()
            .and_then(Type::of_value)
            .is_some_and(|ty| matches!(ty, Type::Result(_, _)))
    {
        "Result"
    } else {
        owner
    };
    let expected = match method {
        "unwrap" | "unwrap_err" | "take" | "ok" | "err" => 1,
        "unwrap_or" | "expect" | "expect_err" | "or" | "xor" | "replace" => 2,
        _ => return Err(format!("{owner}::{method} has no consuming native adapter")),
    };
    if arguments.len() != expected {
        return Err(format!(
            "{owner}::{method} expects {expected} arguments, found {}",
            arguments.len()
        ));
    }
    let receiver = arguments.remove(0);
    let take_option = |value| {
        crate::value::dynamic_option::take_owned_with_definitions(
            value,
            &context.structs,
            &context.enums,
        )
    };
    let take_result = |value| {
        crate::value::dynamic_result::take_owned_with_definitions(
            value,
            &context.structs,
            &context.enums,
        )
    };
    match (owner, method) {
        ("Option", "take" | "replace") => {
            call_owned_mutable_option(method, receiver, arguments.pop(), context)
        }
        ("Option", "unwrap") => {
            take_option(receiver)?.ok_or_else(|| "called `unwrap` on `None`".into())
        }
        ("Option", "unwrap_or") => {
            let default = arguments.pop().expect("arity checked");
            if let Some(Type::Option(item_type)) = Type::of_value(&receiver)
                && !item_type.accepts(&default)
            {
                return Err(format!(
                    "`unwrap_or` default must be {item_type}, found {}",
                    default.type_name()
                ));
            }
            Ok(take_option(receiver)?.unwrap_or(default))
        }
        ("Option", "expect") => {
            let message = arguments[0]
                .as_string()
                .ok_or("expect message must be string")?;
            take_option(receiver)?.ok_or(message)
        }
        ("Option", "or" | "xor") => {
            let right = arguments.pop().expect("arity checked");
            let left_type = Type::of_value(&receiver)
                .and_then(|ty| match ty {
                    Type::Option(item) => Some(*item),
                    _ => None,
                })
                .unwrap_or(Type::Unknown);
            let right_type = Type::of_value(&right)
                .and_then(|ty| match ty {
                    Type::Option(item) => Some(*item),
                    _ => None,
                })
                .unwrap_or(Type::Unknown);
            let item_type = crate::types::merge_types(&left_type, &right_type)
                .ok_or_else(|| "Option operand types do not match".to_owned())?;
            let left = take_option(receiver)?;
            let right = take_option(right)?;
            let selected = if method == "or" {
                left.or(right)
            } else {
                left.xor(right)
            };
            match crate::value::dynamic_option::construct(selected, &item_type)? {
                crate::value::dynamic_option::Construction::Native(value) => Ok(value),
                crate::value::dynamic_option::Construction::Unsupported(value) => {
                    Ok(Value::Option {
                        value: value.map(Rc::new),
                        element_type: Some(item_type),
                    })
                }
            }
        }
        ("Result", "unwrap") => {
            take_result(receiver)?.map_err(|error| format!("called `unwrap` on Err({error})"))
        }
        ("Result", "ok" | "err") => {
            let Type::Result(ok_type, error_type) =
                Type::of_value(&receiver).ok_or("Result has no concrete type")?
            else {
                return Err("Result receiver has the wrong type".into());
            };
            let item_type = if method == "ok" {
                *ok_type
            } else {
                *error_type
            };
            let branch = take_result(receiver)?;
            let item = if method == "ok" {
                branch.ok()
            } else {
                branch.err()
            };
            match crate::value::dynamic_option::construct(item, &item_type)? {
                crate::value::dynamic_option::Construction::Native(value) => Ok(value),
                crate::value::dynamic_option::Construction::Unsupported(item) => {
                    Ok(Value::Option {
                        value: item.map(Rc::new),
                        element_type: Some(item_type),
                    })
                }
            }
        }
        ("Result", "unwrap_or") => {
            let default = arguments.pop().expect("arity checked");
            if let Some(Type::Result(ok_type, _)) = Type::of_value(&receiver)
                && !ok_type.accepts(&default)
            {
                return Err(format!(
                    "`unwrap_or` default must be {ok_type}, found {}",
                    default.type_name()
                ));
            }
            Ok(take_result(receiver)?.unwrap_or(default))
        }
        ("Result", "expect") => {
            let message = arguments[0]
                .as_string()
                .ok_or("expect message must be string")?;
            take_result(receiver)?.map_err(|error| format!("{message}: {error}"))
        }
        ("Result", "unwrap_err") => match take_result(receiver)? {
            Ok(value) => Err(format!("called `unwrap_err` on Ok({value})")),
            Err(error) => Ok(error),
        },
        ("Result", "expect_err") => {
            let message = arguments[0]
                .as_string()
                .ok_or("expect_err message must be string")?;
            match take_result(receiver)? {
                Ok(value) => Err(format!("{message}: {value}")),
                Err(error) => Ok(error),
            }
        }
        _ => Err(format!("{owner}::{method} has no consuming native adapter")),
    }
}

fn call_owned_mutable_option(
    method: &str,
    receiver: Value,
    replacement: Option<Value>,
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    let Value::Reference(reference) = &receiver else {
        return Err(format!("Option::{method} requires a mutable binding"));
    };
    if !reference.mutable {
        return Err(format!("Option::{method} requires `&mut self`"));
    }
    let current = reference.read()?;
    let Value::Dynamic(object) = current else {
        let mut arguments = vec![receiver];
        if let Some(replacement) = replacement {
            arguments.push(replacement);
        }
        return call("Option", method, &arguments);
    };
    let layout = object.descriptor().layout_handle();
    if !matches!(layout.rils_type(), Type::Option(_)) {
        return Err(format!("Option::{method} expects Option receiver"));
    }
    let next = if let Some(replacement) = replacement {
        let item_layout = layout.option_item().ok_or("Option has no item layout")?;
        let item = crate::value::record_codec::NativeRecordCodec::with_definitions(
            &context.structs,
            &context.enums,
        )
        .into_native(replacement, item_layout.clone())?;
        DynamicValue::some(layout.clone(), item)?
    } else {
        DynamicValue::none(layout.clone())?
    };
    if object.is_inline() {
        let descriptor = Rc::new(DynamicType::new(layout));
        let next = crate::value::DynamicObject::new(descriptor, next)?;
        reference
            .write(Value::Dynamic(next))
            .map_err(assignment_error_message)?;
        return Ok(Value::Dynamic(object));
    }
    let previous = object.with_mut(|payload| std::mem::replace(payload, next))?;
    let descriptor = Rc::new(DynamicType::new(layout));
    crate::value::DynamicObject::new(descriptor, previous).map(Value::Dynamic)
}

pub(super) fn call(owner: &str, method: &str, arguments: &[Value]) -> Result<Value, String> {
    match (owner, method) {
        ("Option", "unwrap") => {
            if matches!(import_receiver(&arguments[0])?, Value::Result { .. }) {
                return call("Result", "unwrap", arguments);
            }
            let (value, _) = option_state(&arguments[0])?;
            if value.is_none() {
                return Err("called `unwrap` on `None`".into());
            }
            value.unwrap().clone_owned()
        }
        ("Result", "unwrap") => {
            let (value, _, _) = result_state(&arguments[0])?;
            if let NativeResult::Err(error) = &value {
                return Err(format!("called `unwrap` on Err({error})"));
            }
            value.unwrap().clone_owned()
        }
        ("Option", "unwrap_or") => {
            if matches!(import_receiver(&arguments[0])?, Value::Result { .. }) {
                return call("Result", "unwrap_or", arguments);
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
        ("Result", "unwrap_or") => {
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
        ("Option", "expect") => {
            if matches!(import_receiver(&arguments[0])?, Value::Result { .. }) {
                return call("Result", "expect", arguments);
            }
            let message = arguments[1]
                .as_string()
                .ok_or("expect message must be string")?;
            let (value, _) = option_state(&arguments[0])?;
            if value.is_none() {
                return Err(message.to_string());
            }
            value.expect(message.to_string()).clone_owned()
        }
        ("Result", "expect") => {
            let message = arguments[1]
                .as_string()
                .ok_or("expect message must be string")?;
            let (value, _, _) = result_state(&arguments[0])?;
            if let NativeResult::Err(error) = &value {
                return Err(format!("{message}: {error}"));
            }
            value.expect(message.to_string()).clone_owned()
        }
        ("Result", "unwrap_err") => {
            let (value, _, _) = result_state(&arguments[0])?;
            if let NativeResult::Ok(ok) = &value {
                return Err(format!("called `unwrap_err` on Ok({ok})"));
            }
            value.unwrap_err().clone_owned()
        }
        ("Result", "expect_err") => {
            let (value, _, _) = result_state(&arguments[0])?;
            if let NativeResult::Ok(ok) = &value {
                let message = arguments[1]
                    .as_string()
                    .ok_or("expect_err message must be string")?;
                return Err(format!("{message}: {ok}"));
            }
            let message = arguments[1]
                .as_string()
                .ok_or("expect_err message must be string")?;
            value.expect_err(message.to_string()).clone_owned()
        }
        ("Option", "take") => {
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
        ("Option", "or" | "xor") => {
            let (left, left_type) = option_state(&arguments[0])?;
            let (right, right_type) = option_state(&arguments[1])?;
            let element_type = crate::types::merge_types(
                left_type.as_ref().unwrap_or(&Type::Unknown),
                right_type.as_ref().unwrap_or(&Type::Unknown),
            )
            .ok_or_else(|| "Option operand types do not match".to_string())?;
            let result = if method == "or" {
                left.or(right)
            } else {
                left.xor(right)
            };
            Ok(option_value(result, Some(element_type)))
        }
        ("Option", "replace") => {
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
