use super::*;

pub(super) fn call(id: rils_builtins::BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    use rils_builtins::BuiltinId;

    match id {
        BuiltinId::Clone => match &arguments[0] {
            Value::Reference(reference) => reference.read()?.clone_owned(),
            value => Err(format!(
                "`clone` expects a reference, found {}; use `clone(&value)`",
                value.type_name()
            )),
        },
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
        BuiltinId::SequenceLen | BuiltinId::StringLen => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("len receiver must be a reference".into());
            };
            let value = reference.read()?;
            let length = match value {
                Value::Array(sequence) | Value::Vec(sequence) => sequence.elements.borrow().len(),
                Value::String(value) => value.len(),
                value => {
                    return Err(format!(
                        "len receiver is not a collection: {}",
                        value.type_name()
                    ));
                }
            };
            Ok(Value::Usize(length))
        }
        BuiltinId::SequenceIsEmpty | BuiltinId::StringIsEmpty => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("is_empty receiver must be a reference".into());
            };
            let value = reference.read()?;
            let empty = match value {
                Value::Array(sequence) | Value::Vec(sequence) => {
                    sequence.elements.borrow().is_empty()
                }
                Value::String(value) => value.is_empty(),
                value => return Err(format!("{} has no is_empty method", value.type_name())),
            };
            Ok(Value::Bool(empty))
        }
        BuiltinId::SequenceContains | BuiltinId::StringContains => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("contains receiver must be a reference".into());
            };
            match reference.read()? {
                Value::Array(sequence) | Value::Vec(sequence) => {
                    let needle = import_receiver(&arguments[1])?;
                    let contains = sequence
                        .elements
                        .borrow()
                        .iter()
                        .any(|slot| slot.value.as_ref() == Some(&needle));
                    Ok(Value::Bool(contains))
                }
                Value::String(value) => {
                    let Value::String(needle) = &arguments[1] else {
                        return Err("string contains argument must be string".into());
                    };
                    Ok(Value::Bool(value.contains(needle.as_ref())))
                }
                _ => Err("contains receiver is not a collection".into()),
            }
        }
        BuiltinId::VecPush => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::push requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::push requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("push receiver is not Vec".into());
            };
            let value = &arguments[1];
            if value.contains_reference() {
                return Err("Vec cannot own local references".into());
            }
            let current = sequence
                .elements
                .borrow()
                .first()
                .map(|slot| slot.type_annotation.clone())
                .or_else(|| sequence.element_type.borrow().clone())
                .unwrap_or(Type::Unknown);
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            let element_type = crate::types::merge_types(&current, &actual)
                .ok_or_else(|| format!("Vec element type is `{current}`, found `{actual}`"))?;
            *sequence.element_type.borrow_mut() = Some(element_type.clone());
            sequence.elements.borrow_mut().push(FieldSlot {
                value: Some(value.clone()),
                type_annotation: element_type,
                references: 0,
            });
            Ok(Value::Unit)
        }
        BuiltinId::VecPop => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::pop requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::pop requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("pop receiver is not Vec".into());
            };
            let element_type = sequence
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            let value = {
                let mut elements = sequence.elements.borrow_mut();
                if elements.last().is_some_and(|slot| slot.references > 0) {
                    return Err("cannot pop a referenced Vec element".into());
                }
                elements.pop().and_then(|slot| slot.value).map(Rc::new)
            };
            Ok(Value::Option {
                value,
                element_type: Some(element_type),
            })
        }
        BuiltinId::VecClear | BuiltinId::VecTruncate => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec mutation requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec mutation requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("receiver is not Vec".into());
            };
            let length = if id == BuiltinId::VecClear {
                0
            } else {
                let Value::Usize(length) = arguments[1] else {
                    return Err("Vec::truncate length must be usize".into());
                };
                length
            };
            let mut elements = sequence.elements.borrow_mut();
            if elements
                .get(length..)
                .is_some_and(|tail| tail.iter().any(|slot| slot.references > 0))
            {
                return Err("cannot remove a referenced Vec element".into());
            }
            elements.truncate(length);
            Ok(Value::Unit)
        }
        BuiltinId::VecInsert | BuiltinId::VecRemove | BuiltinId::VecSwapRemove => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec mutation requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec mutation requires `&mut self`".into());
            }
            let Value::Vec(sequence) = reference.read()? else {
                return Err("receiver is not Vec".into());
            };
            let Value::Usize(index) = arguments[1] else {
                return Err("Vec index must be usize".into());
            };
            let mut elements = sequence.elements.borrow_mut();
            if elements.iter().any(|slot| slot.references > 0) {
                return Err("cannot reorder a Vec while an element is referenced".into());
            }
            if id == BuiltinId::VecInsert {
                if index > elements.len() {
                    return Err(format!("index {index} is out of bounds for insertion"));
                }
                let value = &arguments[2];
                if value.contains_reference() {
                    return Err("Vec cannot own local references".into());
                }
                let expected = sequence
                    .element_type
                    .borrow()
                    .clone()
                    .unwrap_or(Type::Unknown);
                let actual = Type::of_value(value).unwrap_or(Type::Unknown);
                let element_type = crate::types::merge_types(&expected, &actual)
                    .ok_or_else(|| format!("Vec element type is `{expected}`, found `{actual}`"))?;
                *sequence.element_type.borrow_mut() = Some(element_type.clone());
                elements.insert(
                    index,
                    FieldSlot {
                        value: Some(value.clone()),
                        type_annotation: element_type,
                        references: 0,
                    },
                );
                return Ok(Value::Unit);
            }
            if index >= elements.len() {
                return Err(format!("index {index} is out of bounds"));
            }
            let slot = if id == BuiltinId::VecRemove {
                elements.remove(index)
            } else {
                elements.swap_remove(index)
            };
            slot.value
                .ok_or_else(|| format!("element at index {index} has been moved"))
        }
        BuiltinId::VecExtend => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("Vec::extend requires a mutable binding".into());
            };
            if !reference.mutable {
                return Err("Vec::extend requires `&mut self`".into());
            }
            let Value::Vec(destination) = reference.read()? else {
                return Err("extend receiver is not Vec".into());
            };
            let Value::Vec(source) = &arguments[1] else {
                return Err("Vec::extend source must be Vec".into());
            };
            if Rc::ptr_eq(&destination, source) {
                return Err("Vec cannot extend itself".into());
            }
            let mut source_elements = source.elements.borrow_mut();
            if source_elements.iter().any(|slot| slot.references > 0) {
                return Err("cannot move from a Vec while an element is referenced".into());
            }
            let destination_type = destination
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            let source_type = source
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown);
            let element_type = crate::types::merge_types(&destination_type, &source_type)
                .ok_or_else(|| {
                    format!("Vec element type is `{destination_type}`, found `{source_type}`")
                })?;
            *destination.element_type.borrow_mut() = Some(element_type);
            destination
                .elements
                .borrow_mut()
                .extend(source_elements.drain(..));
            Ok(Value::Unit)
        }
        BuiltinId::HashMapLen
        | BuiltinId::HashMapIsEmpty
        | BuiltinId::HashMapClear
        | BuiltinId::HashMapContainsKey
        | BuiltinId::HashMapInsert
        | BuiltinId::HashMapGetCloned
        | BuiltinId::HashMapRemove
        | BuiltinId::HashMapKeysCloned
        | BuiltinId::HashMapValuesCloned
        | BuiltinId::HashMapIntoIter
        | BuiltinId::HashSetLen
        | BuiltinId::HashSetIsEmpty
        | BuiltinId::HashSetClear
        | BuiltinId::HashSetContains
        | BuiltinId::HashSetInsert
        | BuiltinId::HashSetRemove
        | BuiltinId::HashSetIsSubset
        | BuiltinId::HashSetIsSuperset
        | BuiltinId::HashSetIsDisjoint
        | BuiltinId::HashSetUnion
        | BuiltinId::HashSetIntersection
        | BuiltinId::HashSetDifference
        | BuiltinId::HashSetSymmetricDifference
        | BuiltinId::HashSetIntoIter => crate::hash_collections::call(id, arguments),
        BuiltinId::IteratorNext
        | BuiltinId::IteratorCount
        | BuiltinId::IteratorLast
        | BuiltinId::IteratorNth
        | BuiltinId::IteratorCollectVec
        | BuiltinId::IteratorTake
        | BuiltinId::IteratorSkip
        | BuiltinId::IteratorRev => {
            let Value::SequenceIterator(iterator) = import_receiver(&arguments[0])? else {
                return Err("iterator method receiver is not a built-in iterator".into());
            };
            let element_type = iterator.element_type.clone();
            let mut items = iterator.items.borrow_mut();
            let count = || match arguments.get(1) {
                Some(Value::Usize(value)) => Ok(*value),
                Some(value) => Err(format!(
                    "iterator count must be usize, found {}",
                    value.type_name()
                )),
                None => Err("missing iterator count".into()),
            };
            match id {
                BuiltinId::IteratorNext => Ok(Value::Option {
                    value: items.pop_front().map(Rc::new),
                    element_type: Some(element_type),
                }),
                BuiltinId::IteratorCount => {
                    let count = items.len();
                    items.clear();
                    Ok(Value::Usize(count))
                }
                BuiltinId::IteratorLast => {
                    let value = items.pop_back().map(Rc::new);
                    items.clear();
                    Ok(Value::Option {
                        value,
                        element_type: Some(element_type),
                    })
                }
                BuiltinId::IteratorNth => {
                    let count = count()?;
                    let skipped = count.min(items.len());
                    items.drain(..skipped);
                    let value = (skipped == count)
                        .then(|| items.pop_front())
                        .flatten()
                        .map(Rc::new);
                    Ok(Value::Option {
                        value,
                        element_type: Some(element_type),
                    })
                }
                BuiltinId::IteratorCollectVec => {
                    let elements = items
                        .drain(..)
                        .map(|value| FieldSlot {
                            value: Some(value),
                            type_annotation: element_type.clone(),
                            references: 0,
                        })
                        .collect();
                    Ok(Value::Vec(Rc::new(SequenceValue {
                        elements: RefCell::new(elements),
                        element_type: RefCell::new(Some(element_type)),
                    })))
                }
                BuiltinId::IteratorTake | BuiltinId::IteratorSkip => {
                    let count = count()?.min(items.len());
                    let selected = if id == BuiltinId::IteratorTake {
                        let selected = items.drain(..count).collect();
                        items.clear();
                        selected
                    } else {
                        items.drain(..count);
                        items.drain(..).collect()
                    };
                    Ok(sequence_iterator_value(selected, element_type))
                }
                BuiltinId::IteratorRev => Ok(sequence_iterator_value(
                    items.drain(..).rev().collect(),
                    element_type,
                )),
                _ => unreachable!("iterator built-in was matched above"),
            }
        }
        BuiltinId::StringStartsWith
        | BuiltinId::StringEndsWith
        | BuiltinId::StringFind
        | BuiltinId::StringTrim
        | BuiltinId::StringTrimStart
        | BuiltinId::StringTrimEnd
        | BuiltinId::StringToLowercase
        | BuiltinId::StringToUppercase
        | BuiltinId::StringRepeat
        | BuiltinId::StringRfind
        | BuiltinId::StringStripPrefix
        | BuiltinId::StringStripSuffix
        | BuiltinId::StringChars
        | BuiltinId::StringBytes
        | BuiltinId::StringLines
        | BuiltinId::StringSplit => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("string method receiver must be a reference".into());
            };
            let Value::String(value) = reference.read()? else {
                return Err("string method receiver is not string".into());
            };
            let argument = |index: usize| match arguments.get(index) {
                Some(Value::String(value)) => Ok(value.as_ref()),
                Some(value) => Err(format!(
                    "string argument must be string, found {}",
                    value.type_name()
                )),
                None => Err("missing string argument".into()),
            };
            match id {
                BuiltinId::StringStartsWith => Ok(Value::Bool(value.starts_with(argument(1)?))),
                BuiltinId::StringEndsWith => Ok(Value::Bool(value.ends_with(argument(1)?))),
                BuiltinId::StringFind => Ok(Value::Option {
                    value: value
                        .find(argument(1)?)
                        .map(|offset| Rc::new(Value::Usize(offset))),
                    element_type: Some(Type::USIZE),
                }),
                BuiltinId::StringTrim => Ok(Value::String(Rc::from(value.trim()))),
                BuiltinId::StringTrimStart => Ok(Value::String(Rc::from(value.trim_start()))),
                BuiltinId::StringTrimEnd => Ok(Value::String(Rc::from(value.trim_end()))),
                BuiltinId::StringToLowercase => Ok(Value::String(Rc::from(value.to_lowercase()))),
                BuiltinId::StringToUppercase => Ok(Value::String(Rc::from(value.to_uppercase()))),
                BuiltinId::StringRepeat => {
                    let Some(Value::Usize(count)) = arguments.get(1) else {
                        return Err("string repeat count must be usize".into());
                    };
                    Ok(Value::String(Rc::from(value.repeat(*count))))
                }
                BuiltinId::StringRfind => Ok(Value::Option {
                    value: value
                        .rfind(argument(1)?)
                        .map(|offset| Rc::new(Value::Usize(offset))),
                    element_type: Some(Type::USIZE),
                }),
                BuiltinId::StringStripPrefix | BuiltinId::StringStripSuffix => {
                    let pattern = argument(1)?;
                    let stripped = if id == BuiltinId::StringStripPrefix {
                        value.strip_prefix(pattern)
                    } else {
                        value.strip_suffix(pattern)
                    };
                    Ok(Value::Option {
                        value: stripped.map(|text| Rc::new(Value::String(Rc::from(text)))),
                        element_type: Some(Type::String),
                    })
                }
                BuiltinId::StringChars => Ok(sequence_iterator_value(
                    value.chars().map(Value::Char).collect(),
                    Type::Char,
                )),
                BuiltinId::StringBytes => Ok(sequence_iterator_value(
                    value.bytes().map(Value::U8).collect(),
                    Type::Integer(IntegerType::U8),
                )),
                BuiltinId::StringLines => Ok(sequence_iterator_value(
                    value
                        .lines()
                        .map(|line| Value::String(Rc::from(line)))
                        .collect(),
                    Type::String,
                )),
                BuiltinId::StringSplit => Ok(sequence_iterator_value(
                    value
                        .split(argument(1)?)
                        .map(|part| Value::String(Rc::from(part)))
                        .collect(),
                    Type::String,
                )),
                _ => unreachable!("string built-in was matched above"),
            }
        }
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
                .map_err(|error| assign_error(error, Span::default()).message)?;
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
        BuiltinId::OptionReplace | BuiltinId::StringReplace => {
            let Value::Reference(reference) = &arguments[0] else {
                return Err("replace receiver must be a reference".into());
            };
            let receiver = reference.read()?;
            if let Value::String(value) = receiver {
                let (Value::String(pattern), Value::String(replacement)) =
                    (&arguments[1], &arguments[2])
                else {
                    return Err("string replace arguments must be string".into());
                };
                return Ok(Value::String(Rc::from(
                    value.replace(pattern.as_ref(), replacement.as_ref()),
                )));
            }
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
                .map_err(|error| assign_error(error, Span::default()).message)?;
            Ok(Value::Option {
                value: previous,
                element_type: Some(resolved),
            })
        }
        _ => Err(format!(
            "runtime built-in `{id:?}` has no direct implementation"
        )),
    }
}

fn sequence_iterator_value(items: VecDeque<Value>, element_type: Type) -> Value {
    Value::SequenceIterator(Rc::new(SequenceIteratorValue {
        items: RefCell::new(items),
        element_type,
    }))
}

fn import_receiver(value: &Value) -> Result<Value, String> {
    match value {
        Value::Reference(reference) => reference.read(),
        value => Ok(value.clone()),
    }
}
