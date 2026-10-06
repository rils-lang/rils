use std::{cell::Cell, rc::Rc};

use crate::{
    types::Type,
    value::{
        BorrowedIndexedIteratorValue, IndexedIteratorStorage, IndexedStorage, Value,
        record_codec::NativeRecordCodec,
    },
};

use super::NativeOwnedContext;

pub(super) fn reject_mutation(sequence: &IndexedStorage) -> Result<(), String> {
    if sequence.active_iterators.get() > 0 {
        Err("cannot structurally mutate a sequence while an element is referenced".into())
    } else {
        Ok(())
    }
}

pub(super) fn reject_growth(sequence: &IndexedStorage) -> Result<(), String> {
    if sequence.active_iterators.get() > 0
        || sequence
            .elements
            .borrow()
            .iter()
            .any(|slot| slot.references > 0)
    {
        Err("cannot structurally mutate a sequence while an element is referenced".into())
    } else {
        Ok(())
    }
}

pub(super) fn borrow(arguments: &[Value]) -> Result<Value, String> {
    borrow_inner(arguments, false, None)
}

pub(super) fn borrow_with_context(
    arguments: &[Value],
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    borrow_inner(arguments, false, Some(context))
}

pub(super) fn borrow_map_with_context(
    arguments: &[Value],
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    borrow_inner(arguments, true, Some(context))
}

fn borrow_inner(
    arguments: &[Value],
    map_entries: bool,
    context: Option<&NativeOwnedContext>,
) -> Result<Value, String> {
    let Some(Value::Reference(receiver)) = arguments.first() else {
        return Err("indexed iterator method requires a borrowed receiver".into());
    };
    let native = crate::value::native_receiver::NativeReceiver::from_value(&Value::Reference(
        receiver.clone(),
    ))?;
    let (storage, length, element_type) = if let Some(object) = native
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
        match receiver.read()? {
            Value::Array(sequence) | Value::Vec(sequence) => {
                let length = sequence.elements.borrow().len();
                sequence
                    .active_iterators
                    .set(sequence.active_iterators.get() + 1);
                let element_type = sequence
                    .element_type
                    .borrow()
                    .clone()
                    .unwrap_or(Type::Unknown);
                (
                    IndexedIteratorStorage::Legacy(sequence),
                    length,
                    element_type,
                )
            }
            _ => return Err("iter receiver is not an indexed sequence".into()),
        }
    };
    Ok(Value::BorrowedIndexedIterator(Rc::new(
        BorrowedIndexedIteratorValue {
            source: receiver.clone(),
            storage,
            index: Cell::new(0),
            length,
            element_type,
            map_entries,
            native_codec: context.map(|context| {
                Rc::new(NativeRecordCodec::with_definitions(
                    &context.structs,
                    &context.enums,
                ))
            }),
        },
    )))
}

pub(super) fn next(arguments: &[Value]) -> Result<Value, String> {
    let Some(Value::Reference(receiver)) = arguments.first() else {
        return Err("indexed iterator method requires a borrowed receiver".into());
    };
    if !receiver.mutable {
        return Err("Iter::next requires `&mut self`".into());
    }
    let (value, item_type) = match receiver.read()? {
        Value::OwnedIterator(iterator) => (iterator.next()?, iterator.element_type.clone()),
        Value::BorrowedIndexedIterator(iterator) => (iterator.next()?, iterator.item_type()),
        Value::BorrowedMapIterator(iterator) => (iterator.next()?, iterator.item_type()),
        Value::BorrowedSetIterator(iterator) => (iterator.next()?, iterator.item_type()),
        _ => return Err("next receiver is not Iter".into()),
    };
    Ok(Value::Option {
        value: value.map(Rc::new),
        element_type: Some(item_type),
    })
}
