//! Execution iterators use the same native storage and operation registration as ranges.
use std::rc::Rc;

use super::{BorrowedIndexedIteratorValue, OwnedIteratorValue};
use crate::{
    Type, Value,
    value::{BytecodeIteratorValue, NativeChildren, NativeObject, NativeType, native_ops},
};

impl Value {
    pub fn from_owned_iterator(iterator: OwnedIteratorValue) -> Self {
        let ty = iterator
            .iterator_type
            .clone()
            .unwrap_or_else(|| Type::Named {
                name: "Iter".into(),
                arguments: vec![iterator.element_type.clone()],
            });
        let descriptor = NativeType::new::<OwnedIteratorValue>(ty)
            .with_children::<OwnedIteratorValue>()
            .register_method(native_ops::NEXT, |call| {
                call.receiver::<OwnedIteratorValue, _>(OwnedIteratorValue::next_option)?
            });
        Self::Native(
            NativeObject::new(Rc::new(descriptor), iterator)
                .expect("matching native iterator registration"),
        )
    }

    pub fn from_borrowed_iterator(iterator: BorrowedIndexedIteratorValue) -> Self {
        let descriptor =
            NativeType::new::<BorrowedIndexedIteratorValue>(iterator.iterator_type.clone())
                .with_children::<BorrowedIndexedIteratorValue>()
                .register_method(native_ops::NEXT, |call| {
                    call.receiver::<BorrowedIndexedIteratorValue, _>(
                        BorrowedIndexedIteratorValue::next_option,
                    )?
                });
        Self::Native(
            NativeObject::new(Rc::new(descriptor), iterator)
                .expect("matching native iterator registration"),
        )
    }

    pub fn from_bytecode_iterator(iterator: BytecodeIteratorValue) -> Self {
        let ty = iterator
            .storage
            .borrow()
            .read()
            .ok()
            .and_then(|value| Type::of_value(&value))
            .expect("script iterator retains its concrete receiver type");
        let descriptor =
            NativeType::new::<BytecodeIteratorValue>(ty).with_children::<BytecodeIteratorValue>();
        Self::Native(
            NativeObject::new(Rc::new(descriptor), iterator)
                .expect("matching native iterator registration"),
        )
    }
}

impl NativeChildren<Value> for BorrowedIndexedIteratorValue {
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&Value::from_reference(self.source.clone()));
    }
}

impl NativeChildren<Value> for BytecodeIteratorValue {
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        if let Ok(value) = self.storage.borrow().read() {
            visit(&value);
        }
    }
}

impl BorrowedIndexedIteratorValue {
    pub fn next_option(&self) -> Result<Value, String> {
        let ty = self.item_type();
        let codec = self.native_codec.as_deref().cloned().unwrap_or_default();
        let layout = codec.resolve_layout(&ty, &[])?;
        let output = crate::value::storage::PreparedOption::from_layout(layout, codec)?;
        output.construct(self.next()?)
    }
}
