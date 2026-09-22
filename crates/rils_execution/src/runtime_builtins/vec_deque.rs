use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use rils_builtins::BuiltinId;

use crate::{
    types::{Type, merge_types},
    value::{Value, VecDequeValue},
};

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    if id == BuiltinId::VecDequeNew {
        return Ok(Value::VecDeque(Rc::new(VecDequeValue {
            elements: RefCell::new(VecDeque::new()),
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
        BuiltinId::VecDequeLen => Ok(Value::Usize(queue.elements.borrow().len())),
        BuiltinId::VecDequeIsEmpty => Ok(Value::Bool(queue.elements.borrow().is_empty())),
        BuiltinId::VecDequePushFront | BuiltinId::VecDequePushBack => {
            let item = arguments.get(1).ok_or("missing VecDeque element")?;
            let actual = Type::of_value(item).unwrap_or(Type::Unknown);
            let expected = queue.element_type.borrow().clone().unwrap_or(Type::Unknown);
            let ty = merge_types(&expected, &actual)
                .ok_or_else(|| format!("VecDeque expects {expected}, found {actual}"))?;
            let item = ty.constrain(item).ok_or("invalid VecDeque element type")?;
            *queue.element_type.borrow_mut() = Some(ty);
            if id == BuiltinId::VecDequePushFront {
                queue.elements.borrow_mut().push_front(item);
            } else {
                queue.elements.borrow_mut().push_back(item);
            }
            Ok(Value::Unit)
        }
        BuiltinId::VecDequePopFront | BuiltinId::VecDequePopBack => {
            let mut elements = queue.elements.borrow_mut();
            let candidate = if id == BuiltinId::VecDequePopFront {
                elements.front()
            } else {
                elements.back()
            };
            if candidate.is_some_and(Value::has_active_references) {
                return Err("cannot remove a referenced VecDeque element".into());
            }
            let value = if id == BuiltinId::VecDequePopFront {
                elements.pop_front()
            } else {
                elements.pop_back()
            };
            Ok(Value::Option {
                value: value.map(Rc::new),
                element_type: queue.element_type.borrow().clone(),
            })
        }
        BuiltinId::VecDequeFrontCloned | BuiltinId::VecDequeBackCloned => {
            let elements = queue.elements.borrow();
            let value = if id == BuiltinId::VecDequeFrontCloned {
                elements.front()
            } else {
                elements.back()
            };
            Ok(Value::Option {
                value: value.map(Value::clone_owned).transpose()?.map(Rc::new),
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
            queue.elements.borrow_mut().clear();
            Ok(Value::Unit)
        }
        _ => Err("unsupported VecDeque operation".into()),
    }
}
