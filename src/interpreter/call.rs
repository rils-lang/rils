use super::execution::anchored_environment;
use super::*;
use crate::environment::StorageSlot;

impl Interpreter {
    pub(super) fn call(
        &mut self,
        callee: Value,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match callee {
            Value::BuiltinFunction(function) => match function {
                BuiltinFunction::VecNew => {
                    check_arity("Vec::new", 0, 0, arguments.len(), span)?;
                    Ok(Value::Vec(Rc::new(SequenceValue {
                        elements: RefCell::new(Vec::new()),
                        element_type: RefCell::new(Some(Type::Unknown)),
                    })))
                }
                BuiltinFunction::VecFrom => {
                    check_arity("Vec::from", 1, 1, arguments.len(), span)?;
                    let Value::Array(array) = &arguments[0] else {
                        return Err(RuntimeError::new("Vec::from expects an array", span));
                    };
                    if array
                        .elements
                        .borrow()
                        .iter()
                        .any(|slot| slot.references > 0)
                    {
                        return Err(RuntimeError::new(
                            "cannot move an array into Vec while an element is referenced",
                            span,
                        ));
                    }
                    let elements = array.elements.borrow_mut().drain(..).collect();
                    Ok(Value::Vec(Rc::new(SequenceValue {
                        elements: RefCell::new(elements),
                        element_type: RefCell::new(array.element_type.borrow().clone()),
                    })))
                }
                BuiltinFunction::IntegerIntrinsic { id, target } => {
                    check_arity("integer intrinsic", 1, 1, arguments.len(), span)?;
                    crate::numeric::execute_integer_intrinsic(id, Some(target), arguments)
                        .map_err(|message| RuntimeError::new(message, span))
                }
            },
            Value::NativeFunction(function) => {
                check_arity(
                    function.name,
                    function.min_arity,
                    function.max_arity,
                    arguments.len(),
                    span,
                )?;
                validate_native_arguments(function.signature.as_ref(), arguments, span)?;
                let value = (function.function)(arguments)
                    .map_err(|message| RuntimeError::new(message, span))?;
                validate_native_return(function.signature.as_ref(), value, span, function.name)
            }
            Value::HostFunction(function) => {
                check_arity(
                    &function.name,
                    function.min_arity,
                    function.max_arity,
                    arguments.len(),
                    span,
                )?;
                validate_native_arguments(function.signature.as_ref(), arguments, span)?;
                let value = (function.function)(arguments)
                    .map_err(|message| RuntimeError::new(message, span))?;
                validate_native_return(function.signature.as_ref(), value, span, &function.name)
            }
            Value::HostBoundMethod(method) => {
                check_arity(
                    &method.function.name,
                    method.function.min_arity,
                    method.function.max_arity,
                    arguments.len(),
                    span,
                )?;
                let mut method_arguments = Vec::with_capacity(arguments.len() + 1);
                method_arguments.push((*method.receiver).clone());
                method_arguments.extend_from_slice(arguments);
                validate_native_arguments(method.function.signature.as_ref(), arguments, span)?;
                let value = (method.function.function)(&method_arguments)
                    .map_err(|message| RuntimeError::new(message, span))?;
                validate_native_return(
                    method.function.signature.as_ref(),
                    value,
                    span,
                    &method.function.name,
                )
            }
            Value::Function(function) => {
                check_arity(
                    &function.name,
                    function.parameters.len(),
                    function.parameters.len(),
                    arguments.len(),
                    span,
                )?;
                let environment = Environment::child(function.closure.clone());
                let mut substitutions: HashMap<String, Type> = function
                    .generic_parameters
                    .iter()
                    .map(|parameter| (parameter.name.clone(), Type::Unknown))
                    .collect();
                for (parameter, argument) in function.parameters.iter().zip(arguments) {
                    if let Some(parameter_type) = &parameter.type_annotation {
                        infer_type_from_value(parameter_type, argument, &mut substitutions)
                            .map_err(|message| RuntimeError::new(message, span))?;
                    }
                }
                validate_generic_bounds(
                    &function.generic_parameters,
                    &substitutions,
                    &function.closure,
                    span,
                )?;
                for (parameter, argument) in function.parameters.iter().zip(arguments) {
                    let expected = parameter
                        .type_annotation
                        .as_ref()
                        .map(|value| {
                            expand_type_aliases(
                                &value.substitute(&substitutions),
                                &function.closure,
                                span,
                            )
                        })
                        .transpose()?;
                    let argument = apply_type(expected.as_ref(), argument, span, &parameter.name)?;
                    environment.borrow_mut().define(
                        parameter.name.clone(),
                        argument,
                        parameter.mutable,
                        parameter.type_annotation.clone(),
                    );
                }
                self.function_depth += 1;
                let result = self.execute_statements(&function.body.statements, environment);
                self.function_depth -= 1;
                let result = match result {
                    Err(error) if error.message == TRY_RETURN_SIGNAL => {
                        let value = self.pending_return.take().ok_or_else(|| {
                            RuntimeError::new("missing Result value for `?` return", span)
                        })?;
                        Ok(Flow::Return(value))
                    }
                    result => result,
                };
                match result {
                    Ok(Flow::Value(value) | Flow::Return(value)) => {
                        if value.contains_reference() {
                            return Err(RuntimeError::new(
                                "references cannot be returned from functions",
                                span,
                            ));
                        }
                        if let Some(return_type) = &function.return_type {
                            let return_type = expand_type_aliases(
                                &return_type.substitute(&substitutions),
                                &function.closure,
                                span,
                            )?;
                            infer_type_from_value(&return_type, &value, &mut substitutions)
                                .map_err(|message| RuntimeError::new(message, span))?;
                        }
                        validate_generic_bounds(
                            &function.generic_parameters,
                            &substitutions,
                            &function.closure,
                            span,
                        )?;
                        let expected = function
                            .return_type
                            .as_ref()
                            .map(|value| {
                                expand_type_aliases(
                                    &value.substitute(&substitutions),
                                    &function.closure,
                                    span,
                                )
                            })
                            .transpose()?;
                        let value = apply_type(
                            expected.as_ref(),
                            &value,
                            span,
                            &format!("return value of `{}`", function.name),
                        )?;
                        Ok(value)
                    }
                    Ok(Flow::Break(_) | Flow::Continue) => Err(RuntimeError::new(
                        "loop control cannot escape a function",
                        span,
                    )),
                    Err(mut error) => {
                        error.stack.push(function.name.clone());
                        Err(error)
                    }
                }
            }
            Value::BoundMethod(method) => {
                let mut method_arguments = Vec::with_capacity(arguments.len() + 1);
                method_arguments.push((*method.receiver).clone());
                method_arguments.extend_from_slice(arguments);
                self.call(
                    Value::Function(method.function.clone()),
                    &method_arguments,
                    span,
                )
            }
            Value::BuiltinBoundMethod(method) => {
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
                            return Err(RuntimeError::new(
                                "Range::next requires `&mut self`",
                                span,
                            ));
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
                        reference.write(Value::Range(range)).map_err(|error| {
                            super::evaluation::assignment_error(error, "Range", span)
                        })?;
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
                                return Err(RuntimeError::new(
                                    "len receiver is not a collection",
                                    span,
                                ));
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
                            return Err(RuntimeError::new(
                                "Vec mutation requires `&mut self`",
                                span,
                            ));
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
                            return Err(RuntimeError::new(
                                "next receiver is not an iterator",
                                span,
                            ));
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
                                let message = if id == rils_builtins::RuntimeMemberId::ResultExpect
                                {
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
                            (rils_builtins::RuntimeMemberId::ResultOk, Err(_)) => {
                                (None, ok_type.clone())
                            }
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
                                        _ => RuntimeError::new(
                                            "Option::expect message must be string",
                                            span,
                                        ),
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
                            return Err(RuntimeError::new(
                                "Option::take requires `&mut self`",
                                span,
                            ));
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
                            .map_err(|error| {
                                super::evaluation::assignment_error(error, "Option", span)
                            })?;
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
                            rils_builtins::RuntimeMemberId::StringLen => {
                                Ok(Value::Usize(value.len()))
                            }
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
                            rils_builtins::RuntimeMemberId::StringReplace => Ok(Value::String(
                                Rc::from(value.replace(string_argument(0)?, string_argument(1)?)),
                            )),
                            _ => unreachable!(),
                        }
                    }
                }
            }
            Value::TraitMethodSelector(selector) => {
                check_arity(
                    &format!("{}::{}", selector.trait_name, selector.method_name),
                    1,
                    usize::MAX,
                    arguments.len(),
                    span,
                )?;
                let receiver = arguments.first().expect("arity checked");
                let actual = Type::of_value(receiver).ok_or_else(|| {
                    RuntimeError::new("trait method receiver has no runtime type", span)
                })?;
                let actual_target = match &actual {
                    Type::Reference { inner, .. } => inner.as_ref(),
                    actual => actual,
                };
                if let Some(expected) = &selector.target {
                    let expected = expand_type_aliases(expected, &selector.environment, span)?;
                    if !matches!(expected, Type::Variable(_) | Type::Unknown)
                        && merge_types(&expected, actual_target).is_none()
                    {
                        return Err(RuntimeError::new(
                            format!(
                                "qualified trait method expects `{expected}`, found `{actual_target}`"
                            ),
                            span,
                        ));
                    }
                }
                let has_explicit_clone = if selector.trait_name == "Clone"
                    && selector.method_name == "clone"
                {
                    match actual_target {
                        Type::Named { name, .. } => match selector.environment.borrow().get(name) {
                            Some(Value::StructType(definition)) => definition
                                .trait_methods
                                .borrow()
                                .get("Clone")
                                .is_some_and(|methods| methods.contains_key("clone")),
                            Some(Value::EnumType(definition)) => definition
                                .trait_methods
                                .borrow()
                                .get("Clone")
                                .is_some_and(|methods| methods.contains_key("clone")),
                            _ => false,
                        },
                        _ => false,
                    }
                } else {
                    false
                };
                if selector.trait_name == "Clone"
                    && selector.method_name == "clone"
                    && !has_explicit_clone
                    && type_implements_trait(actual_target, "Clone", &selector.environment)
                {
                    return self.call(
                        Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                            receiver: Rc::new(receiver.clone()),
                            method: BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::Clone),
                        })),
                        &arguments[1..],
                        span,
                    );
                }
                if matches!(actual_target, Type::Named { name, arguments } if name == "Range" && arguments.is_empty())
                {
                    let method = match (selector.trait_name.as_str(), selector.method_name.as_str())
                    {
                        ("Iterator", "next") => {
                            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::RangeNext)
                        }
                        ("IntoIterator", "into_iter") => {
                            BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::RangeIntoIter)
                        }
                        _ => {
                            return Err(RuntimeError::new(
                                format!(
                                    "trait `{}` has no method `{}` for Range",
                                    selector.trait_name, selector.method_name
                                ),
                                span,
                            ));
                        }
                    };
                    return self.call(
                        Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                            receiver: Rc::new(receiver.clone()),
                            method,
                        })),
                        &arguments[1..],
                        span,
                    );
                }
                let sequence_method = match (
                    selector.trait_name.as_str(),
                    selector.method_name.as_str(),
                    actual_target,
                ) {
                    ("IntoIterator", "into_iter", Type::Array { .. }) => Some(
                        BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::SequenceIntoIter),
                    ),
                    ("IntoIterator", "into_iter", Type::Named { name, arguments })
                        if name == "Vec" && arguments.len() == 1 =>
                    {
                        Some(BuiltinMethod::Runtime(
                            rils_builtins::RuntimeMemberId::SequenceIntoIter,
                        ))
                    }
                    ("Iterator", "next", Type::Named { name, arguments })
                        if name == "SequenceIterator" && arguments.len() == 1 =>
                    {
                        Some(BuiltinMethod::Runtime(
                            rils_builtins::RuntimeMemberId::IteratorNext,
                        ))
                    }
                    _ => None,
                };
                if let Some(method) = sequence_method {
                    return self.call(
                        Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                            receiver: Rc::new(receiver.clone()),
                            method,
                        })),
                        &arguments[1..],
                        span,
                    );
                }
                let Type::Named { name, .. } = actual_target else {
                    return Err(RuntimeError::new(
                        format!("type `{actual_target}` cannot implement traits"),
                        span,
                    ));
                };
                let target = selector.environment.borrow().get(name).ok_or_else(|| {
                    RuntimeError::new(format!("unknown trait method target `{name}`"), span)
                })?;
                let function = match target {
                    Value::StructType(definition) => definition
                        .trait_methods
                        .borrow()
                        .get(&selector.trait_name)
                        .and_then(|methods| methods.get(&selector.method_name))
                        .cloned(),
                    Value::EnumType(definition) => definition
                        .trait_methods
                        .borrow()
                        .get(&selector.trait_name)
                        .and_then(|methods| methods.get(&selector.method_name))
                        .cloned(),
                    _ => None,
                }
                .ok_or_else(|| {
                    RuntimeError::new(
                        format!(
                            "type `{name}` does not implement `{}::{}`",
                            selector.trait_name, selector.method_name
                        ),
                        span,
                    )
                })?;
                self.call(Value::Function(function), arguments, span)
            }
            Value::VariantConstructor(constructor) => {
                if arguments.iter().any(Value::contains_reference) {
                    return Err(RuntimeError::new(
                        "references cannot be stored in enum fields",
                        span,
                    ));
                }
                let variant = constructor
                    .type_definition
                    .variants
                    .iter()
                    .find(|variant| enum_variant_name(variant) == constructor.variant)
                    .expect("constructor refers to declared variant");
                let EnumVariant::Tuple { fields, .. } = variant else {
                    return Err(RuntimeError::new(
                        format!(
                            "{}::{} must be constructed with named fields",
                            constructor.type_definition.name, constructor.variant
                        ),
                        span,
                    ));
                };
                check_arity(
                    &format!(
                        "{}::{}",
                        constructor.type_definition.name, constructor.variant
                    ),
                    fields.len(),
                    fields.len(),
                    arguments.len(),
                    span,
                )?;
                let mut substitutions =
                    generic_substitutions(&constructor.type_definition.generic_parameters);
                for (field_type, value) in fields.iter().zip(arguments) {
                    infer_type_from_value(field_type, value, &mut substitutions)
                        .map_err(|message| RuntimeError::new(message, span))?;
                }
                validate_generic_bounds(
                    &constructor.type_definition.generic_parameters,
                    &substitutions,
                    &constructor.environment,
                    span,
                )?;
                let values = fields
                    .iter()
                    .zip(arguments)
                    .enumerate()
                    .map(|(index, (field_type, value))| {
                        let expected = field_type.substitute(&substitutions);
                        apply_type(
                            Some(&expected),
                            value,
                            span,
                            &format!("variant field {index}"),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Enum(Rc::new(EnumInstance {
                    type_definition: constructor.type_definition.clone(),
                    variant: constructor.variant.clone(),
                    payload: EnumPayload::Tuple(values),
                    type_arguments: generic_arguments(
                        &constructor.type_definition.generic_parameters,
                        &substitutions,
                    ),
                })))
            }
            value => Err(RuntimeError::new(
                format!("{} is not callable", value.type_name()),
                span,
            )),
        }
    }

    pub(super) fn resolve_path(
        &self,
        segments: &[String],
        environment: &EnvironmentRef,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let (environment, segments) = anchored_environment(segments, environment, span)?;
        let Some(first) = segments.first() else {
            return Err(RuntimeError::new("empty path", span));
        };
        let mut base = environment
            .borrow()
            .get(first)
            .ok_or_else(|| RuntimeError::new(format!("undefined name `{first}`"), span))?;
        let mut owner_environment = environment.clone();
        let mut index = 1;
        loop {
            let Value::Module(module) = base.clone() else {
                break;
            };
            if index == segments.len() {
                return Ok(base);
            }
            let segment = &segments[index];
            if !module.public.borrow().contains(segment) {
                return Err(RuntimeError::new(
                    format!("module `{}` has no public member `{segment}`", module.name),
                    span,
                ));
            }
            owner_environment = module.members.clone();
            let next = module.members.borrow().get(segment).ok_or_else(|| {
                RuntimeError::new(
                    format!("module `{}` is missing member `{segment}`", module.name),
                    span,
                )
            })?;
            base = next;
            index += 1;
        }
        if index == segments.len() {
            return Ok(base);
        }
        if index + 1 != segments.len() {
            return Err(RuntimeError::new(
                format!("unsupported path `{}`", segments.join("::")),
                span,
            ));
        }
        let member = &segments[index];
        match base {
            Value::BuiltinType(BuiltinType::Vec) => match member.as_str() {
                "new" => Ok(Value::BuiltinFunction(BuiltinFunction::VecNew)),
                "from" => Ok(Value::BuiltinFunction(BuiltinFunction::VecFrom)),
                _ => Err(RuntimeError::new(
                    format!("Vec has no associated function `{member}`"),
                    span,
                )),
            },
            Value::BuiltinType(BuiltinType::Integer(target)) => {
                let intrinsic =
                    rils_builtins::integer_associated_function(member).ok_or_else(|| {
                        RuntimeError::new(
                            format!("{target} has no associated function `{member}`"),
                            span,
                        )
                    })?;
                Ok(Value::BuiltinFunction(BuiltinFunction::IntegerIntrinsic {
                    id: intrinsic.id,
                    target,
                }))
            }
            Value::StructType(definition) => definition
                .methods
                .borrow()
                .get(member)
                .cloned()
                .map(Value::Function)
                .ok_or_else(|| {
                    RuntimeError::new(
                        format!(
                            "struct `{}` has no associated function `{member}`",
                            segments[0]
                        ),
                        span,
                    )
                }),
            Value::EnumType(definition) => {
                if let Some(method) = definition.methods.borrow().get(member).cloned() {
                    return Ok(Value::Function(method));
                }
                let variant = definition
                    .variants
                    .iter()
                    .find(|variant| enum_variant_name(variant) == member)
                    .ok_or_else(|| {
                        RuntimeError::new(
                            format!("enum `{}` has no variant `{member}`", segments[0]),
                            span,
                        )
                    })?;
                match variant {
                    EnumVariant::Unit { .. } => Ok(Value::Enum(Rc::new(EnumInstance {
                        type_arguments: vec![Type::Unknown; definition.generic_parameters.len()],
                        type_definition: definition,
                        variant: member.clone(),
                        payload: EnumPayload::Unit,
                    }))),
                    EnumVariant::Tuple { .. } | EnumVariant::Record { .. } => {
                        Ok(Value::VariantConstructor(Rc::new(VariantConstructor {
                            type_definition: definition,
                            variant: member.clone(),
                            environment: owner_environment,
                        })))
                    }
                }
            }
            Value::TraitType(definition) => {
                if !definition
                    .methods
                    .iter()
                    .any(|method| method.name == *member)
                {
                    return Err(RuntimeError::new(
                        format!("trait `{}` has no method `{member}`", definition.name),
                        span,
                    ));
                }
                Ok(Value::TraitMethodSelector(Rc::new(TraitMethodSelector {
                    target: None,
                    trait_name: definition.name.clone(),
                    method_name: member.clone(),
                    environment: environment.clone(),
                })))
            }
            _ => Err(RuntimeError::new(
                format!("`{}` is not a struct or enum type", segments[0]),
                span,
            )),
        }
    }

    pub(super) fn resolve_qualified_path(
        &self,
        target: &Type,
        trait_name: &str,
        member: &str,
        environment: &EnvironmentRef,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let trait_value = environment
            .borrow()
            .get(trait_name)
            .ok_or_else(|| RuntimeError::new(format!("unknown trait `{trait_name}`"), span))?;
        let Value::TraitType(definition) = trait_value else {
            return Err(RuntimeError::new(
                format!("`{trait_name}` is not a trait"),
                span,
            ));
        };
        if !definition
            .methods
            .iter()
            .any(|method| method.name == member)
        {
            return Err(RuntimeError::new(
                format!("trait `{trait_name}` has no method `{member}`"),
                span,
            ));
        }
        Ok(Value::TraitMethodSelector(Rc::new(TraitMethodSelector {
            target: Some(target.clone()),
            trait_name: trait_name.into(),
            method_name: member.into(),
            environment: environment.clone(),
        })))
    }

    pub(super) fn resolve_member(
        &self,
        object: Value,
        name: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match &object {
            Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::Isize(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::Usize(_) => {
                let intrinsic = rils_builtins::integer_method(name).ok_or_else(|| {
                    RuntimeError::new(
                        format!("{} has no method `{name}`", object.type_name()),
                        span,
                    )
                })?;
                Ok(Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                    receiver: Rc::new(object.clone()),
                    method: BuiltinMethod::IntegerIntrinsic(intrinsic.id),
                })))
            }
            Value::HostObject(instance) => instance
                .type_definition
                .methods
                .borrow()
                .get(name)
                .cloned()
                .map(|function| {
                    Value::HostBoundMethod(Rc::new(HostBoundMethod {
                        receiver: Rc::new(object.clone()),
                        function,
                    }))
                })
                .ok_or_else(|| {
                    RuntimeError::new(
                        format!(
                            "type `{}` has no method `{name}`",
                            instance.type_definition.name
                        ),
                        span,
                    )
                }),
            value if builtin_runtime_member(value, name).is_some() => {
                let (id, _) = builtin_runtime_member(value, name).expect("member was checked");
                Ok(Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                    receiver: Rc::new(object.clone()),
                    method: BuiltinMethod::Runtime(id),
                })))
            }
            Value::Tuple(sequence) => {
                let index = name
                    .parse::<usize>()
                    .map_err(|_| RuntimeError::new(format!("tuple has no field `{name}`"), span))?;
                let mut elements = sequence.elements.borrow_mut();
                let slot = elements.get_mut(index).ok_or_else(|| {
                    RuntimeError::new(format!("tuple index {index} is out of bounds"), span)
                })?;
                let value = slot.value.as_ref().ok_or_else(|| {
                    RuntimeError::new(format!("use of moved tuple field `{index}`"), span)
                })?;
                if value.is_copy() {
                    return value
                        .clone_owned()
                        .map_err(|message| RuntimeError::new(message, span));
                }
                if slot.references > 0 {
                    return Err(RuntimeError::new(
                        format!("cannot move tuple field `{index}` while it is referenced"),
                        span,
                    ));
                }
                Ok(slot.value.take().expect("tuple field value was checked"))
            }
            Value::Struct(instance) => {
                if instance.fields.borrow().contains_key(name) {
                    let mut fields = instance.fields.borrow_mut();
                    let value = fields
                        .get_mut(name)
                        .expect("field presence was checked")
                        .value
                        .as_mut()
                        .ok_or_else(|| {
                            RuntimeError::new(
                                format!(
                                    "use of moved field `{}.{name}`",
                                    instance.type_definition.name
                                ),
                                span,
                            )
                        })?;
                    if value.is_copy() {
                        return value
                            .clone_owned()
                            .map_err(|message| RuntimeError::new(message, span));
                    }
                    let field = fields.get_mut(name).expect("field presence was checked");
                    if field.references > 0 {
                        return Err(RuntimeError::new(
                            format!("cannot move field `{name}` while it is referenced"),
                            span,
                        ));
                    }
                    return Ok(field.value.take().expect("field value was checked"));
                }
                self.bind_method(
                    object.clone(),
                    &instance.type_definition.methods,
                    &instance.type_definition.trait_methods,
                    &instance.type_definition.name,
                    name,
                    span,
                )
            }
            Value::Enum(instance) => self.bind_method(
                object.clone(),
                &instance.type_definition.methods,
                &instance.type_definition.trait_methods,
                &instance.type_definition.name,
                name,
                span,
            ),
            Value::Reference(reference) => {
                let borrowed = reference
                    .read()
                    .map_err(|message| RuntimeError::new(message, span))?;
                if let Some((id, receiver)) = builtin_runtime_member(&borrowed, name) {
                    if receiver == rils_builtins::ReceiverMode::Mutable && !reference.mutable {
                        return Err(RuntimeError::new(
                            format!("{}::{name} requires `&mut self`", borrowed.type_name()),
                            span,
                        ));
                    }
                    return Ok(Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                        receiver: Rc::new(object.clone()),
                        method: BuiltinMethod::Runtime(id),
                    })));
                }
                match borrowed {
                    Value::HostObject(instance) => instance
                        .type_definition
                        .methods
                        .borrow()
                        .get(name)
                        .cloned()
                        .map(|function| {
                            Value::HostBoundMethod(Rc::new(HostBoundMethod {
                                receiver: Rc::new(object.clone()),
                                function,
                            }))
                        })
                        .ok_or_else(|| {
                            RuntimeError::new(
                                format!(
                                    "type `{}` has no method `{name}`",
                                    instance.type_definition.name
                                ),
                                span,
                            )
                        }),
                    Value::Tuple(sequence) => {
                        let index = name.parse::<usize>().map_err(|_| {
                            RuntimeError::new(format!("tuple has no field `{name}`"), span)
                        })?;
                        let elements = sequence.elements.borrow();
                        let value = elements
                            .get(index)
                            .and_then(|slot| slot.value.as_ref())
                            .ok_or_else(|| {
                                RuntimeError::new(
                                    format!("use of moved tuple field `{index}`"),
                                    span,
                                )
                            })?;
                        if !value.is_copy() {
                            return Err(RuntimeError::new(
                                format!(
                                    "cannot move non-Copy tuple field `{index}` through a reference"
                                ),
                                span,
                            ));
                        }
                        value
                            .clone_owned()
                            .map_err(|message| RuntimeError::new(message, span))
                    }
                    Value::Struct(instance) => {
                        if let Some(field) = instance.fields.borrow().get(name) {
                            let value = field.value.as_ref().ok_or_else(|| {
                                RuntimeError::new(format!("use of moved field `{name}`"), span)
                            })?;
                            if value.is_copy() {
                                return value
                                    .clone_owned()
                                    .map_err(|message| RuntimeError::new(message, span));
                            }
                            return Err(RuntimeError::new(
                                format!("cannot move non-Copy field `{name}` through a reference"),
                                span,
                            ));
                        }
                        self.bind_method(
                            object.clone(),
                            &instance.type_definition.methods,
                            &instance.type_definition.trait_methods,
                            &instance.type_definition.name,
                            name,
                            span,
                        )
                    }
                    Value::Enum(instance) => self.bind_method(
                        object.clone(),
                        &instance.type_definition.methods,
                        &instance.type_definition.trait_methods,
                        &instance.type_definition.name,
                        name,
                        span,
                    ),
                    _ if name == "clone" => {
                        Ok(Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                            receiver: Rc::new(object.clone()),
                            method: BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::Clone),
                        })))
                    }
                    value => Err(RuntimeError::new(
                        format!("{} has no member `{name}`", value.type_name()),
                        span,
                    )),
                }
            }
            _ if name == "clone" => Ok(Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                receiver: Rc::new(object),
                method: BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::Clone),
            }))),
            _ => Err(RuntimeError::new(
                format!("{} has no member `{name}`", object.type_name()),
                span,
            )),
        }
    }

    pub(super) fn bind_method(
        &self,
        receiver: Value,
        methods: &std::cell::RefCell<HashMap<String, Rc<UserFunction>>>,
        trait_methods: &std::cell::RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
        type_name: &str,
        name: &str,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let function = select_method(methods, trait_methods, name).map_err(|traits| {
            RuntimeError::new(
                format!(
                    "method `{name}` is ambiguous for `{type_name}`; candidates come from traits {}",
                    traits.join(", ")
                ),
                span,
            )
        })?;
        let Some(function) = function else {
            if name == "clone" {
                return Ok(Value::BuiltinBoundMethod(Rc::new(BuiltinBoundMethod {
                    receiver: Rc::new(receiver),
                    method: BuiltinMethod::Runtime(rils_builtins::RuntimeMemberId::Clone),
                })));
            }
            return Err(RuntimeError::new(
                format!("type `{type_name}` has no member `{name}`"),
                span,
            ));
        };
        if function
            .parameters
            .first()
            .map(|parameter| parameter.name.as_str())
            != Some("self")
        {
            return Err(RuntimeError::new(
                format!("`{type_name}::{name}` is an associated function, not a method"),
                span,
            ));
        }
        let receiver = match function
            .parameters
            .first()
            .and_then(|parameter| parameter.type_annotation.as_ref())
        {
            Some(Type::Reference { mutable, .. }) if !matches!(receiver, Value::Reference(_)) => {
                let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(*mutable)));
                storage.borrow_mut().initialize(receiver);
                Value::Reference(Rc::new(ReferenceValue::new_storage(storage, *mutable)))
            }
            _ => receiver,
        };
        Ok(Value::BoundMethod(Rc::new(BoundMethod {
            receiver: Rc::new(receiver),
            function,
        })))
    }
}

pub(super) fn builtin_runtime_member(
    value: &Value,
    name: &str,
) -> Option<(rils_builtins::RuntimeMemberId, rils_builtins::ReceiverMode)> {
    let owner = match value {
        Value::Array(_) => "Array",
        Value::String(_) => "string",
        Value::Vec(_) => "Vec",
        Value::Range(_) => "Range",
        Value::Option { .. } => "Option",
        Value::Result { .. } => "Result",
        Value::SequenceIterator(_) => "Iterator",
        _ => return None,
    };
    let member = rils_builtins::builtin_member(owner, name)?;
    Some((member.runtime?, member.receiver?))
}

fn validate_native_arguments(
    signature: Option<&FunctionSignature>,
    arguments: &[Value],
    span: Span,
) -> Result<(), RuntimeError> {
    let Some(parameters) = signature.and_then(|signature| signature.parameters.as_ref()) else {
        return Ok(());
    };
    for (index, (expected, argument)) in parameters.iter().zip(arguments).enumerate() {
        apply_type(
            Some(expected),
            argument,
            span,
            &format!("native argument {}", index + 1),
        )?;
    }
    Ok(())
}

fn validate_native_return(
    signature: Option<&FunctionSignature>,
    value: Value,
    span: Span,
    name: &str,
) -> Result<Value, RuntimeError> {
    let Some(signature) = signature else {
        return Ok(value);
    };
    apply_type(
        Some(&signature.return_type),
        &value,
        span,
        &format!("return value of `{name}`"),
    )
}

pub(super) fn select_method(
    methods: &std::cell::RefCell<HashMap<String, Rc<UserFunction>>>,
    trait_methods: &std::cell::RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
    name: &str,
) -> Result<Option<Rc<UserFunction>>, Vec<String>> {
    if let Some(method) = methods.borrow().get(name).cloned() {
        return Ok(Some(method));
    }
    let trait_methods = trait_methods.borrow();
    let mut candidates = trait_methods
        .iter()
        .filter_map(|(trait_name, methods)| {
            methods
                .get(name)
                .cloned()
                .map(|method| (trait_name.clone(), method))
        })
        .collect::<Vec<_>>();
    match candidates.len() {
        0 => Ok(None),
        1 => Ok(Some(candidates.pop().expect("one candidate").1)),
        _ => {
            let mut traits = candidates
                .into_iter()
                .map(|(trait_name, _)| trait_name)
                .collect::<Vec<_>>();
            traits.sort();
            Err(traits)
        }
    }
}
