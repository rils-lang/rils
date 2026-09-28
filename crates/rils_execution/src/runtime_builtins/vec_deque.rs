use std::{cell::RefCell, rc::Rc};

use rils_builtins::BuiltinId;
use rils_stdlib::stdlib::{prelude::Option as NativeOption, vec_deque::VecDeque as NativeVecDeque};

use crate::{
    types::{Type, merge_types},
    value::{Value, VecDequeValue},
};

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    if id == BuiltinId::VecDequeNew {
        return Ok(Value::VecDeque(Rc::new(VecDequeValue {
            elements: RefCell::new(NativeVecDeque::<Value>::new().into()),
            element_type: RefCell::new(Some(Type::Unknown)),
        })));
    }
    let receiver = arguments.first().ok_or("missing VecDeque receiver")?;
    let mutating = matches!(
        id,
        BuiltinId::VecDequePushFront
            | BuiltinId::VecDequePushBack
            | BuiltinId::VecDequePopFront
            | BuiltinId::VecDequePopBack
            | BuiltinId::VecDequeClear
    );
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("VecDeque mutation requires a mutable reference".into());
    }
    let value = match receiver {
        Value::Reference(reference) => reference.read()?,
        value => value.clone(),
    };
    if let Value::Dynamic(object) = value
        && crate::value::native_layouts::vec_deque::matches(
            object.descriptor().layout().rils_type(),
        )
    {
        return call_dynamic(id, arguments, &object);
    }
    let Value::VecDeque(queue) = super::import_receiver(receiver)? else {
        return Err("expected VecDeque receiver".into());
    };
    call_queue(id, arguments, &queue)
}

fn call_dynamic(
    id: BuiltinId,
    arguments: &[Value],
    object: &crate::value::DynamicObject,
) -> Result<Value, String> {
    let item_layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("VecDeque has no native element layout")?;
    let item_type = item_layout.rils_type().clone();
    match id {
        BuiltinId::VecDequeLen => Ok(crate::numeric::native_usize(
            object.with(|value| value.sequence_len())??,
        )),
        BuiltinId::VecDequeIsEmpty => Ok(Value::Bool(
            object.with(|value| value.sequence_len())?? == 0,
        )),
        BuiltinId::VecDequePushFront | BuiltinId::VecDequePushBack => {
            let item = arguments.get(1).ok_or("missing VecDeque element")?.clone();
            let item = crate::value::record_codec::into_native(item, item_layout.clone())?;
            object.with_mut(|value| {
                if id == BuiltinId::VecDequePushFront {
                    value.push_sequence_front(item)
                } else {
                    value.push_sequence_item(item)
                }
            })??;
            Ok(Value::Unit)
        }
        BuiltinId::VecDequePopFront | BuiltinId::VecDequePopBack => {
            let item = object.with_mut(|value| {
                let length = value.sequence_len()?;
                if length == 0 {
                    Ok(None)
                } else {
                    let index = if id == BuiltinId::VecDequePopFront {
                        0
                    } else {
                        length - 1
                    };
                    value.take_sequence_item(index).map(Some)
                }
            })??;
            let item = item
                .map(crate::value::record_codec::from_native)
                .transpose()?;
            Ok(Value::Option {
                value: item.map(Rc::new),
                element_type: Some(item_type),
            })
        }
        BuiltinId::VecDequeClear => {
            object.with_mut(|value| value.clear_sequence())??;
            Ok(Value::Unit)
        }
        BuiltinId::VecDequeFrontCloned | BuiltinId::VecDequeBackCloned => {
            let item = object.with(|value| {
                let length = value.sequence_len()?;
                if length == 0 {
                    return Ok(None);
                }
                let index = if id == BuiltinId::VecDequeFrontCloned {
                    0
                } else {
                    length - 1
                };
                value
                    .with_sequence_item(index, |item| {
                        rils_stdlib::native::registry().clone_borrowed_element(item)
                    })?
                    .map(Some)
            })??;
            Ok(Value::Option {
                value: item
                    .map(crate::value::record_codec::from_native)
                    .transpose()?
                    .map(Rc::new),
                element_type: Some(item_type),
            })
        }
        _ => Err("unsupported VecDeque operation".into()),
    }
}

fn call_queue(id: BuiltinId, arguments: &[Value], queue: &VecDequeValue) -> Result<Value, String> {
    match id {
        BuiltinId::VecDequeLen => Ok(crate::numeric::native_usize(with_native(queue, |native| {
            native.len()
        }))),
        BuiltinId::VecDequeIsEmpty => {
            Ok(Value::Bool(with_native(queue, |native| native.is_empty())))
        }
        BuiltinId::VecDequePushFront | BuiltinId::VecDequePushBack => {
            let item = arguments.get(1).ok_or("missing VecDeque element")?;
            let actual = Type::of_value(item).unwrap_or(Type::Unknown);
            let expected = queue.element_type.borrow().clone().unwrap_or(Type::Unknown);
            let ty = merge_types(&expected, &actual)
                .ok_or_else(|| format!("VecDeque expects {expected}, found {actual}"))?;
            let item = ty.constrain(item).ok_or("invalid VecDeque element type")?;
            *queue.element_type.borrow_mut() = Some(ty);
            with_native(queue, |native| {
                if id == BuiltinId::VecDequePushFront {
                    native.push_front(item);
                } else {
                    native.push_back(item);
                }
            });
            Ok(Value::Unit)
        }
        BuiltinId::VecDequePopFront | BuiltinId::VecDequePopBack => {
            let has_references = {
                let elements = queue.elements.borrow();
                let candidate = if id == BuiltinId::VecDequePopFront {
                    elements.front()
                } else {
                    elements.back()
                };
                candidate.is_some_and(Value::has_active_references)
            };
            if has_references {
                return Err("cannot remove a referenced VecDeque element".into());
            }
            let value = if id == BuiltinId::VecDequePopFront {
                with_native(queue, |native| native.pop_front())
            } else {
                with_native(queue, |native| native.pop_back())
            };
            Ok(Value::Option {
                value: match value {
                    NativeOption::Some(value) => Some(Rc::new(value)),
                    NativeOption::None => None,
                },
                element_type: queue.element_type.borrow().clone(),
            })
        }
        BuiltinId::VecDequeFrontCloned | BuiltinId::VecDequeBackCloned => {
            let elements = queue.elements.borrow();
            let value = if id == BuiltinId::VecDequeFrontCloned {
                NativeVecDeque::<Value>::clone_front_with(&elements, Value::clone_owned)?
            } else {
                NativeVecDeque::<Value>::clone_back_with(&elements, Value::clone_owned)?
            };
            Ok(Value::Option {
                value: match value {
                    NativeOption::Some(value) => Some(Rc::new(value)),
                    NativeOption::None => None,
                },
                element_type: queue.element_type.borrow().clone(),
            })
        }
        BuiltinId::VecDequeClear => {
            if queue
                .elements
                .borrow()
                .iter()
                .any(Value::has_active_references)
            {
                return Err("cannot clear referenced VecDeque elements".into());
            }
            with_native(queue, |native| native.clear());
            Ok(Value::Unit)
        }
        _ => Err("unsupported VecDeque operation".into()),
    }
}

fn with_native<R>(
    queue: &VecDequeValue,
    operation: impl FnOnce(&mut NativeVecDeque<Value>) -> R,
) -> R {
    let mut elements = queue.elements.borrow_mut();
    let mut native = NativeVecDeque::from(std::mem::take(&mut *elements));
    let result = operation(&mut native);
    *elements = native.into();
    result
}
