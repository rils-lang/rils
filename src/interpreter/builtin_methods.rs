use super::*;

impl Interpreter {
    pub(super) fn call_builtin_method(
        &mut self,
        method: &BuiltinBoundMethod,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let arity = match method.method {
            BuiltinMethod::IntegerIntrinsic(id) | BuiltinMethod::FloatIntrinsic(id) => {
                rils_builtins::intrinsic(id).map_or(0, |item| item.signature.parameters.len())
            }
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
            BuiltinMethod::FloatIntrinsic(id) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::numeric::execute_intrinsic(id, None, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::RangeIntoIter) => {
                Ok((*method.receiver).clone())
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::IteratorIntoIter) => {
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
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::FormatterWriteStr) => {
                let buffer = super::formatting::formatter_buffer(&method.receiver, span)?;
                let Value::String(value) = &arguments[0] else {
                    return Err(RuntimeError::new(
                        "Formatter::write_str expects string",
                        span,
                    ));
                };
                buffer.write_str(value);
                Ok(format_ok())
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::FormatterWriteDerivedDebug) => {
                self.write_derived_debug(&method.receiver, &arguments[0], span)?;
                Ok(format_ok())
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::HashMapLen
                | rils_builtins::RuntimeMemberId::HashMapIsEmpty
                | rils_builtins::RuntimeMemberId::HashMapClear
                | rils_builtins::RuntimeMemberId::HashMapContainsKey
                | rils_builtins::RuntimeMemberId::HashMapInsert
                | rils_builtins::RuntimeMemberId::HashMapGetCloned
                | rils_builtins::RuntimeMemberId::HashMapRemove
                | rils_builtins::RuntimeMemberId::HashMapKeysCloned
                | rils_builtins::RuntimeMemberId::HashMapValuesCloned
                | rils_builtins::RuntimeMemberId::HashMapIntoIter
                | rils_builtins::RuntimeMemberId::HashSetLen
                | rils_builtins::RuntimeMemberId::HashSetIsEmpty
                | rils_builtins::RuntimeMemberId::HashSetClear
                | rils_builtins::RuntimeMemberId::HashSetContains
                | rils_builtins::RuntimeMemberId::HashSetInsert
                | rils_builtins::RuntimeMemberId::HashSetRemove
                | rils_builtins::RuntimeMemberId::HashSetIsSubset
                | rils_builtins::RuntimeMemberId::HashSetIsSuperset
                | rils_builtins::RuntimeMemberId::HashSetIsDisjoint
                | rils_builtins::RuntimeMemberId::HashSetUnion
                | rils_builtins::RuntimeMemberId::HashSetIntersection
                | rils_builtins::RuntimeMemberId::HashSetDifference
                | rils_builtins::RuntimeMemberId::HashSetSymmetricDifference
                | rils_builtins::RuntimeMemberId::HashSetIntoIter),
            ) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::hash_collections::call(
                    id.bytecode_import()
                        .expect("hash methods have core imports"),
                    &values,
                )
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
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::SequenceContains) => {
                let value = read_builtin_receiver(method.receiver.as_ref(), span)?;
                let sequence = match value {
                    Value::Array(sequence) | Value::Vec(sequence) => sequence,
                    _ => {
                        return Err(RuntimeError::new(
                            "contains receiver is not a collection",
                            span,
                        ));
                    }
                };
                let needle = read_builtin_receiver(&arguments[0], span)?;
                let contains = sequence
                    .elements
                    .borrow()
                    .iter()
                    .any(|slot| slot.value.as_ref() == Some(&needle));
                Ok(Value::Bool(contains))
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
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::VecInsert
                | rils_builtins::RuntimeMemberId::VecRemove
                | rils_builtins::RuntimeMemberId::VecSwapRemove),
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
                let Value::Usize(index) = arguments[0] else {
                    return Err(RuntimeError::new("Vec index must be usize", span));
                };
                let mut elements = sequence.elements.borrow_mut();
                if elements.iter().any(|slot| slot.references > 0) {
                    return Err(RuntimeError::new(
                        "cannot reorder a Vec while an element is referenced",
                        span,
                    ));
                }
                if id == rils_builtins::RuntimeMemberId::VecInsert {
                    if index > elements.len() {
                        return Err(RuntimeError::new(
                            format!("index {index} is out of bounds for insertion"),
                            span,
                        ));
                    }
                    let value = &arguments[1];
                    if value.contains_reference() {
                        return Err(RuntimeError::new("Vec cannot own local references", span));
                    }
                    let expected = sequence
                        .element_type
                        .borrow()
                        .clone()
                        .unwrap_or(Type::Unknown);
                    let actual = Type::of_value(value).unwrap_or(Type::Unknown);
                    let element_type = merge_types(&expected, &actual).ok_or_else(|| {
                        RuntimeError::new(
                            format!("Vec element type is `{expected}`, found `{actual}`"),
                            span,
                        )
                    })?;
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
                    return Err(RuntimeError::new(
                        format!("index {index} is out of bounds"),
                        span,
                    ));
                }
                let slot = if id == rils_builtins::RuntimeMemberId::VecRemove {
                    elements.remove(index)
                } else {
                    elements.swap_remove(index)
                };
                slot.value.ok_or_else(|| {
                    RuntimeError::new(format!("element at index {index} has been moved"), span)
                })
            }
            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::VecExtend) => {
                let Value::Reference(reference) = method.receiver.as_ref() else {
                    return Err(RuntimeError::new(
                        "Vec::extend requires a mutable binding",
                        span,
                    ));
                };
                if !reference.mutable {
                    return Err(RuntimeError::new("Vec::extend requires `&mut self`", span));
                }
                let Value::Vec(destination) = reference
                    .read()
                    .map_err(|message| RuntimeError::new(message, span))?
                else {
                    return Err(RuntimeError::new("extend receiver is not Vec", span));
                };
                let Value::Vec(source) = &arguments[0] else {
                    return Err(RuntimeError::new("Vec::extend source must be Vec", span));
                };
                if Rc::ptr_eq(&destination, source) {
                    return Err(RuntimeError::new("Vec cannot extend itself", span));
                }
                let mut source_elements = source.elements.borrow_mut();
                if source_elements.iter().any(|slot| slot.references > 0) {
                    return Err(RuntimeError::new(
                        "cannot move from a Vec while an element is referenced",
                        span,
                    ));
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
                let element_type =
                    merge_types(&destination_type, &source_type).ok_or_else(|| {
                        RuntimeError::new(
                            format!(
                                "Vec element type is `{destination_type}`, found `{source_type}`"
                            ),
                            span,
                        )
                    })?;
                *destination.element_type.borrow_mut() = Some(element_type);
                destination
                    .elements
                    .borrow_mut()
                    .extend(source_elements.drain(..));
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
                id @ (rils_builtins::RuntimeMemberId::IteratorCount
                | rils_builtins::RuntimeMemberId::IteratorLast
                | rils_builtins::RuntimeMemberId::IteratorNth
                | rils_builtins::RuntimeMemberId::IteratorCollectVec
                | rils_builtins::RuntimeMemberId::IteratorTake
                | rils_builtins::RuntimeMemberId::IteratorSkip
                | rils_builtins::RuntimeMemberId::IteratorRev
                | rils_builtins::RuntimeMemberId::IteratorMap
                | rils_builtins::RuntimeMemberId::IteratorFilter
                | rils_builtins::RuntimeMemberId::IteratorFilterMap
                | rils_builtins::RuntimeMemberId::IteratorFold
                | rils_builtins::RuntimeMemberId::IteratorForEach
                | rils_builtins::RuntimeMemberId::IteratorAny
                | rils_builtins::RuntimeMemberId::IteratorAll
                | rils_builtins::RuntimeMemberId::IteratorFind
                | rils_builtins::RuntimeMemberId::IteratorPosition
                | rils_builtins::RuntimeMemberId::IteratorEnumerate),
            ) => self.call_iterator_default_method(id, method.receiver.as_ref(), arguments, span),
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::ResultIsOk
                | rils_builtins::RuntimeMemberId::ResultIsErr
                | rils_builtins::RuntimeMemberId::ResultUnwrap
                | rils_builtins::RuntimeMemberId::ResultUnwrapOr
                | rils_builtins::RuntimeMemberId::ResultExpect
                | rils_builtins::RuntimeMemberId::ResultOk
                | rils_builtins::RuntimeMemberId::ResultErr
                | rils_builtins::RuntimeMemberId::ResultMap
                | rils_builtins::RuntimeMemberId::ResultMapErr
                | rils_builtins::RuntimeMemberId::ResultAndThen
                | rils_builtins::RuntimeMemberId::ResultOrElse),
            )
            | BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::ResultUnwrapErr
                | rils_builtins::RuntimeMemberId::ResultExpectErr),
            )
            | BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::OptionIsSome
                | rils_builtins::RuntimeMemberId::OptionIsNone
                | rils_builtins::RuntimeMemberId::OptionUnwrap
                | rils_builtins::RuntimeMemberId::OptionUnwrapOr
                | rils_builtins::RuntimeMemberId::OptionExpect
                | rils_builtins::RuntimeMemberId::OptionTake
                | rils_builtins::RuntimeMemberId::OptionOr
                | rils_builtins::RuntimeMemberId::OptionXor
                | rils_builtins::RuntimeMemberId::OptionMap
                | rils_builtins::RuntimeMemberId::OptionAndThen
                | rils_builtins::RuntimeMemberId::OptionOrElse),
            )
            | BuiltinMethod::Runtime(id @ rils_builtins::RuntimeMemberId::OptionReplace) => {
                self.call_option_result_method(id, method.receiver.as_ref(), arguments, span)
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::RuntimeMemberId::StringLen
                | rils_builtins::RuntimeMemberId::StringIsEmpty
                | rils_builtins::RuntimeMemberId::StringContains
                | rils_builtins::RuntimeMemberId::StringStartsWith
                | rils_builtins::RuntimeMemberId::StringEndsWith
                | rils_builtins::RuntimeMemberId::StringFind
                | rils_builtins::RuntimeMemberId::StringTrim
                | rils_builtins::RuntimeMemberId::StringReplace
                | rils_builtins::RuntimeMemberId::StringTrimStart
                | rils_builtins::RuntimeMemberId::StringTrimEnd
                | rils_builtins::RuntimeMemberId::StringToLowercase
                | rils_builtins::RuntimeMemberId::StringToUppercase
                | rils_builtins::RuntimeMemberId::StringRepeat
                | rils_builtins::RuntimeMemberId::StringRfind
                | rils_builtins::RuntimeMemberId::StringStripPrefix
                | rils_builtins::RuntimeMemberId::StringStripSuffix
                | rils_builtins::RuntimeMemberId::StringChars
                | rils_builtins::RuntimeMemberId::StringBytes
                | rils_builtins::RuntimeMemberId::StringLines
                | rils_builtins::RuntimeMemberId::StringSplit),
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
                    rils_builtins::RuntimeMemberId::StringTrimStart => {
                        Ok(Value::String(Rc::from(value.trim_start())))
                    }
                    rils_builtins::RuntimeMemberId::StringTrimEnd => {
                        Ok(Value::String(Rc::from(value.trim_end())))
                    }
                    rils_builtins::RuntimeMemberId::StringToLowercase => {
                        Ok(Value::String(Rc::from(value.to_lowercase())))
                    }
                    rils_builtins::RuntimeMemberId::StringToUppercase => {
                        Ok(Value::String(Rc::from(value.to_uppercase())))
                    }
                    rils_builtins::RuntimeMemberId::StringRepeat => {
                        let Some(Value::Usize(count)) = arguments.first() else {
                            return Err(RuntimeError::new(
                                "string repeat count must be usize",
                                span,
                            ));
                        };
                        Ok(Value::String(Rc::from(value.repeat(*count))))
                    }
                    rils_builtins::RuntimeMemberId::StringRfind => Ok(Value::Option {
                        value: value
                            .rfind(string_argument(0)?)
                            .map(|offset| Rc::new(Value::Usize(offset))),
                        element_type: Some(Type::USIZE),
                    }),
                    rils_builtins::RuntimeMemberId::StringStripPrefix
                    | rils_builtins::RuntimeMemberId::StringStripSuffix => {
                        let pattern = string_argument(0)?;
                        let stripped = if id == rils_builtins::RuntimeMemberId::StringStripPrefix {
                            value.strip_prefix(pattern)
                        } else {
                            value.strip_suffix(pattern)
                        };
                        Ok(Value::Option {
                            value: stripped.map(|text| Rc::new(Value::String(Rc::from(text)))),
                            element_type: Some(Type::String),
                        })
                    }
                    rils_builtins::RuntimeMemberId::StringChars => Ok(string_iterator(
                        value.chars().map(Value::Char).collect(),
                        Type::Char,
                    )),
                    rils_builtins::RuntimeMemberId::StringBytes => Ok(string_iterator(
                        value.bytes().map(Value::U8).collect(),
                        Type::Integer(crate::IntegerType::U8),
                    )),
                    rils_builtins::RuntimeMemberId::StringLines => Ok(string_iterator(
                        value
                            .lines()
                            .map(|line| Value::String(Rc::from(line)))
                            .collect(),
                        Type::String,
                    )),
                    rils_builtins::RuntimeMemberId::StringSplit => Ok(string_iterator(
                        value
                            .split(string_argument(0)?)
                            .map(|part| Value::String(Rc::from(part)))
                            .collect(),
                        Type::String,
                    )),
                    rils_builtins::RuntimeMemberId::StringReplace => Ok(Value::String(Rc::from(
                        value.replace(string_argument(0)?, string_argument(1)?),
                    ))),
                    _ => unreachable!(),
                }
            }
        }
    }
}

fn read_builtin_receiver(value: &Value, span: Span) -> Result<Value, RuntimeError> {
    match value {
        Value::Reference(reference) => reference
            .read()
            .map_err(|message| RuntimeError::new(message, span)),
        value => Ok(value.clone()),
    }
}

fn format_ok() -> Value {
    Value::Result {
        value: Ok(Rc::new(Value::Unit)),
        ok_type: Some(Type::Unit),
        error_type: Some(Type::named("FormatError")),
    }
}

fn string_iterator(items: std::collections::VecDeque<Value>, element_type: Type) -> Value {
    Value::SequenceIterator(Rc::new(SequenceIteratorValue {
        items: RefCell::new(items),
        element_type,
    }))
}
