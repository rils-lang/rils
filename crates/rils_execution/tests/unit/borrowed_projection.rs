use super::*;
use crate::{
    environment::StorageSlot,
    value::{ReferenceValue, native_instance, record_codec::NativeRecordCodec},
};
use rils_stdlib::storage::EntryShape;
use rils_value::DynamicValue;
use std::cell::RefCell;

#[test]
fn reordered_equal_typed_parameters_keep_declared_identity_and_reference_leases() {
    let schema = EntrySchema {
        owner: "HashMap",
        parameters: 2,
        entry: EntryShape::Tuple(&[EntryShape::Parameter(1), EntryShape::Parameter(0)]),
    };
    let ty = Type::Named {
        name: "HashMap".into(),
        arguments: vec![Type::I32, Type::I32],
    };
    let context = super::super::NativeOwnedContext::default();
    let layout = context.layout(&ty).unwrap();
    let entry_layout = layout.sequence_item().unwrap().clone();
    let field_layout = entry_layout.record_fields().unwrap()[0].layout_handle();
    let entry = DynamicValue::record(
        entry_layout,
        vec![
            DynamicValue::from_rust(field_layout.clone(), 20i32).unwrap(),
            DynamicValue::from_rust(field_layout, 10i32).unwrap(),
        ],
    )
    .unwrap();
    let source = native_instance::from_native(
        DynamicValue::sequence(layout, vec![entry]).unwrap(),
        Rc::new(NativeRecordCodec::new()),
    )
    .unwrap();
    let Value::Dynamic(object) = &source else {
        unreachable!()
    };
    let object = object.clone();
    let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    slot.borrow_mut().initialize(source);
    let source = Value::from_reference(Rc::new(ReferenceValue::new_storage(slot, true)));
    let iterator = borrow(std::slice::from_ref(&source), &schema, "iter").unwrap();
    let Value::Native(iterator) = iterator else {
        unreachable!()
    };
    let item = iterator
        .with::<BorrowedIndexedIteratorValue, _>(|iterator| iterator.next())
        .unwrap()
        .unwrap()
        .unwrap();
    let values = item
        .with_native_view(|view| {
            (0..2)
                .map(|index| {
                    view.field(index)?
                        .with_rust::<Rc<ReferenceValue>, _>(|reference| {
                            reference.read().unwrap().get_cloned::<i32>().unwrap()
                        })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .unwrap()
        .unwrap();
    assert_eq!(values, [10, 20]);
    assert!(
        iterator
            .with::<BorrowedIndexedIteratorValue, _>(|iterator| iterator.next())
            .unwrap()
            .unwrap()
            .is_none()
    );
    drop(iterator);
    assert!(
        object
            .with_mut(|value| value.clear_sequence())
            .unwrap()
            .is_err()
    );
    drop(item);
    object
        .with_mut(|value| value.clear_sequence())
        .unwrap()
        .unwrap();
}
