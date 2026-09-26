use std::{cell::Cell, rc::Rc};

use crate::{
    types::Type,
    value::{BorrowedIndexedIteratorValue, IndexedStorage, Value},
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
    let (Value::Array(sequence) | Value::Vec(sequence)) = receiver.read()? else {
        return Err("iter receiver is not an array or Vec".into());
    };
    let length = sequence.elements.borrow().len();
    sequence
        .active_iterators
        .set(sequence.active_iterators.get() + 1);
    Ok(Value::BorrowedIndexedIterator(Rc::new(
        BorrowedIndexedIteratorValue {
            source: receiver.clone(),
            storage: sequence.clone(),
            index: Cell::new(0),
            length,
            element_type: sequence
                .element_type
                .borrow()
                .clone()
                .unwrap_or(Type::Unknown),
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
