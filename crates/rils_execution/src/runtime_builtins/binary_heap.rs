use std::{cell::RefCell, cmp::Ordering, rc::Rc};

use rils_builtins::BuiltinId;

use crate::{
    types::{Type, merge_types},
    value::{BinaryHeapValue, Value},
};

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    if id == BuiltinId::BinaryHeapNew {
        return Ok(Value::BinaryHeap(Rc::new(BinaryHeapValue {
            elements: RefCell::new(Vec::new()),
            element_type: RefCell::new(Some(Type::Unknown)),
        })));
    }
    let receiver = arguments.first().ok_or("missing BinaryHeap receiver")?;
    let mutating = matches!(
        id,
        BuiltinId::BinaryHeapPush | BuiltinId::BinaryHeapPop | BuiltinId::BinaryHeapClear
    );
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BinaryHeap mutation requires a mutable reference".into());
    }
    let Value::BinaryHeap(heap) = super::import_receiver(receiver)? else {
        return Err("expected BinaryHeap receiver".into());
    };
    match id {
        BuiltinId::BinaryHeapLen => Ok(Value::Usize(heap.elements.borrow().len())),
        BuiltinId::BinaryHeapIsEmpty => Ok(Value::Bool(heap.elements.borrow().is_empty())),
        BuiltinId::BinaryHeapPush => {
            let item = arguments.get(1).ok_or("missing BinaryHeap element")?;
            if !orderable(item) {
                return Err(format!(
                    "BinaryHeap does not support ordering {}",
                    item.type_name()
                ));
            }
            let actual = Type::of_value(item).unwrap_or(Type::Unknown);
            let expected = heap.element_type.borrow().clone().unwrap_or(Type::Unknown);
            let ty = merge_types(&expected, &actual)
                .ok_or_else(|| format!("BinaryHeap expects {expected}, found {actual}"))?;
            let item = ty
                .constrain(item)
                .ok_or("invalid BinaryHeap element type")?;
            let mut elements = heap.elements.borrow_mut();
            if elements.iter().any(Value::has_active_references) {
                return Err("cannot reorder referenced BinaryHeap elements".into());
            }
            elements.push(item);
            let mut index = elements.len() - 1;
            while index > 0 {
                let parent = (index - 1) / 2;
                if compare(&elements[index], &elements[parent])? != Ordering::Greater {
                    break;
                }
                elements.swap(index, parent);
                index = parent;
            }
            *heap.element_type.borrow_mut() = Some(ty);
            Ok(Value::Unit)
        }
        BuiltinId::BinaryHeapPop => {
            let mut elements = heap.elements.borrow_mut();
            if elements.iter().any(Value::has_active_references) {
                return Err("cannot remove referenced BinaryHeap elements".into());
            }
            let value = if elements.is_empty() {
                None
            } else {
                let last = elements.len() - 1;
                elements.swap(0, last);
                let value = elements.pop();
                let mut index = 0;
                while index * 2 + 1 < elements.len() {
                    let left = index * 2 + 1;
                    let right = left + 1;
                    let child = if right < elements.len()
                        && compare(&elements[right], &elements[left])? == Ordering::Greater
                    {
                        right
                    } else {
                        left
                    };
                    if compare(&elements[child], &elements[index])? != Ordering::Greater {
                        break;
                    }
                    elements.swap(index, child);
                    index = child;
                }
                value
            };
            Ok(Value::Option {
                value: value.map(Rc::new),
                element_type: heap.element_type.borrow().clone(),
            })
        }
        BuiltinId::BinaryHeapPeekCloned => Ok(Value::Option {
            value: heap
                .elements
                .borrow()
                .first()
                .map(Value::clone_owned)
                .transpose()?
                .map(Rc::new),
            element_type: heap.element_type.borrow().clone(),
        }),
        BuiltinId::BinaryHeapClear => {
            let mut elements = heap.elements.borrow_mut();
            if elements.iter().any(Value::has_active_references) {
                return Err("cannot clear referenced BinaryHeap elements".into());
            }
            elements.clear();
            Ok(Value::Unit)
        }
        _ => Err("unsupported BinaryHeap operation".into()),
    }
}

fn orderable(value: &Value) -> bool {
    matches!(
        value,
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
            | Value::Usize(_)
            | Value::Char(_)
            | Value::String(_)
    )
}

fn compare(left: &Value, right: &Value) -> Result<Ordering, String> {
    macro_rules! compare_variants {
        ($($variant:ident),+ $(,)?) => {
            match (left, right) {
                $((Value::$variant(left), Value::$variant(right)) => Ok(left.cmp(right)),)+
                _ => Err("BinaryHeap elements must have the same orderable type".into()),
            }
        };
    }
    compare_variants!(
        I8, I16, I32, I64, I128, Isize, U8, U16, U32, U64, U128, Usize, Char, String
    )
}
