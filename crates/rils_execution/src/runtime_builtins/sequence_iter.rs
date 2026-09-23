use std::{cell::Cell, rc::Rc};

use rils_builtins::BuiltinId;

use crate::{
    types::Type,
    value::{BorrowedSequenceIteratorValue, SequenceValue, Value},
};

pub(super) fn reject_mutation(sequence: &SequenceValue) -> Result<(), String> {
    if sequence.active_iterators.get() > 0 {
        Err("cannot structurally mutate a sequence while an element is referenced".into())
    } else {
        Ok(())
    }
}

pub(super) fn reject_growth(sequence: &SequenceValue) -> Result<(), String> {
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

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    let Some(Value::Reference(receiver)) = arguments.first() else {
        return Err("sequence iterator method requires a borrowed receiver".into());
    };
    match id {
        BuiltinId::SequenceIter => {
            let (Value::Array(sequence) | Value::Vec(sequence)) = receiver.read()? else {
                return Err("iter receiver is not an array or Vec".into());
            };
            let length = sequence.elements.borrow().len();
            sequence
                .active_iterators
                .set(sequence.active_iterators.get() + 1);
            Ok(Value::BorrowedSequenceIterator(Rc::new(
                BorrowedSequenceIteratorValue {
                    source: receiver.clone(),
                    sequence: sequence.clone(),
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
        BuiltinId::SequenceIterNext => {
            if !receiver.mutable {
                return Err("Iter::next requires `&mut self`".into());
            }
            let Value::BorrowedSequenceIterator(iterator) = receiver.read()? else {
                return Err("next receiver is not Iter".into());
            };
            let value = iterator.next()?.map(Rc::new);
            Ok(Value::Option {
                value,
                element_type: Some(Type::Reference {
                    mutable: false,
                    inner: Box::new(iterator.element_type.clone()),
                }),
            })
        }
        _ => Err("unsupported sequence iterator operation".into()),
    }
}
