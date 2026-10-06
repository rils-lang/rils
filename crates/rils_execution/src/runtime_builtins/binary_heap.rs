use crate::value::native_receiver::NativeReceiver;
use std::{cell::RefCell, cmp::Ordering, rc::Rc};

use rils_builtins::{BuiltinMember, ReceiverMode, TypePattern};
use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::DynamicValue;

use crate::{
    types::{Type, merge_types},
    value::{BinaryHeapValue, HashKey, Value},
};

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let member = rils_builtins::builtin("BinaryHeap")?
        .members
        .iter()
        .find(|member| member.native_symbol == Some(symbol))?;
    if member.name == "new" {
        return Some(if arguments.is_empty() {
            Ok(Value::BinaryHeap(Rc::new(BinaryHeapValue {
                elements: RefCell::new(Vec::new()),
                element_type: RefCell::new(Some(Type::Unknown)),
            })))
        } else {
            Err(format!(
                "BinaryHeap::new expects 0 arguments, found {}",
                arguments.len()
            ))
        });
    }
    member.receiver?;
    Some(call_named(member.name, arguments))
}

fn owned_member(symbol: &str) -> Option<&'static BuiltinMember> {
    let declaration = rils_builtins::builtin("BinaryHeap")?;
    declaration.members.iter().find(|member| {
        member.native_symbol == Some(symbol)
            && member.receiver == Some(ReceiverMode::Mutable)
            && member.signature.is_some_and(|signature| {
                signature
                    .parameters
                    .iter()
                    .any(|parameter| matches!(parameter, TypePattern::Generic(name) if declaration.type_parameters.contains(name)))
            })
    })
}

fn into_iter_member(symbol: &str) -> bool {
    rils_builtins::builtin("BinaryHeap").is_some_and(|declaration| {
        declaration
            .members
            .iter()
            .any(|member| member.name == "into_iter" && member.native_symbol == Some(symbol))
    })
}

pub(super) fn is_owned_symbol(symbol: &str) -> bool {
    owned_member(symbol).is_some() || into_iter_member(symbol)
}

pub(super) fn call_owned_symbol(
    symbol: &str,
    mut arguments: Vec<Value>,
    context: &super::NativeOwnedContext,
) -> Option<Result<Value, String>> {
    if into_iter_member(symbol) {
        return Some((|| {
            if arguments.len() != 1 {
                return Err("BinaryHeap::into_iter expects one receiver".into());
            }
            let receiver = arguments.into_iter().next().expect("arity checked");
            match crate::iteration::into_iterator_with_context(receiver, context)? {
                crate::iteration::IntoIteratorResult::Ready(iterator) => Ok(iterator),
                crate::iteration::IntoIteratorResult::UserDefined(_) => {
                    Err("BinaryHeap::into_iter expects a BinaryHeap receiver".into())
                }
            }
        })());
    }
    let member = owned_member(symbol)?;
    Some((|| {
        if member.name != "push" {
            return Err(format!(
                "owned native BinaryHeap method `{}` is not supported",
                member.name
            ));
        }
        if arguments.len() != 2 {
            return Err(format!(
                "native method `{symbol}` expects 2 arguments, found {}",
                arguments.len()
            ));
        }
        let receiver = &arguments[0];
        if !matches!(receiver, Value::Reference(reference) if reference.mutable) {
            return Err("BinaryHeap mutation requires a mutable reference".into());
        }
        let item = arguments.pop().expect("arity checked");
        if !orderable(&item) {
            return Err(format!(
                "BinaryHeap does not support ordering {}",
                item.type_name()
            ));
        }
        if let Some(object) = NativeReceiver::from_value(&arguments[0])? {
            if !crate::value::native_layouts::binary_heap::matches(
                object.descriptor().layout().rils_type(),
            ) {
                return Err("wrong native collection receiver".into());
            }
            let layout = object
                .descriptor()
                .layout()
                .sequence_item()
                .ok_or("BinaryHeap has no native element layout")?
                .clone();
            if !layout.rils_type().accepts(&item) {
                return Err(format!(
                    "BinaryHeap expects {}, found {}",
                    layout.rils_type(),
                    item.type_name()
                ));
            }
            let mut codec = crate::value::record_codec::NativeRecordCodec::with_definitions(
                &context.structs,
                &context.enums,
            );
            let item = codec.into_native(item, layout)?;
            return push_dynamic_item(&object, item);
        }
        match super::import_receiver(&arguments[0])? {
            Value::BinaryHeap(heap) => push_heap_item(&heap, item),
            _ => Err("expected BinaryHeap receiver".into()),
        }
    })())
}

fn call_named(name: &str, arguments: &[Value]) -> Result<Value, String> {
    let receiver = arguments.first().ok_or("missing BinaryHeap receiver")?;
    let mutating = matches!(name, "push" | "pop" | "clear");
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BinaryHeap mutation requires a mutable reference".into());
    }
    if let Some(object) = NativeReceiver::from_value(receiver)?
        && crate::value::native_layouts::binary_heap::matches(
            object.descriptor().layout().rils_type(),
        )
    {
        return call_dynamic(name, arguments, &object);
    }
    let Value::BinaryHeap(heap) = super::import_receiver(receiver)? else {
        return Err("expected BinaryHeap receiver".into());
    };
    call_heap(name, arguments, &heap)
}

fn call_dynamic(name: &str, arguments: &[Value], object: &NativeReceiver) -> Result<Value, String> {
    let item_layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("BinaryHeap has no native element layout")?;
    let item_type = item_layout.rils_type().clone();
    match name {
        "len" => Ok(crate::numeric::native_usize(
            object.with(|value| value.sequence_len())??,
        )),
        "is_empty" => Ok(Value::Bool(
            object.with(|value| value.sequence_len())?? == 0,
        )),
        "push" => {
            let item = arguments.get(1).ok_or("missing BinaryHeap element")?;
            if !orderable(item) {
                return Err(format!(
                    "BinaryHeap does not support ordering {}",
                    item.type_name()
                ));
            }
            let item = crate::value::record_codec::into_native(item.clone(), item_layout.clone())?;
            push_dynamic_item(object, item)
        }
        "pop" => {
            let item = object.with_mut(|mut value| {
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
                        && compare_native_items(&value.view(), right, left)? == Ordering::Greater
                    {
                        right
                    } else {
                        left
                    };
                    if compare_native_items(&value.view(), child, index)? != Ordering::Greater {
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
        "peek_cloned" => {
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
        "clear" => {
            object.with_mut(|mut value| value.clear_sequence())??;
            Ok(Value::Unit)
        }
        _ => Err("unsupported BinaryHeap operation".into()),
    }
}

fn push_dynamic_item(object: &NativeReceiver, item: DynamicValue) -> Result<Value, String> {
    object.with_mut(|mut value| {
        value.push_sequence_item(item)?;
        let mut index = value.sequence_len()? - 1;
        while index > 0 {
            let parent = (index - 1) / 2;
            if compare_native_items(&value.view(), index, parent)? != Ordering::Greater {
                break;
            }
            value.swap_sequence_items(index, parent)?;
            index = parent;
        }
        Ok::<(), String>(())
    })??;
    Ok(Value::Unit)
}

fn push_heap_item(heap: &BinaryHeapValue, item: Value) -> Result<Value, String> {
    let actual = Type::of_value(&item).unwrap_or(Type::Unknown);
    let expected = heap.element_type.borrow().clone().unwrap_or(Type::Unknown);
    let ty = merge_types(&expected, &actual)
        .ok_or_else(|| format!("BinaryHeap expects {expected}, found {actual}"))?;
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

fn compare_native_items(
    value: &rils_value::DynamicValueRef<'_>,
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

fn call_heap(name: &str, arguments: &[Value], heap: &BinaryHeapValue) -> Result<Value, String> {
    match name {
        "len" => Ok(crate::numeric::native_usize(heap.elements.borrow().len())),
        "is_empty" => Ok(Value::Bool(heap.elements.borrow().is_empty())),
        "push" => {
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
            push_heap_item(heap, item)
        }
        "pop" => {
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
        "peek_cloned" => Ok(Value::Option {
            value: heap
                .elements
                .borrow()
                .first()
                .map(Value::clone_owned)
                .transpose()?
                .map(Rc::new),
            element_type: heap.element_type.borrow().clone(),
        }),
        "clear" => {
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
    if crate::value::char_payload(value).is_some() {
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
    if let (Some(left), Some(right)) = (
        crate::value::char_payload(left),
        crate::value::char_payload(right),
    ) {
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
