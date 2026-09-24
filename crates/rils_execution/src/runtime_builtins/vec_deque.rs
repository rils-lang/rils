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
    let Value::VecDeque(queue) = super::import_receiver(receiver)? else {
        return Err("expected VecDeque receiver".into());
    };
    match id {
        BuiltinId::VecDequeLen => Ok(Value::Usize(with_native(&queue, |native| native.len()))),
        BuiltinId::VecDequeIsEmpty => {
            Ok(Value::Bool(with_native(&queue, |native| native.is_empty())))
        }
        BuiltinId::VecDequePushFront | BuiltinId::VecDequePushBack => {
            let item = arguments.get(1).ok_or("missing VecDeque element")?;
            let actual = Type::of_value(item).unwrap_or(Type::Unknown);
            let expected = queue.element_type.borrow().clone().unwrap_or(Type::Unknown);
            let ty = merge_types(&expected, &actual)
                .ok_or_else(|| format!("VecDeque expects {expected}, found {actual}"))?;
            let item = ty.constrain(item).ok_or("invalid VecDeque element type")?;
            *queue.element_type.borrow_mut() = Some(ty);
            with_native(&queue, |native| {
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
                with_native(&queue, |native| native.pop_front())
            } else {
                with_native(&queue, |native| native.pop_back())
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
            with_native(&queue, |native| native.clear());
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
