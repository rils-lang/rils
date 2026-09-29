//! Type-erased native cell operations that retain concrete item layouts.

use std::{cell::RefCell, rc::Rc};

use rils_stdlib::stdlib::cell::{ErasedCell, ErasedRefCell};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

use crate::{
    Type, Value,
    value::{ReferenceValue, record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver},
};

use super::NativeOwnedContext;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    CellNew,
    CellGet,
    CellSet,
    CellReplace,
    RefCellNew,
    RefCellBorrow,
    RefCellBorrowMut,
    RefCellReplace,
}

fn operation(path: &str) -> Option<Operation> {
    let (owner, method) = rils_builtins::native_member_owner(path)?;
    if method.builtin_id.is_some() || method.native_symbol != Some(path) {
        return None;
    }
    let cell = rils_builtins::builtin("Cell")?;
    let ref_cell = rils_builtins::builtin("RefCell")?;
    match (
        std::ptr::eq(owner, cell),
        std::ptr::eq(owner, ref_cell),
        method.name,
    ) {
        (true, _, "new") => Some(Operation::CellNew),
        (true, _, "get") => Some(Operation::CellGet),
        (true, _, "set") => Some(Operation::CellSet),
        (true, _, "replace") => Some(Operation::CellReplace),
        (_, true, "new") => Some(Operation::RefCellNew),
        (_, true, "borrow") => Some(Operation::RefCellBorrow),
        (_, true, "borrow_mut") => Some(Operation::RefCellBorrowMut),
        (_, true, "replace") => Some(Operation::RefCellReplace),
        _ => None,
    }
}

pub(super) fn is_owned_symbol(path: &str) -> bool {
    operation(path).is_some()
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
    let operation = operation(path)?;
    Some((|| {
        if matches!(operation, Operation::CellNew | Operation::RefCellNew) {
            if arguments.len() != 1 {
                return Err("Cell::new expects one value".into());
            }
            let value = arguments.pop().expect("checked argument count");
            let ty = Type::of_value(&value).ok_or("Cell::new needs a concrete item type")?;
            let item_layout =
                RecordLayoutResolver::with_enums(&context.structs, &context.enums).resolve(&ty)?;
            let native = NativeRecordCodec::with_definitions(&context.structs, &context.enums)
                .into_native(value, item_layout.clone())?;
            let (layout, payload) = if operation == Operation::CellNew {
                let layout = DynamicLayout::of::<ErasedCell>(Type::Named {
                    name: "Cell".into(),
                    arguments: vec![ty],
                });
                let payload =
                    DynamicValue::from_rust(layout.clone(), ErasedCell(RefCell::new(native)))?;
                (layout, payload)
            } else {
                let item_descriptor = Rc::new(DynamicType::<()>::new(item_layout));
                let layout = DynamicLayout::of::<ErasedRefCell>(Type::Named {
                    name: "RefCell".into(),
                    arguments: vec![ty],
                });
                let payload = DynamicValue::from_rust(
                    layout.clone(),
                    ErasedRefCell {
                        value: DynamicObject::new_shared(item_descriptor, native)?,
                        references: std::cell::Cell::new(0),
                    },
                )?;
                (layout, payload)
            };
            return native_value(layout, payload);
        }
        if matches!(
            operation,
            Operation::RefCellBorrow | Operation::RefCellBorrowMut | Operation::RefCellReplace
        ) {
            let expected = if operation == Operation::RefCellReplace {
                2
            } else {
                1
            };
            if arguments.len() != expected {
                return Err("RefCell method received the wrong number of arguments".into());
            }
            let guard = match &arguments[0] {
                Value::Reference(reference) => Some(reference.clone()),
                _ => None,
            };
            let receiver = super::import_receiver(&arguments[0])?;
            let Value::Dynamic(object) = receiver else {
                return Err("expected RefCell receiver".into());
            };
            let Type::Named {
                name,
                arguments: types,
            } = object.descriptor().layout().rils_type()
            else {
                return Err("expected RefCell receiver".into());
            };
            if name != "RefCell" {
                return Err("expected RefCell receiver".into());
            }
            let item_ty = types.first().ok_or("RefCell has no item type")?;
            return match operation {
                Operation::RefCellBorrow | Operation::RefCellBorrowMut => {
                    let reference = ReferenceValue::new_dynamic_cell(
                        object,
                        operation == Operation::RefCellBorrowMut,
                        guard,
                        context.structs.clone(),
                        context.enums.clone(),
                    )?;
                    Ok(Value::Reference(Rc::new(reference)))
                }
                Operation::RefCellReplace => {
                    let layout = RecordLayoutResolver::with_enums(&context.structs, &context.enums)
                        .resolve(item_ty)?;
                    let mut codec =
                        NativeRecordCodec::with_definitions(&context.structs, &context.enums);
                    let native = codec
                        .into_native(arguments.pop().expect("checked argument count"), layout)?;
                    let old = object.with(|payload| {
                        payload.with::<ErasedRefCell, _>(|cell| {
                            if cell.references.get() > 0 {
                                return Err("cannot replace RefCell while borrowed".to_owned());
                            }
                            cell.value
                                .with_mut(|value| std::mem::replace(value, native))
                        })
                    })???;
                    codec.from_native(old)
                }
                _ => unreachable!(),
            };
        }
        if arguments.len()
            != if operation == Operation::CellGet {
                1
            } else {
                2
            }
        {
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
        match operation {
            Operation::CellGet => {
                if !item_layout.is_copy() {
                    return Err("Cell::get requires a Copy value".into());
                }
                let item = object.with(|payload| {
                    payload.with::<ErasedCell, _>(|cell| cell.0.borrow().copy_path(&[]))
                })???;
                codec.from_native(item)
            }
            Operation::CellSet | Operation::CellReplace => {
                let value = arguments.pop().expect("checked argument count");
                let native = codec.into_native(value, item_layout)?;
                let old = object.with(|payload| {
                    payload.with::<ErasedCell, _>(|cell| {
                        std::mem::replace(&mut *cell.0.borrow_mut(), native)
                    })
                })??;
                if operation == Operation::CellSet {
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
