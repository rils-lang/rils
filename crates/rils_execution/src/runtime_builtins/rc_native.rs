//! Type-erased native storage for shared handles with arbitrary native payloads.

use std::rc::Rc;

use rils_builtins::BuiltinId;
use rils_stdlib::stdlib::rc::{ErasedRc, ErasedWeak};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

use crate::{Type, Value, value::record_codec::NativeRecordCodec};

use super::NativeOwnedContext;

#[derive(Clone, Copy)]
enum Operation {
    New,
    StrongCount,
    Downgrade,
    Upgrade,
    WeakStrongCount,
    WeakCount,
}

fn operation(symbol: &str) -> Option<Operation> {
    let (owner, method) = rils_builtins::native_member_owner(symbol)?;
    if method.builtin_id.is_some() || method.native_symbol != Some(symbol) {
        return None;
    }
    let rc = rils_builtins::builtin("Rc")?;
    let weak = rils_builtins::builtin("Weak")?;
    match (
        std::ptr::eq(owner, rc),
        std::ptr::eq(owner, weak),
        method.name,
    ) {
        (true, _, "new") => Some(Operation::New),
        (true, _, "strong_count") => Some(Operation::StrongCount),
        (true, _, "downgrade") => Some(Operation::Downgrade),
        (_, true, "upgrade") => Some(Operation::Upgrade),
        (_, true, "strong_count") => Some(Operation::WeakStrongCount),
        (_, true, "weak_count") => Some(Operation::WeakCount),
        _ => None,
    }
}

pub(super) fn is_owned_symbol(symbol: &str) -> bool {
    matches!(operation(symbol), Some(Operation::New))
}

pub(super) fn call_owned_symbol(
    symbol: &str,
    mut arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Option<Result<Value, String>> {
    if !is_owned_symbol(symbol) {
        return None;
    }
    Some((|| {
        if arguments.len() != 1 {
            return Err("Rc::new expects one value".into());
        }
        let value = arguments.pop().expect("checked argument count");
        let item_type = Type::of_value(&value).ok_or("Rc::new needs a concrete item type")?;
        let mut resolver = crate::value::record_layout::RecordLayoutResolver::with_enums(
            &context.structs,
            &context.enums,
        );
        let item_layout = resolver.resolve(&item_type)?;
        let native = NativeRecordCodec::with_definitions(&context.structs, &context.enums)
            .into_native(value, item_layout)?;
        let ty = Type::Named {
            name: "Rc".into(),
            arguments: vec![item_type],
        };
        let layout = DynamicLayout::of::<ErasedRc>(ty);
        native_value(
            layout.clone(),
            DynamicValue::from_rust(layout, ErasedRc(Rc::new(native)))?,
        )
    })())
}

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    if is_owned_symbol(symbol) {
        return Some(Err("Rc::new requires an owned native call".into()));
    }
    let operation = operation(symbol)?;
    if arguments.len() != 1 {
        return Some(Err(format!(
            "native method `{symbol}` expects one receiver, found {} arguments",
            arguments.len()
        )));
    }
    let receiver = arguments.first()?;
    let Value::Dynamic(object) = super::import_receiver(receiver).ok()? else {
        return None;
    };
    Some((|| {
        let ty = object.descriptor().layout().rils_type().clone();
        let Type::Named {
            arguments: types, ..
        } = &ty
        else {
            return Err("shared handle has no type argument".into());
        };
        let item_type = types
            .first()
            .ok_or("shared handle has no item type")?
            .clone();
        match operation {
            Operation::StrongCount => {
                let count = object.with(|value| {
                    value.with::<ErasedRc, _>(|handle| Rc::strong_count(&handle.0))
                })??;
                Ok(crate::numeric::native_usize(count))
            }
            Operation::Downgrade => {
                let weak = object.with(|value| {
                    value.with::<ErasedRc, _>(|handle| ErasedWeak(Rc::downgrade(&handle.0)))
                })??;
                let layout = DynamicLayout::of::<ErasedWeak>(Type::Named {
                    name: "Weak".into(),
                    arguments: vec![item_type],
                });
                native_value(layout.clone(), DynamicValue::from_rust(layout, weak)?)
            }
            Operation::Upgrade => {
                let strong = object.with(|value| {
                    value.with::<ErasedWeak, _>(|handle| handle.0.upgrade().map(ErasedRc))
                })??;
                let rc_layout = DynamicLayout::of::<ErasedRc>(Type::Named {
                    name: "Rc".into(),
                    arguments: vec![item_type],
                });
                let option_layout = DynamicLayout::option(rc_layout.clone())?;
                let value = match strong {
                    Some(strong) => DynamicValue::some(
                        option_layout.clone(),
                        DynamicValue::from_rust(rc_layout, strong)?,
                    )?,
                    None => DynamicValue::none(option_layout.clone())?,
                };
                native_value(option_layout, value)
            }
            Operation::WeakStrongCount => {
                let count = object.with(|value| {
                    value.with::<ErasedWeak, _>(|handle| handle.0.strong_count())
                })??;
                Ok(crate::numeric::native_usize(count))
            }
            Operation::WeakCount => {
                let count = object
                    .with(|value| value.with::<ErasedWeak, _>(|handle| handle.0.weak_count()))??;
                Ok(crate::numeric::native_usize(count))
            }
            _ => unreachable!(),
        }
    })())
}

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Option<Result<Value, String>> {
    let symbol = id.canonical_path()?;
    call_symbol(symbol, arguments)
}

fn native_value(layout: Rc<DynamicLayout>, value: DynamicValue) -> Result<Value, String> {
    DynamicObject::new(Rc::new(DynamicType::new(layout)), value).map(Value::Dynamic)
}
