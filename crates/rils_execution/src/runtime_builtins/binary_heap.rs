use std::{cell::RefCell, cmp::Ordering, rc::Rc};

use rils_builtins::BuiltinId;
use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::DynamicValue;

use crate::{
    types::{Type, merge_types},
    value::{BinaryHeapValue, HashKey, Value},
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
    let value = match receiver {
        Value::Reference(reference) => reference.read()?,
        value => value.clone(),
    };
    if let Value::Dynamic(object) = value
        && crate::value::native_layouts::binary_heap::matches(
            object.descriptor().layout().rils_type(),
        )
    {
        return call_dynamic(id, arguments, &object);
    }
    let Value::BinaryHeap(heap) = super::import_receiver(receiver)? else {
        return Err("expected BinaryHeap receiver".into());
    };
    call_heap(id, arguments, &heap)
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
        .ok_or("BinaryHeap has no native element layout")?;
    let item_type = item_layout.rils_type().clone();
    match id {
        BuiltinId::BinaryHeapLen => Ok(crate::numeric::native_usize(
            object.with(|value| value.sequence_len())??,
        )),
        BuiltinId::BinaryHeapIsEmpty => Ok(Value::Bool(
            object.with(|value| value.sequence_len())?? == 0,
        )),
        BuiltinId::BinaryHeapPush => {
            let item = arguments.get(1).ok_or("missing BinaryHeap element")?;
            if !orderable(item) {
                return Err(format!(
                    "BinaryHeap does not support ordering {}",
                    item.type_name()
                ));
            }
            let item = crate::value::record_codec::into_native(item.clone(), item_layout.clone())?;
            object.with_mut(|value| {
                value.push_sequence_item(item)?;
                let mut index = value.sequence_len()? - 1;
                while index > 0 {
                    let parent = (index - 1) / 2;
                    if compare_native_items(value, index, parent)? != Ordering::Greater {
                        break;
                    }
                    value.swap_sequence_items(index, parent)?;
                    index = parent;
                }
                Ok::<(), String>(())
            })??;
            Ok(Value::Unit)
        }
        BuiltinId::BinaryHeapPop => {
            let item = object.with_mut(|value| {
                let length = value.sequence_len()?;
                if length == 0 {
                    return Ok(None);
                }
                value.swap_sequence_items(0, length - 1)?;
                let item = value.take_sequence_item(length - 1)?;
                let mut index = 0;
                while index * 2 + 1 < value.sequence_len()? {
                    let left = index * 2 + 1;
                    let right = left + 1;
                    let child = if right < value.sequence_len()?
                        && compare_native_items(value, right, left)? == Ordering::Greater
                    {
                        right
                    } else {
                        left
                    };
                    if compare_native_items(value, child, index)? != Ordering::Greater {
                        break;
                    }
                    value.swap_sequence_items(index, child)?;
                    index = child;
                }
                Ok::<_, String>(Some(item))
            })??;
            Ok(Value::Option {
                value: item
                    .map(crate::value::record_codec::from_native)
                    .transpose()?
                    .map(Rc::new),
                element_type: Some(item_type),
            })
        }
        BuiltinId::BinaryHeapPeekCloned => {
            let value = object.with(|value| {
                if value.sequence_len()? == 0 {
                    Ok(None)
                } else {
                    value
                        .with_sequence_item(0, native_key)?
                        .map(|key| Some(key.to_value()))
                }
            })??;
            Ok(Value::Option {
                value: value.map(Rc::new),
                element_type: Some(item_type),
            })
        }
        BuiltinId::BinaryHeapClear => {
            object.with_mut(|value| value.clear_sequence())??;
            Ok(Value::Unit)
        }
        _ => Err("unsupported BinaryHeap operation".into()),
    }
}

fn compare_native_items(
    value: &DynamicValue,
    left: usize,
    right: usize,
) -> Result<Ordering, String> {
    let left = value.with_sequence_item(left, native_key)??;
    let right = value.with_sequence_item(right, native_key)??;
    Ok(left.cmp(&right))
}

fn native_key(value: &DynamicValue) -> Result<HashKey, String> {
    let value = if value.descriptor().rils_type() == &Type::String {
        let text = value.with::<NativeString, _>(|text| std::string::String::from(text.clone()))?;
        crate::value::native_string(text)
    } else {
        crate::value::record_codec::from_native(value.copy_owned()?)?
    };
    HashKey::from_ordered_value(&value)
}

fn call_heap(id: BuiltinId, arguments: &[Value], heap: &BinaryHeapValue) -> Result<Value, String> {
    match id {
        BuiltinId::BinaryHeapLen => Ok(crate::numeric::native_usize(heap.elements.borrow().len())),
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
    if crate::numeric::i8_payload(value).is_some() {
        return true;
    }
    if crate::numeric::i32_payload(value).is_some() {
        return true;
    }
    if value.as_usize().is_some() {
        return true;
    }
    if value.as_string().is_some() {
        return true;
    }
    matches!(
        crate::numeric::lower_migrated_integer(value.clone()),
        Value::I16(_)
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
    )
}

fn compare(left: &Value, right: &Value) -> Result<Ordering, String> {
    if let (Some(left), Some(right)) = (
        crate::numeric::i8_payload(left),
        crate::numeric::i8_payload(right),
    ) {
        return Ok(left.cmp(&right));
    }
    if let (Some(left), Some(right)) = (
        crate::numeric::i32_payload(left),
        crate::numeric::i32_payload(right),
    ) {
        return Ok(left.cmp(&right));
    }
    if let (Some(left), Some(right)) = (left.as_usize(), right.as_usize()) {
        return Ok(left.cmp(&right));
    }
    if let (Some(left), Some(right)) = (left.as_string(), right.as_string()) {
        return Ok(left.cmp(&right));
    }
    macro_rules! compare_variants {
        ($left:expr, $right:expr; $($variant:ident),+ $(,)?) => {
            match ($left, $right) {
                $((Value::$variant(left), Value::$variant(right)) => Ok(left.cmp(right)),)+
                _ => Err("BinaryHeap elements must have the same orderable type".into()),
            }
        };
    }
    let lowered_left = crate::numeric::lower_migrated_integer(left.clone());
    let lowered_right = crate::numeric::lower_migrated_integer(right.clone());
    compare_variants!(
        &lowered_left,
        &lowered_right;
        I16, I64, I128, Isize, U8, U16, U32, U64, U128, Usize, Char
    )
}
