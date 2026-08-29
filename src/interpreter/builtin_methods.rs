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
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::RangeIntoIter) => {
                Ok((*method.receiver).clone())
            }
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::IteratorIntoIter) => {
                Ok((*method.receiver).clone())
            }
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::Clone) => {
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
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::FormatterWriteStr) => {
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
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::FormatterWriteDerivedDebug) => {
                self.write_derived_debug(&method.receiver, &arguments[0], span)?;
                Ok(format_ok())
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::HashMapLen
                | rils_builtins::BuiltinId::HashMapIsEmpty
                | rils_builtins::BuiltinId::HashMapClear
                | rils_builtins::BuiltinId::HashMapContainsKey
                | rils_builtins::BuiltinId::HashMapInsert
                | rils_builtins::BuiltinId::HashMapGetCloned
                | rils_builtins::BuiltinId::HashMapRemove
                | rils_builtins::BuiltinId::HashMapKeysCloned
                | rils_builtins::BuiltinId::HashMapValuesCloned
                | rils_builtins::BuiltinId::HashMapIntoIter
                | rils_builtins::BuiltinId::HashSetLen
                | rils_builtins::BuiltinId::HashSetIsEmpty
                | rils_builtins::BuiltinId::HashSetClear
                | rils_builtins::BuiltinId::HashSetContains
                | rils_builtins::BuiltinId::HashSetInsert
                | rils_builtins::BuiltinId::HashSetRemove
                | rils_builtins::BuiltinId::HashSetIsSubset
                | rils_builtins::BuiltinId::HashSetIsSuperset
                | rils_builtins::BuiltinId::HashSetIsDisjoint
                | rils_builtins::BuiltinId::HashSetUnion
                | rils_builtins::BuiltinId::HashSetIntersection
                | rils_builtins::BuiltinId::HashSetDifference
                | rils_builtins::BuiltinId::HashSetSymmetricDifference
                | rils_builtins::BuiltinId::HashSetIntoIter),
            ) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::hash_collections::call(id, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::RangeNext) => {
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
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::SequenceLen
                | rils_builtins::BuiltinId::SequenceIsEmpty
                | rils_builtins::BuiltinId::SequenceContains),
            ) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::bytecode::runtime_builtins::call(id, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::VecPush) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::bytecode::runtime_builtins::call(rils_builtins::BuiltinId::VecPush, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::VecPop) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::bytecode::runtime_builtins::call(rils_builtins::BuiltinId::VecPop, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::VecClear | rils_builtins::BuiltinId::VecTruncate),
            ) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::bytecode::runtime_builtins::call(id, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::VecInsert
                | rils_builtins::BuiltinId::VecRemove
                | rils_builtins::BuiltinId::VecSwapRemove),
            ) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::bytecode::runtime_builtins::call(id, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::VecExtend) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::bytecode::runtime_builtins::call(
                    rils_builtins::BuiltinId::VecExtend,
                    &values,
                )
                .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::SequenceIntoIter) => {
                crate::bytecode::runtime_builtins::call(
                    rils_builtins::BuiltinId::SequenceIntoIter,
                    &[(*method.receiver).clone()],
                )
                .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(rils_builtins::BuiltinId::IteratorNext) => {
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
                id @ (rils_builtins::BuiltinId::IteratorCount
                | rils_builtins::BuiltinId::IteratorLast
                | rils_builtins::BuiltinId::IteratorNth
                | rils_builtins::BuiltinId::IteratorCollectVec
                | rils_builtins::BuiltinId::IteratorTake
                | rils_builtins::BuiltinId::IteratorSkip
                | rils_builtins::BuiltinId::IteratorRev),
            ) => {
                let receiver = match method.receiver.as_ref() {
                    Value::Reference(reference) => reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, span))?,
                    value => value.clone(),
                };
                if matches!(receiver, Value::SequenceIterator(_)) {
                    let mut values = Vec::with_capacity(arguments.len() + 1);
                    values.push((*method.receiver).clone());
                    values.extend_from_slice(arguments);
                    crate::bytecode::runtime_builtins::call(id, &values)
                        .map_err(|message| RuntimeError::new(message, span))
                } else {
                    self.call_iterator_default_method(id, method.receiver.as_ref(), arguments, span)
                }
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::IteratorMap
                | rils_builtins::BuiltinId::IteratorFilter
                | rils_builtins::BuiltinId::IteratorFilterMap
                | rils_builtins::BuiltinId::IteratorFold
                | rils_builtins::BuiltinId::IteratorForEach
                | rils_builtins::BuiltinId::IteratorAny
                | rils_builtins::BuiltinId::IteratorAll
                | rils_builtins::BuiltinId::IteratorFind
                | rils_builtins::BuiltinId::IteratorPosition
                | rils_builtins::BuiltinId::IteratorEnumerate),
            ) => self.call_iterator_default_method(id, method.receiver.as_ref(), arguments, span),
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::ResultIsOk
                | rils_builtins::BuiltinId::ResultIsErr
                | rils_builtins::BuiltinId::OptionIsSome
                | rils_builtins::BuiltinId::OptionIsNone),
            ) => crate::bytecode::runtime_builtins::call(id, &[(*method.receiver).clone()])
                .map_err(|message| RuntimeError::new(message, span)),
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::ResultUnwrap
                | rils_builtins::BuiltinId::ResultUnwrapOr
                | rils_builtins::BuiltinId::ResultExpect
                | rils_builtins::BuiltinId::ResultOk
                | rils_builtins::BuiltinId::ResultErr
                | rils_builtins::BuiltinId::ResultMap
                | rils_builtins::BuiltinId::ResultMapErr
                | rils_builtins::BuiltinId::ResultAndThen
                | rils_builtins::BuiltinId::ResultOrElse),
            )
            | BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::ResultUnwrapErr
                | rils_builtins::BuiltinId::ResultExpectErr),
            )
            | BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::OptionUnwrap
                | rils_builtins::BuiltinId::OptionUnwrapOr
                | rils_builtins::BuiltinId::OptionExpect
                | rils_builtins::BuiltinId::OptionTake
                | rils_builtins::BuiltinId::OptionOr
                | rils_builtins::BuiltinId::OptionXor
                | rils_builtins::BuiltinId::OptionMap
                | rils_builtins::BuiltinId::OptionAndThen
                | rils_builtins::BuiltinId::OptionOrElse),
            )
            | BuiltinMethod::Runtime(id @ rils_builtins::BuiltinId::OptionReplace) => {
                self.call_option_result_method(id, method.receiver.as_ref(), arguments, span)
            }
            BuiltinMethod::Runtime(
                id @ (rils_builtins::BuiltinId::StringLen
                | rils_builtins::BuiltinId::StringIsEmpty
                | rils_builtins::BuiltinId::StringContains
                | rils_builtins::BuiltinId::StringStartsWith
                | rils_builtins::BuiltinId::StringEndsWith
                | rils_builtins::BuiltinId::StringFind
                | rils_builtins::BuiltinId::StringTrim
                | rils_builtins::BuiltinId::StringReplace
                | rils_builtins::BuiltinId::StringTrimStart
                | rils_builtins::BuiltinId::StringTrimEnd
                | rils_builtins::BuiltinId::StringToLowercase
                | rils_builtins::BuiltinId::StringToUppercase
                | rils_builtins::BuiltinId::StringRepeat
                | rils_builtins::BuiltinId::StringRfind
                | rils_builtins::BuiltinId::StringStripPrefix
                | rils_builtins::BuiltinId::StringStripSuffix
                | rils_builtins::BuiltinId::StringChars
                | rils_builtins::BuiltinId::StringBytes
                | rils_builtins::BuiltinId::StringLines
                | rils_builtins::BuiltinId::StringSplit),
            ) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                crate::bytecode::runtime_builtins::call(id, &values)
                    .map_err(|message| RuntimeError::new(message, span))
            }
            BuiltinMethod::Runtime(id) => Err(RuntimeError::new(
                format!("unknown runtime member ID {:#x}", id.as_raw()),
                span,
            )),
        }
    }
}

fn format_ok() -> Value {
    Value::Result {
        value: Ok(Rc::new(Value::Unit)),
        ok_type: Some(Type::Unit),
        error_type: Some(Type::named("FormatError")),
    }
}
