//! Vec operations over declaration-derived native sequences.

use std::rc::Rc;

use rils_builtins::{BuiltinMember, ReceiverMode, TypePattern, builtin};

use crate::{
    Type,
    value::{DynamicObject, Value, record_codec},
};

use super::{NativeOwnedContext, import_receiver, indexed_iter, vector};

fn owned_member(symbol: &str) -> Option<&'static BuiltinMember> {
    let declaration = builtin("Vec")?;
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

fn from_array_member(symbol: &str) -> bool {
    builtin("Vec")
        .and_then(|declaration| declaration.member("from"))
        .is_some_and(|member| member.native_symbol == Some(symbol))
}

fn into_iter_member(symbol: &str) -> bool {
    builtin("Vec").is_some_and(|declaration| {
        declaration
            .members
            .iter()
            .any(|member| member.name == "into_iter" && member.native_symbol == Some(symbol))
    })
}

fn iter_member(symbol: &str) -> bool {
    builtin("Vec").is_some_and(|declaration| {
        declaration
            .members
            .iter()
            .any(|member| member.name == "iter" && member.native_symbol == Some(symbol))
    })
}

fn owned_output_member(symbol: &str) -> Option<&'static str> {
    builtin("Vec")?
        .members
        .iter()
        .find(|member| {
            matches!(member.name, "pop" | "remove" | "swap_remove")
                && member.native_symbol == Some(symbol)
        })
        .map(|member| member.name)
}

pub(crate) fn is_owned_symbol(symbol: &str) -> bool {
    from_array_member(symbol)
        || owned_member(symbol).is_some()
        || into_iter_member(symbol)
        || iter_member(symbol)
        || owned_output_member(symbol).is_some()
}

pub(crate) fn call_owned_symbol(
    symbol: &str,
    mut arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Option<Result<Value, String>> {
    if from_array_member(symbol) {
        return Some(super::collection_constructor::from_array(arguments));
    }
    if iter_member(symbol) {
        return Some(indexed_iter::borrow_with_context(&arguments, context));
    }
    if into_iter_member(symbol) {
        return Some((|| {
            if arguments.len() != 1 {
                return Err("Vec::into_iter expects one receiver".into());
            }
            let receiver = arguments.into_iter().next().expect("arity checked");
            match receiver {
                Value::Dynamic(object)
                    if crate::value::native_layouts::vec::matches(
                        object.descriptor().layout().rils_type(),
                    ) =>
                {
                    into_iterator_with_context(object, context)
                }
                value => match crate::iteration::into_iterator(value)? {
                    crate::iteration::IntoIteratorResult::Ready(iterator) => Ok(iterator),
                    crate::iteration::IntoIteratorResult::UserDefined(_) => {
                        Err("Vec::into_iter expects a Vec receiver".into())
                    }
                },
            }
        })());
    }
    if let Some(name) = owned_output_member(symbol) {
        return Some((|| {
            let expected = if name == "pop" { 1 } else { 2 };
            if arguments.len() != expected {
                return Err(format!("Vec::{name} expects {expected} runtime arguments"));
            }
            if !matches!(arguments.first(), Some(Value::Reference(reference)) if reference.mutable)
            {
                return Err(format!("Vec::{name} requires `&mut self`"));
            }
            let receiver = import_receiver(&arguments[0])?;
            let Value::Dynamic(object) = receiver else {
                return super::native::call_symbol(symbol, &arguments)
                    .unwrap_or_else(|| Err(format!("Vec::{name} has no runtime adapter")));
            };
            if !crate::value::native_layouts::vec::matches(object.descriptor().layout().rils_type())
            {
                return Err(format!("Vec::{name} requires a Vec receiver"));
            }
            let item_type = object
                .descriptor()
                .layout()
                .sequence_item()
                .ok_or("native Vec has no item layout")?
                .rils_type()
                .clone();
            let codec =
                record_codec::NativeRecordCodec::with_definitions(&context.structs, &context.enums);
            if name == "pop" {
                let item = object.with_mut(|payload| {
                    let length = payload.sequence_len()?;
                    if length == 0 {
                        Ok(None)
                    } else {
                        payload.take_sequence_item(length - 1).map(Some)
                    }
                })??;
                Ok(Value::Option {
                    value: item
                        .map(|item| codec.from_native(item).map(Rc::new))
                        .transpose()?,
                    element_type: Some(item_type),
                })
            } else {
                let position = index(&arguments, 1)?;
                let item = object.with_mut(|payload| {
                    if name == "remove" {
                        payload.take_sequence_item(position)
                    } else {
                        payload.swap_remove_sequence_item(position)
                    }
                })??;
                codec.from_native(item)
            }
        })());
    }
    let member = owned_member(symbol)?;
    Some((|| {
        let arity = member
            .signature
            .expect("owned method has a signature")
            .parameters
            .len()
            + 1;
        if arguments.len() != arity {
            return Err(format!(
                "native method `{symbol}` expects {arity} arguments, found {}",
                arguments.len()
            ));
        }
        if !matches!(arguments.first(), Some(Value::Reference(reference)) if reference.mutable) {
            return Err("Vec method requires `&mut self`".into());
        }
        let receiver = import_receiver(&arguments[0])?;
        let Value::Dynamic(object) = receiver else {
            return match member.name {
                "push" => vector::push_owned(arguments),
                "insert" => vector::insert_owned(arguments),
                name => Err(format!("owned native Vec method `{name}` is not supported")),
            };
        };
        if !crate::value::native_layouts::vec::matches(object.descriptor().layout().rils_type()) {
            return Err("native Vec method requires a Vec receiver".into());
        }
        let item_layout = object
            .descriptor()
            .layout()
            .sequence_item()
            .ok_or("native Vec has no item layout")?
            .clone();
        let value = arguments.pop().expect("arity checked");
        if !item_layout.rils_type().accepts(&value) {
            return Err(format!(
                "Vec expects {}, found {}",
                item_layout.rils_type(),
                value.type_name()
            ));
        }
        let mut codec =
            record_codec::NativeRecordCodec::with_definitions(&context.structs, &context.enums);
        let value = codec.into_native(value, item_layout)?;
        match member.name {
            "push" => object.with_mut(|payload| payload.push_sequence_item(value))??,
            "insert" => {
                let index = index(&arguments, 1)?;
                object.with_mut(|payload| payload.insert_sequence_item(index, value))??;
            }
            name => return Err(format!("owned native Vec method `{name}` is not supported")),
        }
        Ok(Value::Unit)
    })())
}

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let receiver = arguments.first()?;
    let Value::Dynamic(object) = import_receiver(receiver).ok()? else {
        return None;
    };
    if !crate::value::native_layouts::vec::matches(object.descriptor().layout().rils_type()) {
        return None;
    }
    let member = builtin("Vec")?
        .members
        .iter()
        .find(|member| member.native_symbol == Some(symbol))?;
    Some(call(member.name, arguments, &object))
}

pub(crate) fn into_iterator(object: DynamicObject) -> Result<Value, String> {
    into_iterator_with_context(
        object,
        &NativeOwnedContext {
            structs: Vec::new(),
            enums: Vec::new(),
        },
    )
}

pub(crate) fn into_iterator_with_context(
    object: DynamicObject,
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    crate::iteration::native_sequence_into_iterator(object, context)
}

pub(crate) fn extend(arguments: &[Value]) -> Option<Result<Value, String>> {
    let Some(receiver) = arguments.first() else {
        return Some(Err("missing Vec receiver".into()));
    };
    let Value::Dynamic(destination) = import_receiver(receiver).ok()? else {
        return None;
    };
    if !crate::value::native_layouts::vec::matches(destination.descriptor().layout().rils_type()) {
        return None;
    }
    Some((|| {
        if !matches!(receiver, Value::Reference(reference) if reference.mutable) {
            return Err("Vec::extend requires `&mut self`".into());
        }
        let source = import_receiver(arguments.get(1).ok_or("missing source Vec")?)?;
        match source {
            Value::Dynamic(source)
                if crate::value::native_layouts::vec::matches(
                    source.descriptor().layout().rils_type(),
                ) =>
            {
                if destination.same_storage(&source) {
                    return Err("Vec cannot extend itself".into());
                }
                if !destination
                    .descriptor()
                    .layout()
                    .compatible_with(source.descriptor().layout())
                {
                    return Err("Vec element types do not match".into());
                }
                destination.with_mut(|destination| {
                    destination
                        .sequence_borrows()?
                        .check_structural_mutation()?;
                    source.with_mut(|source| {
                        let items = source.take_all_sequence_items()?;
                        for item in items {
                            destination.push_sequence_item(item)?;
                        }
                        Ok::<_, String>(())
                    })??;
                    Ok::<_, String>(())
                })??;
            }
            Value::Vec(source) => {
                indexed_iter::reject_growth(&source)?;
                let layout = destination
                    .descriptor()
                    .layout()
                    .sequence_item()
                    .ok_or("native Vec has no item layout")?
                    .clone();
                let items = source
                    .elements
                    .borrow()
                    .iter()
                    .map(|slot| {
                        let value = slot
                            .value
                            .as_ref()
                            .ok_or("cannot extend from a partially moved Vec")?;
                        if !layout.rils_type().accepts(value) {
                            return Err(format!(
                                "Vec expects {}, found {}",
                                layout.rils_type(),
                                value.type_name()
                            ));
                        }
                        record_codec::into_native(value.clone_owned()?, layout.clone())
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                destination.with_mut(|destination| {
                    destination
                        .sequence_borrows()?
                        .check_structural_mutation()?;
                    for item in items {
                        destination.push_sequence_item(item)?;
                    }
                    Ok::<_, String>(())
                })??;
                source.elements.borrow_mut().clear();
            }
            _ => return Err("Vec::extend source must be Vec".into()),
        }
        Ok(Value::Unit)
    })())
}

fn call(name: &str, arguments: &[Value], object: &DynamicObject) -> Result<Value, String> {
    let item_layout = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("native Vec has no item layout")?
        .clone();
    let item_type = item_layout.rils_type().clone();
    let mutating = matches!(
        name,
        "push" | "pop" | "clear" | "truncate" | "insert" | "remove" | "swap_remove"
    );
    if mutating
        && !matches!(arguments.first(), Some(Value::Reference(reference)) if reference.mutable)
    {
        return Err("Vec method requires `&mut self`".into());
    }
    match name {
        "len" => Ok(crate::numeric::native_usize(
            object.with(|value| value.sequence_len())??,
        )),
        "is_empty" => Ok(Value::Bool(
            object.with(|value| value.sequence_len())?? == 0,
        )),
        "push" => {
            let value = item(arguments, 1, &item_layout)?;
            object.with_mut(|payload| payload.push_sequence_item(value))??;
            Ok(Value::Unit)
        }
        "pop" => {
            let item = object.with_mut(|payload| {
                let length = payload.sequence_len()?;
                if length == 0 {
                    Ok(None)
                } else {
                    payload.take_sequence_item(length - 1).map(Some)
                }
            })??;
            option(item, item_type)
        }
        "clear" => {
            object.with_mut(|payload| payload.clear_sequence())??;
            Ok(Value::Unit)
        }
        "truncate" => {
            let length = index(arguments, 1)?;
            object.with_mut(|payload| payload.truncate_sequence(length))??;
            Ok(Value::Unit)
        }
        "insert" => {
            let index = index(arguments, 1)?;
            let value = item(arguments, 2, &item_layout)?;
            object.with_mut(|payload| payload.insert_sequence_item(index, value))??;
            Ok(Value::Unit)
        }
        "remove" | "swap_remove" => {
            let index = index(arguments, 1)?;
            let value = object.with_mut(|payload| {
                if name == "remove" {
                    payload.take_sequence_item(index)
                } else {
                    payload.swap_remove_sequence_item(index)
                }
            })??;
            record_codec::from_native(value)
        }
        "contains" => {
            let needle = import_receiver(arguments.get(1).ok_or("missing Vec element")?)?;
            let length = object.with(|payload| payload.sequence_len())??;
            for index in 0..length {
                if crate::value::dynamic_sequence::borrowed_item(object, index)? == needle {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }
        "iter" => indexed_iter::borrow(arguments),
        "extend" => extend(arguments).expect("native Vec receiver was checked"),
        "into_iter" => into_iterator(object.clone()),
        _ => Err(format!("native Vec method `{name}` is not supported")),
    }
}

fn item(
    arguments: &[Value],
    index: usize,
    layout: &Rc<rils_value::DynamicLayout>,
) -> Result<rils_value::DynamicValue, String> {
    let value = arguments
        .get(index)
        .ok_or("missing Vec element")?
        .clone_owned()?;
    if !layout.rils_type().accepts(&value) {
        return Err(format!(
            "Vec expects {}, found {}",
            layout.rils_type(),
            value.type_name()
        ));
    }
    record_codec::into_native(value, layout.clone())
}

fn index(arguments: &[Value], index: usize) -> Result<usize, String> {
    arguments
        .get(index)
        .and_then(Value::as_usize)
        .ok_or("Vec index must be usize".into())
}

fn option(value: Option<rils_value::DynamicValue>, item_type: Type) -> Result<Value, String> {
    Ok(Value::Option {
        value: value
            .map(record_codec::from_native)
            .transpose()?
            .map(Rc::new),
        element_type: Some(item_type),
    })
}
