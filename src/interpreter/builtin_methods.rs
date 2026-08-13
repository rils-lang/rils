use super::*;

impl Interpreter {
    pub(super) fn call_builtin_method(
        &mut self,
        method: &BuiltinBoundMethod,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let arity = match method.method {
            BuiltinMethod::IntegerIntrinsic(id) => rils_builtins::INTEGER_INTRINSICS
                .iter()
                .find(|item| item.id == id)
                .map_or(0, |item| item.signature.parameters.len()),
            BuiltinMethod::Runtime(id) => rils_builtins::runtime_member(id)
                .and_then(|(_, member)| member.signature)
                .map_or(0, |signature| signature.parameters.len()),
        };
        check_arity("builtin method", arity, arity, arguments.len(), span)?;
        match method.method {
            BuiltinMethod::IntegerIntrinsic(id) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::numeric::execute_integer_intrinsic(id, None, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::RangeIntoIter) => {
                Ok((*method.receiver).clone())
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::Clone) => {
                let value = match method.receiver.as_ref() {
                    Value::Reference(reference) => reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, span))?,
                    value => value.clone(),
                };
                value
                    .clone_owned()
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::RangeNext) => {
                let Value::Reference(reference) = method.receiver.as_ref() else {
                    return Err(RuntimeError::new(
                        "Range::next requires a mutable range binding",
                        span,
                    ));
                };
                if !reference.mutable {
                    return Err(RuntimeError::new("Range::next requires `&mut self`", span));
                }
                let Value::Range(mut range) = reference
                    .read()
                    .map_err(|message| RuntimeError::new(message, span))?
                else {
                    return Err(RuntimeError::new(
                        "Range::next receiver is not a Range",
                        span,
                    ));
                };
                let element_type = range.element_type();
                let current = range
                    .next()
                    .map_err(|message| RuntimeError::new(message, span))?;
                reference
                    .write(Value::Range(range))
                    .map_err(|error| super::evaluation::assignment_error(error, "Range", span))?;
                Ok(Value::Option {
                    value: current.map(Rc::new),
                    element_type: Some(element_type),
                })
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::SequenceLen) => {
                let value = match method.receiver.as_ref() {
                    Value::Reference(reference) => reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, span))?,
                    value => value.clone(),
                };
                let length = match value {
                    Value::Array(sequence) | Value::Vec(sequence) => {
                        sequence.elements.borrow().len()
                    }
                    _ => {
                        return Err(RuntimeError::new("len receiver is not a collection", span));
                    }
                };
                Ok(Value::Usize(length))
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::SequenceIsEmpty) => {
                let value = match method.receiver.as_ref() {
                    Value::Reference(reference) => reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, span))?,
                    value => value.clone(),
                };
                let empty = match value {
                    Value::Array(sequence) | Value::Vec(sequence) => {
                        sequence.elements.borrow().is_empty()
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "is_empty receiver is not a collection",
                            span,
                        ));
                    }
                };
                Ok(Value::Bool(empty))
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::VecPush) => {
                let Value::Reference(reference) = method.receiver.as_ref() else {
                    return Err(RuntimeError::new(
                        "Vec::push requires a mutable binding",
                        span,
                    ));
                };
                if !reference.mutable {
                    return Err(RuntimeError::new("Vec::push requires `&mut self`", span));
                }
                let Value::Vec(sequence) = reference
                    .read()
                    .map_err(|message| RuntimeError::new(message, span))?
                else {
                    return Err(RuntimeError::new("push receiver is not Vec", span));
                };
                let value = &arguments[0];
                if value.contains_reference() {
                    return Err(RuntimeError::new("Vec cannot own local references", span));
                }
                let current = sequence
                    .elements
                    .borrow()
                    .first()
                    .map(|slot| slot.type_annotation.clone())
                    .or_else(|| sequence.element_type.borrow().clone())
                    .unwrap_or(Type::Unknown);
                let actual = Type::of_value(value).unwrap_or(Type::Unknown);
                let element_type = merge_types(&current, &actual).ok_or_else(|| {
                    RuntimeError::new(
                        format!("Vec element type is `{current}`, found `{actual}`"),
                        span,
                    )
                })?;
                *sequence.element_type.borrow_mut() = Some(element_type.clone());
                sequence.elements.borrow_mut().push(FieldSlot {
                    value: Some(value.clone()),
                    type_annotation: element_type,
                    references: 0,
                });
                Ok(Value::Unit)
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::VecPop) => {
                let Value::Reference(reference) = method.receiver.as_ref() else {
                    return Err(RuntimeError::new(
                        "Vec::pop requires a mutable binding",
                        span,
                    ));
                };
                if !reference.mutable {
                    return Err(RuntimeError::new("Vec::pop requires `&mut self`", span));
                }
                let Value::Vec(sequence) = reference
                    .read()
                    .map_err(|message| RuntimeError::new(message, span))?
                else {
                    return Err(RuntimeError::new("pop receiver is not Vec", span));
                };
                let element_type = sequence
                    .element_type
                    .borrow()
                    .clone()
                    .unwrap_or(Type::Unknown);
                let value = {
                    let mut elements = sequence.elements.borrow_mut();
                    if elements.last().is_some_and(|slot| slot.references > 0) {
                        return Err(RuntimeError::new(
                            "cannot pop a referenced Vec element",
                            span,
                        ));
                    }
                    elements.pop().and_then(|slot| slot.value).map(Rc::new)
                };
                Ok(Value::Option {
                    value,
                    element_type: Some(element_type),
                })
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::VecClear
                | rils_builtins::RuntimeMemberId::VecTruncate),
            ) => {
                let Value::Reference(reference) = method.receiver.as_ref() else {
                    return Err(RuntimeError::new(
                        "Vec mutation requires a mutable binding",
                        span,
                    ));
                };
                if !reference.mutable {
                    return Err(RuntimeError::new("Vec mutation requires `&mut self`", span));
                }
                let Value::Vec(sequence) = reference
                    .read()
                    .map_err(|message| RuntimeError::new(message, span))?
                else {
                    return Err(RuntimeError::new("receiver is not Vec", span));
                };
                let length = if id == rils_builtins::RuntimeMemberId::VecClear {
                    0
                } else {
                    let Value::Usize(length) = arguments[0] else {
                        return Err(RuntimeError::new(
                            "Vec::truncate length must be usize",
                            span,
                        ));
                    };
                    length
                };
                let mut elements = sequence.elements.borrow_mut();
                if elements
                    .get(length..)
                    .is_some_and(|tail| tail.iter().any(|slot| slot.references > 0))
                {
                    return Err(RuntimeError::new(
                        "cannot remove a referenced Vec element",
                        span,
                    ));
                }
                elements.truncate(length);
                Ok(Value::Unit)
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::SequenceIntoIter) => {
                let sequence = match method.receiver.as_ref() {
                    Value::Array(sequence) | Value::Vec(sequence) => sequence,
                    _ => {
                        return Err(RuntimeError::new(
                            "into_iter receiver is not a collection",
                            span,
                        ));
                    }
                };
                if sequence
                    .elements
                    .borrow()
                    .iter()
                    .any(|slot| slot.references > 0)
                {
                    return Err(RuntimeError::new(
                        "cannot iterate a collection while an element is referenced",
                        span,
                    ));
                }
                let element_type = sequence
                    .element_type
                    .borrow()
                    .clone()
                    .unwrap_or(Type::Unknown);
                let items = sequence
                    .elements
                    .borrow_mut()
                    .drain(..)
                    .filter_map(|slot| slot.value)
                    .collect();
                Ok(Value::SequenceIterator(Rc::new(SequenceIteratorValue {
                    items: RefCell::new(items),
                    element_type,
                })))
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::IteratorNext) => {
                let Value::Reference(reference) = method.receiver.as_ref() else {
                    return Err(RuntimeError::new(
                        "Iterator::next requires a mutable binding",
                        span,
                    ));
                };
                if !reference.mutable {
                    return Err(RuntimeError::new(
                        "Iterator::next requires `&mut self`",
                        span,
                    ));
                }
                let Value::SequenceIterator(iterator) = reference
                    .read()
                    .map_err(|message| RuntimeError::new(message, span))?
                else {
                    return Err(RuntimeError::new("next receiver is not an iterator", span));
                };
                let value = iterator.items.borrow_mut().pop_front().map(Rc::new);
                Ok(Value::Option {
                    value,
                    element_type: Some(iterator.element_type.clone()),
                })
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::ResultIsOk
                | rils_builtins::RuntimeMemberId::ResultIsErr),
            ) => {
                let receiver = match method.receiver.as_ref() {
                    Value::Reference(reference) => reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, span))?,
                    value => value.clone(),
                };
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
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::ResultUnwrap
                | rils_builtins::RuntimeMemberId::ResultUnwrapOr
                | rils_builtins::RuntimeMemberId::ResultExpect),
            ) => {
                let Value::Result { value, ok_type, .. } = method.receiver.as_ref() else {
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
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::ResultOk
                | rils_builtins::RuntimeMemberId::ResultErr),
            ) => {
                let Value::Result {
                    value,
                    ok_type,
                    error_type,
                } = method.receiver.as_ref()
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
                    (rils_builtins::RuntimeMemberId::ResultErr, Ok(_)) => {
                        (None, error_type.clone())
                    }
                    _ => unreachable!(),
                };
                Ok(Value::Option {
                    value,
                    element_type,
                })
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::OptionIsSome
                | rils_builtins::RuntimeMemberId::OptionIsNone),
            ) => {
                let receiver = match method.receiver.as_ref() {
                    Value::Reference(reference) => reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, span))?,
                    value => value.clone(),
                };
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
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::OptionUnwrap
                | rils_builtins::RuntimeMemberId::OptionUnwrapOr
                | rils_builtins::RuntimeMemberId::OptionExpect),
            ) => {
                let Value::Option {
                    value,
                    element_type,
                } = method.receiver.as_ref()
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
                                Value::String(message) => {
                                    RuntimeError::new(message.to_string(), span)
                                }
                                _ => {
                                    RuntimeError::new("Option::expect message must be string", span)
                                }
                            }
                        } else {
                            RuntimeError::new("called `unwrap` on `None`", span)
                        }
                    })
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::OptionTake) => {
                let Value::Reference(reference) = method.receiver.as_ref() else {
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
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::StringLen
                | rils_builtins::RuntimeMemberId::StringIsEmpty
                | rils_builtins::RuntimeMemberId::StringContains
                | rils_builtins::RuntimeMemberId::StringStartsWith
                | rils_builtins::RuntimeMemberId::StringEndsWith
                | rils_builtins::RuntimeMemberId::StringFind
                | rils_builtins::RuntimeMemberId::StringTrim
                | rils_builtins::RuntimeMemberId::StringReplace),
            ) => {
                let receiver = match method.receiver.as_ref() {
                    Value::Reference(reference) => reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, span))?,
                    value => value.clone(),
                };
                let Value::String(value) = receiver else {
                    return Err(RuntimeError::new(
                        "string method receiver is not string",
                        span,
                    ));
                };
                let string_argument = |index: usize| match arguments.get(index) {
                    Some(Value::String(value)) => Ok(value.as_ref()),
                    Some(value) => Err(RuntimeError::new(
                        format!(
                            "string argument must be string, found {}",
                            value.type_name()
                        ),
                        span,
                    )),
                    None => Err(RuntimeError::new("missing string argument", span)),
                };
                match id {
                    rils_builtins::RuntimeMemberId::StringLen => Ok(Value::Usize(value.len())),
                    rils_builtins::RuntimeMemberId::StringIsEmpty => {
                        Ok(Value::Bool(value.is_empty()))
                    }
                    rils_builtins::RuntimeMemberId::StringContains => {
                        Ok(Value::Bool(value.contains(string_argument(0)?)))
                    }
                    rils_builtins::RuntimeMemberId::StringStartsWith => {
                        Ok(Value::Bool(value.starts_with(string_argument(0)?)))
                    }
                    rils_builtins::RuntimeMemberId::StringEndsWith => {
                        Ok(Value::Bool(value.ends_with(string_argument(0)?)))
                    }
                    rils_builtins::RuntimeMemberId::StringFind => Ok(Value::Option {
                        value: value
                            .find(string_argument(0)?)
                            .map(|offset| Rc::new(Value::Usize(offset))),
                        element_type: Some(Type::USIZE),
                    }),
                    rils_builtins::RuntimeMemberId::StringTrim => {
                        Ok(Value::String(Rc::from(value.trim())))
                    }
                    rils_builtins::RuntimeMemberId::StringReplace => Ok(Value::String(Rc::from(
                        value.replace(string_argument(0)?, string_argument(1)?),
                    ))),
                    _ => unreachable!(),
                }
            }
        }
    }
}
