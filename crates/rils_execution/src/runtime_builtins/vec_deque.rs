use std::{cell::RefCell, rc::Rc};

use rils_builtins::{BuiltinId, BuiltinMember, ReceiverMode, TypePattern};
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
    call_named(
        id.member_name().ok_or("unknown VecDeque operation")?,
        arguments,
    )
}

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let member = rils_builtins::builtin("VecDeque")?
        .members
        .iter()
        .find(|member| member.native_symbol == Some(symbol) && member.receiver.is_some())?;
    Some(call_named(member.name, arguments))
}

fn owned_member(symbol: &str) -> Option<&'static BuiltinMember> {
    let declaration = rils_builtins::builtin("VecDeque")?;
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
    rils_builtins::builtin("VecDeque").is_some_and(|declaration| {
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
                return Err("VecDeque::into_iter expects one receiver".into());
            }
            let receiver = arguments.into_iter().next().expect("arity checked");
            match crate::iteration::into_iterator_with_context(receiver, context)? {
                crate::iteration::IntoIteratorResult::Ready(iterator) => Ok(iterator),
                crate::iteration::IntoIteratorResult::UserDefined(_) => {
                    Err("VecDeque::into_iter expects a VecDeque receiver".into())
                }
            }
        })());
    }
    let member = owned_member(symbol)?;
    Some((|| {
        if arguments.len() != 2 {
            return Err(format!(
                "native method `{symbol}` expects 2 arguments, found {}",
                arguments.len()
            ));
        }
        let receiver = &arguments[0];
        if !matches!(receiver, Value::Reference(reference) if reference.mutable) {
            return Err("VecDeque mutation requires a mutable reference".into());
        }
        let receiver = match receiver {
            Value::Reference(reference) => reference.read()?,
            value => value.clone(),
        };
        let item = arguments.pop().expect("arity checked");
        match receiver {
            Value::Dynamic(object)
                if crate::value::native_layouts::vec_deque::matches(
                    object.descriptor().layout().rils_type(),
                ) =>
            {
                let layout = object
                    .descriptor()
                    .layout()
                    .sequence_item()
                    .ok_or("VecDeque has no native element layout")?
                    .clone();
                if !layout.rils_type().accepts(&item) {
                    return Err(format!(
                        "VecDeque expects {}, found {}",
                        layout.rils_type(),
                        item.type_name()
                    ));
                }
                let mut codec = crate::value::record_codec::NativeRecordCodec::with_definitions(
                    &context.structs,
                    &context.enums,
                );
                let item = codec.into_native(item, layout)?;
                object.with_mut(|value| match member.name {
                    "push_front" => value.push_sequence_front(item),
                    "push_back" => value.push_sequence_item(item),
                    name => Err(format!(
                        "owned native VecDeque method `{name}` is not supported"
                    )),
                })??;
            }
            Value::VecDeque(queue) => {
                let actual = Type::of_value(&item).unwrap_or(Type::Unknown);
                let expected = queue.element_type.borrow().clone().unwrap_or(Type::Unknown);
                let ty = merge_types(&expected, &actual)
                    .ok_or_else(|| format!("VecDeque expects {expected}, found {actual}"))?;
                *queue.element_type.borrow_mut() = Some(ty);
                match member.name {
                    "push_front" => with_native(&queue, |native| native.push_front(item)),
                    "push_back" => with_native(&queue, |native| native.push_back(item)),
                    name => {
                        return Err(format!(
                            "owned native VecDeque method `{name}` is not supported"
                        ));
                    }
                }
            }
            _ => return Err("expected VecDeque receiver".into()),
        }
        Ok(Value::Unit)
    })())
}

fn call_named(name: &str, arguments: &[Value]) -> Result<Value, String> {
    let receiver = arguments.first().ok_or("missing VecDeque receiver")?;
    let mutating = matches!(
        name,
        "push_front" | "push_back" | "pop_front" | "pop_back" | "clear"
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
        return call_dynamic(name, arguments, &object);
    }
    let Value::VecDeque(queue) = super::import_receiver(receiver)? else {
        return Err("expected VecDeque receiver".into());
    };
    call_queue(name, arguments, &queue)
}

fn call_dynamic(
    name: &str,
    arguments: &[Value],
    object: &crate::value::DynamicObject,
) -> Result<Value, String> {
    let item_layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("VecDeque has no native element layout")?;
    let item_type = item_layout.rils_type().clone();
    match name {
        "len" => Ok(crate::numeric::native_usize(
            object.with(|value| value.sequence_len())??,
        )),
        "is_empty" => Ok(Value::Bool(
            object.with(|value| value.sequence_len())?? == 0,
        )),
        "push_front" | "push_back" => {
            let item = arguments.get(1).ok_or("missing VecDeque element")?.clone();
            let item = crate::value::record_codec::into_native(item, item_layout.clone())?;
            object.with_mut(|value| {
                if name == "push_front" {
                    value.push_sequence_front(item)
                } else {
                    value.push_sequence_item(item)
                }
            })??;
            Ok(Value::Unit)
        }
        "pop_front" | "pop_back" => {
            let item = object.with_mut(|value| {
                let length = value.sequence_len()?;
                if length == 0 {
                    Ok(None)
                } else {
                    let index = if name == "pop_front" { 0 } else { length - 1 };
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
        "clear" => {
            object.with_mut(|value| value.clear_sequence())??;
            Ok(Value::Unit)
        }
        "front_cloned" | "back_cloned" => {
            let item = object.with(|value| {
                let length = value.sequence_len()?;
                if length == 0 {
                    return Ok(None);
                }
                let index = if name == "front_cloned" {
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

fn call_queue(name: &str, arguments: &[Value], queue: &VecDequeValue) -> Result<Value, String> {
    match name {
        "len" => Ok(crate::numeric::native_usize(with_native(queue, |native| {
            native.len()
        }))),
        "is_empty" => Ok(Value::Bool(with_native(queue, |native| native.is_empty()))),
        "push_front" | "push_back" => {
            let item = arguments.get(1).ok_or("missing VecDeque element")?;
            let actual = Type::of_value(item).unwrap_or(Type::Unknown);
            let expected = queue.element_type.borrow().clone().unwrap_or(Type::Unknown);
            let ty = merge_types(&expected, &actual)
                .ok_or_else(|| format!("VecDeque expects {expected}, found {actual}"))?;
            let item = ty.constrain(item).ok_or("invalid VecDeque element type")?;
            *queue.element_type.borrow_mut() = Some(ty);
            with_native(queue, |native| {
                if name == "push_front" {
                    native.push_front(item);
                } else {
                    native.push_back(item);
                }
            });
            Ok(Value::Unit)
        }
        "pop_front" | "pop_back" => {
            let has_references = {
                let elements = queue.elements.borrow();
                let candidate = if name == "pop_front" {
                    elements.front()
                } else {
                    elements.back()
                };
                candidate.is_some_and(Value::has_active_references)
            };
            if has_references {
                return Err("cannot remove a referenced VecDeque element".into());
            }
            let value = if name == "pop_front" {
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
        "front_cloned" | "back_cloned" => {
            let elements = queue.elements.borrow();
            let value = if name == "front_cloned" {
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
        "clear" => {
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
