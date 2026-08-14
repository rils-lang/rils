use super::*;
use crate::environment::{StorageRef, StorageSlot};

enum IteratorCursor {
    Sequence(Rc<SequenceIteratorValue>),
    Range(RangeValue),
    Dynamic {
        storage: StorageRef,
        element_type: Type,
    },
}

impl IteratorCursor {
    fn new(value: &Value, span: Span) -> Result<Self, RuntimeError> {
        let value = match value {
            Value::Reference(reference) => reference
                .read()
                .map_err(|message| RuntimeError::new(message, span))?,
            value => value.clone(),
        };
        Ok(match value {
            Value::SequenceIterator(iterator) => Self::Sequence(iterator),
            Value::Range(range) => Self::Range(range),
            value => {
                let element_type = Type::of_value(&value).unwrap_or(Type::Unknown);
                let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
                storage.borrow_mut().initialize(value);
                Self::Dynamic {
                    storage,
                    element_type,
                }
            }
        })
    }

    fn element_type(&self) -> Type {
        match self {
            Self::Sequence(iterator) => iterator.element_type.clone(),
            Self::Range(range) => range.element_type(),
            Self::Dynamic { element_type, .. } => element_type.clone(),
        }
    }

    fn next(
        &mut self,
        interpreter: &mut Interpreter,
        span: Span,
    ) -> Result<Option<Value>, RuntimeError> {
        match self {
            Self::Sequence(iterator) => Ok(iterator.items.borrow_mut().pop_front()),
            Self::Range(range) => range
                .next()
                .map_err(|message| RuntimeError::new(message, span)),
            Self::Dynamic {
                storage,
                element_type,
            } => {
                let receiver =
                    Value::Reference(Rc::new(ReferenceValue::new_storage(storage.clone(), true)));
                let method = interpreter.resolve_member(receiver, "next", span)?;
                match interpreter.call(method, &[], span)? {
                    Value::Option {
                        value,
                        element_type: next_type,
                    } => {
                        if let Some(next_type) = next_type {
                            *element_type = next_type;
                        }
                        value.map(|value| owned_rc(value, span)).transpose()
                    }
                    value => Err(RuntimeError::new(
                        format!(
                            "Iterator::next must return Option, found {}",
                            value.type_name()
                        ),
                        span,
                    )),
                }
            }
        }
    }
}

impl Interpreter {
    pub(super) fn call_iterator_default_method(
        &mut self,
        id: rils_builtins::RuntimeMemberId,
        receiver: &Value,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        use rils_builtins::RuntimeMemberId::*;

        let mut iterator = IteratorCursor::new(receiver, span)?;
        let source_type = iterator.element_type();
        match id {
            IteratorCount => {
                let mut count = 0usize;
                while iterator.next(self, span)?.is_some() {
                    count += 1;
                }
                return Ok(Value::Usize(count));
            }
            IteratorLast => {
                let mut last = None;
                while let Some(item) = iterator.next(self, span)? {
                    last = Some(item);
                }
                return Ok(option_value(last, source_type));
            }
            IteratorNth => {
                let count = iterator_count_argument(arguments, span)?;
                for _ in 0..count {
                    if iterator.next(self, span)?.is_none() {
                        return Ok(option_value(None, source_type));
                    }
                }
                return Ok(option_value(iterator.next(self, span)?, source_type));
            }
            IteratorCollectVec => {
                let mut elements = Vec::new();
                while let Some(item) = iterator.next(self, span)? {
                    elements.push(FieldSlot {
                        value: Some(item),
                        type_annotation: source_type.clone(),
                        references: 0,
                    });
                }
                return Ok(Value::Vec(Rc::new(SequenceValue {
                    elements: RefCell::new(elements),
                    element_type: RefCell::new(Some(source_type)),
                })));
            }
            IteratorTake | IteratorSkip | IteratorRev => {
                let count = if matches!(id, IteratorTake | IteratorSkip) {
                    iterator_count_argument(arguments, span)?
                } else {
                    0
                };
                let mut items = std::collections::VecDeque::new();
                let mut index = 0usize;
                while let Some(item) = iterator.next(self, span)? {
                    let selected = match id {
                        IteratorTake => index < count,
                        IteratorSkip => index >= count,
                        IteratorRev => true,
                        _ => unreachable!(),
                    };
                    if selected {
                        items.push_back(item);
                    }
                    index += 1;
                }
                if id == IteratorRev {
                    items = items.into_iter().rev().collect();
                }
                return Ok(iterator_value(items, source_type));
            }
            _ => {}
        }
        if id == IteratorEnumerate {
            let mut items = std::collections::VecDeque::new();
            let mut index = 0usize;
            while let Some(item) = iterator.next(self, span)? {
                items.push_back(tuple_value(vec![Value::Usize(index), item]));
                index += 1;
            }
            return Ok(iterator_value(
                items,
                Type::Tuple(vec![Type::USIZE, source_type]),
            ));
        }

        let callback = match id {
            IteratorFold => arguments.get(1),
            _ => arguments.first(),
        }
        .ok_or_else(|| RuntimeError::new("missing iterator callback", span))?
        .clone();

        match id {
            IteratorMap => {
                let mut items = std::collections::VecDeque::new();
                let mut output_type = Type::Unknown;
                while let Some(item) = iterator.next(self, span)? {
                    let mapped = self.call(callback.clone(), &[item], span)?;
                    output_type = merge_runtime_type(output_type, &mapped);
                    items.push_back(mapped);
                }
                Ok(iterator_value(items, output_type))
            }
            IteratorFilter => {
                let mut items = std::collections::VecDeque::new();
                while let Some(item) = iterator.next(self, span)? {
                    let (keep, item) = self.call_iterator_predicate_ref(&callback, item, span)?;
                    if keep {
                        items.push_back(item);
                    }
                }
                Ok(iterator_value(items, source_type))
            }
            IteratorFilterMap => {
                let mut items = std::collections::VecDeque::new();
                let mut output_type = Type::Unknown;
                while let Some(item) = iterator.next(self, span)? {
                    match self.call(callback.clone(), &[item], span)? {
                        Value::Option {
                            value,
                            element_type,
                        } => {
                            if let Some(element_type) = element_type {
                                output_type = merge_types_or_unknown(output_type, element_type);
                            }
                            if let Some(value) = value {
                                let value = owned_rc(value, span)?;
                                output_type = merge_runtime_type(output_type, &value);
                                items.push_back(value);
                            }
                        }
                        value => {
                            return Err(RuntimeError::new(
                                format!(
                                    "Iterator::filter_map callback must return Option, found {}",
                                    value.type_name()
                                ),
                                span,
                            ));
                        }
                    }
                }
                Ok(iterator_value(items, output_type))
            }
            IteratorFold => {
                let mut accumulator = arguments
                    .first()
                    .ok_or_else(|| RuntimeError::new("missing fold initial value", span))?
                    .clone();
                while let Some(item) = iterator.next(self, span)? {
                    accumulator = self.call(callback.clone(), &[accumulator, item], span)?;
                }
                Ok(accumulator)
            }
            IteratorForEach => {
                while let Some(item) = iterator.next(self, span)? {
                    self.call(callback.clone(), &[item], span)?;
                }
                Ok(Value::Unit)
            }
            IteratorAny | IteratorAll => {
                let expected = id == IteratorAll;
                while let Some(item) = iterator.next(self, span)? {
                    let matches = callback_bool(self.call(callback.clone(), &[item], span)?, span)?;
                    if matches != expected {
                        return Ok(Value::Bool(!expected));
                    }
                }
                Ok(Value::Bool(expected))
            }
            IteratorFind => {
                while let Some(item) = iterator.next(self, span)? {
                    let (matches, item) =
                        self.call_iterator_predicate_ref(&callback, item, span)?;
                    if matches {
                        return Ok(Value::Option {
                            value: Some(Rc::new(item)),
                            element_type: Some(source_type),
                        });
                    }
                }
                Ok(Value::Option {
                    value: None,
                    element_type: Some(source_type),
                })
            }
            IteratorPosition => {
                let mut index = 0usize;
                while let Some(item) = iterator.next(self, span)? {
                    if callback_bool(self.call(callback.clone(), &[item], span)?, span)? {
                        return Ok(Value::Option {
                            value: Some(Rc::new(Value::Usize(index))),
                            element_type: Some(Type::USIZE),
                        });
                    }
                    index += 1;
                }
                Ok(Value::Option {
                    value: None,
                    element_type: Some(Type::USIZE),
                })
            }
            _ => unreachable!(),
        }
    }

    fn call_iterator_predicate_ref(
        &mut self,
        callback: &Value,
        item: Value,
        span: Span,
    ) -> Result<(bool, Value), RuntimeError> {
        let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
        storage.borrow_mut().initialize(item);
        let reference =
            Value::Reference(Rc::new(ReferenceValue::new_storage(storage.clone(), false)));
        let result = self.call(callback.clone(), &[reference], span)?;
        let item = storage.borrow_mut().take().map_err(|_| {
            RuntimeError::new("iterator predicate retained its item reference", span)
        })?;
        Ok((callback_bool(result, span)?, item))
    }
}

fn callback_bool(value: Value, span: Span) -> Result<bool, RuntimeError> {
    match value {
        Value::Bool(value) => Ok(value),
        value => Err(RuntimeError::new(
            format!(
                "iterator predicate must return bool, found {}",
                value.type_name()
            ),
            span,
        )),
    }
}

fn iterator_count_argument(arguments: &[Value], span: Span) -> Result<usize, RuntimeError> {
    match arguments.first() {
        Some(Value::Usize(value)) => Ok(*value),
        Some(value) => Err(RuntimeError::new(
            format!("iterator count must be usize, found {}", value.type_name()),
            span,
        )),
        None => Err(RuntimeError::new("missing iterator count", span)),
    }
}

fn option_value(value: Option<Value>, element_type: Type) -> Value {
    Value::Option {
        value: value.map(Rc::new),
        element_type: Some(element_type),
    }
}

fn owned_rc(value: Rc<Value>, span: Span) -> Result<Value, RuntimeError> {
    match Rc::try_unwrap(value) {
        Ok(value) => Ok(value),
        Err(value) => value
            .clone_owned()
            .map_err(|message| RuntimeError::new(message, span)),
    }
}

fn iterator_value(items: std::collections::VecDeque<Value>, element_type: Type) -> Value {
    Value::SequenceIterator(Rc::new(SequenceIteratorValue {
        items: RefCell::new(items),
        element_type,
    }))
}

fn tuple_value(values: Vec<Value>) -> Value {
    Value::Tuple(Rc::new(SequenceValue {
        elements: RefCell::new(
            values
                .into_iter()
                .map(|value| FieldSlot {
                    type_annotation: Type::of_value(&value).unwrap_or(Type::Unknown),
                    value: Some(value),
                    references: 0,
                })
                .collect(),
        ),
        element_type: RefCell::new(None),
    }))
}

fn merge_runtime_type(current: Type, value: &Value) -> Type {
    Type::of_value(value).map_or(current.clone(), |actual| {
        merge_types_or_unknown(current, actual)
    })
}

fn merge_types_or_unknown(current: Type, actual: Type) -> Type {
    if current == Type::Unknown {
        actual
    } else {
        crate::types::merge_types(&current, &actual).unwrap_or(Type::Unknown)
    }
}
