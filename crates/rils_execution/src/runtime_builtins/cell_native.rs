//! Type-erased native cell operations that retain concrete item layouts.

use std::{cell::RefCell, rc::Rc};

use rils_builtins::BuiltinId;
use rils_stdlib::stdlib::cell::ErasedCell;
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

use crate::{
    Type, Value,
    value::{record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver},
};

use super::NativeOwnedContext;

fn symbol(id: BuiltinId) -> &'static str {
    id.canonical_path().expect("Cell built-in has a path")
}

pub(super) fn is_owned_symbol(path: &str) -> bool {
    [
        BuiltinId::CellNew,
        BuiltinId::CellGet,
        BuiltinId::CellSet,
        BuiltinId::CellReplace,
    ]
    .into_iter()
    .any(|id| symbol(id) == path)
}

pub(super) fn call_symbol(path: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    is_owned_symbol(path).then(|| {
        let _ = arguments;
        Err("Cell methods require an owned native call".into())
    })
}

pub(super) fn call_owned_symbol(
    path: &str,
    mut arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Option<Result<Value, String>> {
    let id = [
        BuiltinId::CellNew,
        BuiltinId::CellGet,
        BuiltinId::CellSet,
        BuiltinId::CellReplace,
    ]
    .into_iter()
    .find(|id| symbol(*id) == path)?;
    Some((|| {
        if id == BuiltinId::CellNew {
            if arguments.len() != 1 {
                return Err("Cell::new expects one value".into());
            }
            let value = arguments.pop().expect("checked argument count");
            let ty = Type::of_value(&value).ok_or("Cell::new needs a concrete item type")?;
            let item_layout =
                RecordLayoutResolver::with_enums(&context.structs, &context.enums).resolve(&ty)?;
            let native = NativeRecordCodec::with_definitions(&context.structs, &context.enums)
                .into_native(value, item_layout)?;
            let layout = DynamicLayout::of::<ErasedCell>(Type::Named {
                name: "Cell".into(),
                arguments: vec![ty],
            });
            let payload =
                DynamicValue::from_rust(layout.clone(), ErasedCell(RefCell::new(native)))?;
            return native_value(layout, payload);
        }
        if arguments.len() != if id == BuiltinId::CellGet { 1 } else { 2 } {
            return Err("Cell method received the wrong number of arguments".into());
        }
        let receiver = super::import_receiver(&arguments[0])?;
        let Value::Dynamic(object) = receiver else {
            return Err("expected Cell receiver".into());
        };
        let Type::Named {
            name,
            arguments: types,
        } = object.descriptor().layout().rils_type()
        else {
            return Err("expected Cell receiver".into());
        };
        if name != "Cell" {
            return Err("expected Cell receiver".into());
        }
        let item_ty = types.first().ok_or("Cell has no item type")?;
        let item_layout =
            RecordLayoutResolver::with_enums(&context.structs, &context.enums).resolve(item_ty)?;
        let mut codec = NativeRecordCodec::with_definitions(&context.structs, &context.enums);
        match id {
            BuiltinId::CellGet => {
                if !item_layout.is_copy() {
                    return Err("Cell::get requires a Copy value".into());
                }
                let item = object.with(|payload| {
                    payload.with::<ErasedCell, _>(|cell| cell.0.borrow().copy_path(&[]))
                })???;
                codec.from_native(item)
            }
            BuiltinId::CellSet | BuiltinId::CellReplace => {
                let value = arguments.pop().expect("checked argument count");
                let native = codec.into_native(value, item_layout)?;
                let old = object.with(|payload| {
                    payload.with::<ErasedCell, _>(|cell| {
                        std::mem::replace(&mut *cell.0.borrow_mut(), native)
                    })
                })??;
                if id == BuiltinId::CellSet {
                    Ok(Value::Unit)
                } else {
                    codec.from_native(old)
                }
            }
            _ => unreachable!(),
        }
    })())
}

fn native_value(layout: Rc<DynamicLayout>, value: DynamicValue) -> Result<Value, String> {
    DynamicObject::new(Rc::new(DynamicType::new(layout)), value).map(Value::Dynamic)
}
