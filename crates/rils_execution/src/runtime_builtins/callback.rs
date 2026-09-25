use std::cell::RefCell;
use std::rc::Rc;

use rils_stdlib::stdlib::{option::Option as NativeOption, result::Result as NativeResult};

use crate::{Type, Value, environment::StorageSlot, types::merge_types, value::ReferenceValue};

use super::{NativeCallError, import_receiver};

#[derive(Clone, Copy)]
pub(super) enum Operation {
    OptionMap,
    OptionAndThen,
    OptionOrElse,
    OptionFilter,
    ResultMap,
    ResultMapErr,
    ResultAndThen,
    ResultOrElse,
}

pub(super) fn call<E>(
    operation: Operation,
    arguments: &[Value],
    invoke: &mut super::NativeCallback<'_, E>,
) -> Result<Value, NativeCallError<E>> {
    if arguments.len() != 2 {
        return Err(format!(
            "native callback method expects 2 arguments, found {}",
            arguments.len()
        )
        .into());
    }
    let receiver = import_receiver(&arguments[0])?;
    let function = &arguments[1];
    match operation {
        Operation::OptionMap
        | Operation::OptionAndThen
        | Operation::OptionOrElse
        | Operation::OptionFilter => {
            let Value::Option {
                value,
                element_type,
            } = receiver
            else {
                return Err("callback method expects Option receiver".into());
            };
            let native = match value {
                Some(value) => NativeOption::Some(value),
                None => NativeOption::None,
            };
            let mut mapped_type = None;
            let result = match operation {
                Operation::OptionMap => native.__rils_try_map(|value| {
                    let mapped = invoke(function, &[value.as_ref().clone()])
                        .map_err(NativeCallError::Callback)?;
                    mapped_type = Type::of_value(&mapped);
                    Ok::<_, NativeCallError<E>>(Rc::new(mapped))
                })?,
                Operation::OptionAndThen => native.__rils_try_and_then(|value| {
                    let mapped = invoke(function, &[value.as_ref().clone()])
                        .map_err(NativeCallError::Callback)?;
                    let Value::Option {
                        value,
                        element_type,
                    } = mapped
                    else {
                        return Err("Option::and_then callback must return Option".into());
                    };
                    mapped_type = element_type;
                    Ok::<_, NativeCallError<E>>(match value {
                        Some(value) => NativeOption::Some(value),
                        None => NativeOption::None,
                    })
                })?,
                Operation::OptionOrElse => native.__rils_try_or_else(|| {
                    let mapped = invoke(function, &[]).map_err(NativeCallError::Callback)?;
                    let Value::Option {
                        value,
                        element_type: callback_type,
                    } = mapped
                    else {
                        return Err("Option::or_else callback must return Option".into());
                    };
                    if merge_types(
                        element_type.as_ref().unwrap_or(&Type::Unknown),
                        callback_type.as_ref().unwrap_or(&Type::Unknown),
                    )
                    .is_none()
                    {
                        return Err(
                            "Option::or_else callback returned an incompatible Option".into()
                        );
                    }
                    mapped_type = callback_type;
                    Ok::<_, NativeCallError<E>>(match value {
                        Some(value) => NativeOption::Some(value),
                        None => NativeOption::None,
                    })
                })?,
                Operation::OptionFilter => native.__rils_try_filter(|value| {
                    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
                    storage.borrow_mut().initialize(value.as_ref().clone());
                    let reference =
                        Value::Reference(Rc::new(ReferenceValue::new_storage(storage, false)));
                    let accepted =
                        invoke(function, &[reference]).map_err(NativeCallError::Callback)?;
                    match accepted {
                        Value::Bool(accepted) => Ok::<_, NativeCallError<E>>(accepted),
                        value => Err(format!(
                            "Option::filter callback must return bool, found {}",
                            value.type_name()
                        )
                        .into()),
                    }
                })?,
                _ => unreachable!(),
            };
            Ok(Value::Option {
                value: match result {
                    NativeOption::Some(value) => Some(value),
                    NativeOption::None => None,
                },
                element_type: mapped_type.or(match operation {
                    Operation::OptionOrElse | Operation::OptionFilter => element_type,
                    _ => None,
                }),
            })
        }
        Operation::ResultMap
        | Operation::ResultMapErr
        | Operation::ResultAndThen
        | Operation::ResultOrElse => {
            let Value::Result {
                value,
                ok_type,
                error_type,
            } = receiver
            else {
                return Err("callback method expects Result receiver".into());
            };
            let native = match value {
                Ok(value) => NativeResult::Ok(value),
                Err(value) => NativeResult::Err(value),
            };
            let mut mapped_ok = None;
            let mut mapped_error = None;
            let result = match operation {
                Operation::ResultMap => native.__rils_try_map(|value| {
                    let mapped = invoke(function, &[value.as_ref().clone()])
                        .map_err(NativeCallError::Callback)?;
                    mapped_ok = Type::of_value(&mapped);
                    Ok::<_, NativeCallError<E>>(Rc::new(mapped))
                })?,
                Operation::ResultMapErr => native.__rils_try_map_err(|value| {
                    let mapped = invoke(function, &[value.as_ref().clone()])
                        .map_err(NativeCallError::Callback)?;
                    mapped_error = Type::of_value(&mapped);
                    Ok::<_, NativeCallError<E>>(Rc::new(mapped))
                })?,
                Operation::ResultAndThen => native.__rils_try_and_then(|value| {
                    let mapped = invoke(function, &[value.as_ref().clone()])
                        .map_err(NativeCallError::Callback)?;
                    let Value::Result {
                        value,
                        ok_type: callback_ok,
                        error_type: callback_error,
                    } = mapped
                    else {
                        return Err("Result combinator callback must return Result".into());
                    };
                    if merge_types(
                        error_type.as_ref().unwrap_or(&Type::Unknown),
                        callback_error.as_ref().unwrap_or(&Type::Unknown),
                    )
                    .is_none()
                    {
                        return Err(
                            "Result combinator callback returned an incompatible Result".into()
                        );
                    }
                    mapped_ok = callback_ok;
                    mapped_error = callback_error;
                    Ok::<_, NativeCallError<E>>(match value {
                        Ok(value) => NativeResult::Ok(value),
                        Err(value) => NativeResult::Err(value),
                    })
                })?,
                Operation::ResultOrElse => native.__rils_try_or_else(|value| {
                    let mapped = invoke(function, &[value.as_ref().clone()])
                        .map_err(NativeCallError::Callback)?;
                    let Value::Result {
                        value,
                        ok_type: callback_ok,
                        error_type: callback_error,
                    } = mapped
                    else {
                        return Err("Result combinator callback must return Result".into());
                    };
                    if merge_types(
                        ok_type.as_ref().unwrap_or(&Type::Unknown),
                        callback_ok.as_ref().unwrap_or(&Type::Unknown),
                    )
                    .is_none()
                    {
                        return Err(
                            "Result combinator callback returned an incompatible Result".into()
                        );
                    }
                    mapped_ok = callback_ok;
                    mapped_error = callback_error;
                    Ok::<_, NativeCallError<E>>(match value {
                        Ok(value) => NativeResult::Ok(value),
                        Err(value) => NativeResult::Err(value),
                    })
                })?,
                _ => unreachable!(),
            };
            Ok(Value::Result {
                value: match result {
                    NativeResult::Ok(value) => Ok(value),
                    NativeResult::Err(value) => Err(value),
                },
                ok_type: mapped_ok.or(match operation {
                    Operation::ResultMapErr | Operation::ResultOrElse => ok_type,
                    _ => None,
                }),
                error_type: mapped_error.or(match operation {
                    Operation::ResultMap | Operation::ResultAndThen => error_type,
                    _ => None,
                }),
            })
        }
    }
}
