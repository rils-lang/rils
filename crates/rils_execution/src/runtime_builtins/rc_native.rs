//! Type-erased native storage for shared handles with arbitrary native payloads.

use std::rc::Rc;

use rils_builtins::BuiltinId;
use rils_stdlib::stdlib::rc::{ErasedRc, ErasedWeak};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

use crate::{Type, Value, value::record_codec::NativeRecordCodec};

use super::NativeOwnedContext;

fn matches(id: BuiltinId, symbol: &str) -> bool {
    id.canonical_path().is_some_and(|path| path == symbol)
}

pub(super) fn is_owned_symbol(symbol: &str) -> bool {
    matches(BuiltinId::RcNew, symbol)
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
    let id = [
        BuiltinId::RcStrongCount,
        BuiltinId::RcDowngrade,
        BuiltinId::WeakUpgrade,
        BuiltinId::WeakStrongCount,
        BuiltinId::WeakWeakCount,
    ]
    .into_iter()
    .find(|id| matches(*id, symbol))?;
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
        match id {
            BuiltinId::RcStrongCount => {
                let count = object.with(|value| {
                    value.with::<ErasedRc, _>(|handle| Rc::strong_count(&handle.0))
                })??;
                Ok(crate::numeric::native_usize(count))
            }
            BuiltinId::RcDowngrade => {
                let weak = object.with(|value| {
                    value.with::<ErasedRc, _>(|handle| ErasedWeak(Rc::downgrade(&handle.0)))
                })??;
                let layout = DynamicLayout::of::<ErasedWeak>(Type::Named {
                    name: "Weak".into(),
                    arguments: vec![item_type],
                });
                native_value(layout.clone(), DynamicValue::from_rust(layout, weak)?)
            }
            BuiltinId::WeakUpgrade => {
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
            BuiltinId::WeakStrongCount => {
                let count = object.with(|value| {
                    value.with::<ErasedWeak, _>(|handle| handle.0.strong_count())
                })??;
                Ok(crate::numeric::native_usize(count))
            }
            BuiltinId::WeakWeakCount => {
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
