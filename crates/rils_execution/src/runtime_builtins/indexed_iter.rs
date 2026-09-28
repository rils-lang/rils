use std::{cell::Cell, rc::Rc};

use crate::{
    types::Type,
    value::{BorrowedIndexedIteratorValue, IndexedIteratorStorage, IndexedStorage, Value},
};

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
    let Some(Value::Reference(receiver)) = arguments.first() else {
        return Err("indexed iterator method requires a borrowed receiver".into());
    };
    let (storage, length, element_type) = match receiver.read()? {
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
        Value::Dynamic(object)
            if crate::value::native_layouts::vec::matches(
                object.descriptor().layout().rils_type(),
            ) =>
        {
            let element_type = object
                .descriptor()
                .layout()
                .sequence_item()
                .ok_or("native Vec has no item layout")?
                .rils_type()
                .clone();
            if !object
                .descriptor()
                .layout()
                .sequence_item()
                .is_some_and(|item| item.is_copy() || item.rils_type() == &Type::String)
            {
                return Err("native borrowed iteration requires Copy or string elements".into());
            }
            let (length, ledger) = object.with(|payload| {
                Ok::<_, String>((payload.sequence_len()?, payload.sequence_borrows()?))
            })??;
            let lease = ledger.begin_iteration()?;
            (
                IndexedIteratorStorage::Native {
                    object,
                    _lease: lease,
                },
                length,
                element_type,
            )
        }
        _ => return Err("iter receiver is not an array or Vec".into()),
    };
    Ok(Value::BorrowedIndexedIterator(Rc::new(
        BorrowedIndexedIteratorValue {
            source: receiver.clone(),
            storage,
            index: Cell::new(0),
            length,
            element_type,
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
        Value::BorrowedIndexedIterator(iterator) => (
            iterator.next()?,
            Type::Reference {
                mutable: false,
                inner: Box::new(iterator.element_type.clone()),
            },
        ),
        Value::BorrowedMapIterator(iterator) => (iterator.next()?, iterator.item_type()),
        Value::BorrowedSetIterator(iterator) => (iterator.next()?, iterator.item_type()),
        _ => return Err("next receiver is not Iter".into()),
    };
    Ok(Value::Option {
        value: value.map(Rc::new),
        element_type: Some(item_type),
    })
}
