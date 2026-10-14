use std::{cell::Cell, rc::Rc};

use crate::{
    types::Type,
    value::{BorrowedIndexedIteratorValue, IndexedIteratorStorage, Value},
};

pub(super) fn borrow(arguments: &[Value]) -> Result<Value, String> {
    borrow_inner(arguments, false)
}

pub(super) fn borrow_map(arguments: &[Value]) -> Result<Value, String> {
    borrow_inner(arguments, true)
}

fn borrow_inner(arguments: &[Value], map_entries: bool) -> Result<Value, String> {
    let Some(receiver) = arguments.first().and_then(Value::as_reference) else {
        return Err("indexed iterator method requires a borrowed receiver".into());
    };
    let native = crate::value::native_receiver::NativeReceiver::from_value(&arguments[0])?;
    let codec = native
        .as_ref()
        .map(|object| object.codec())
        .unwrap_or_default();
    let (storage, length, element_type) = if let Some(object) = &native
        && let Type::Array { element, length } = object.descriptor().layout().rils_type()
    {
        object.with(|view| {
            if view.is_partially_moved()? {
                return Err("cannot iterate a partially moved array".into());
            }
            Ok::<_, String>(())
        })??;
        (
            IndexedIteratorStorage::NativeArray,
            *length,
            element.as_ref().clone(),
        )
    } else if let Some(object) = native
        && object.descriptor().layout().sequence_item().is_some()
    {
        let element_type = object
            .descriptor()
            .layout()
            .sequence_item()
            .expect("checked sequence")
            .rils_type()
            .clone();
        let (length, ledger) = object
            .with(|view| Ok::<_, String>((view.sequence_len()?, view.sequence_borrows()?)))??;
        let lease = ledger.begin_iteration()?;
        (
            IndexedIteratorStorage::Native {
                ledger,
                _lease: lease,
            },
            length,
            element_type,
        )
    } else {
        return Err("iter receiver is not a native indexed sequence".into());
    };
    Ok(Value::from_borrowed_iterator(
        BorrowedIndexedIteratorValue {
            source: receiver.clone(),
            storage,
            index: Cell::new(0),
            length,
            element_type,
            map_entries,
            native_codec: Some(Rc::new(codec)),
        },
    ))
}

pub(super) fn next(arguments: &[Value]) -> Result<Value, String> {
    let Some(receiver) = arguments.first().and_then(Value::as_reference) else {
        return Err("indexed iterator method requires a borrowed receiver".into());
    };
    if !receiver.mutable {
        return Err("Iter::next requires `&mut self`".into());
    }
    super::range::next(arguments)
}
