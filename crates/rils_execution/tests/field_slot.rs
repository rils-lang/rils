use std::{cell::Cell, rc::Rc};

use rils_execution::{
    Type, Value,
    value::{
        DynamicObject, FieldSlot, record_codec::NativeRecordCodec,
        record_layout::RecordLayoutResolver,
    },
};
use rils_value::{DynamicType, DynamicValue};

#[test]
fn moved_slots_retain_operations_without_retaining_the_payload() {
    let ty = Type::Option(Box::new(Type::String));
    let layout = RecordLayoutResolver::new(&[]).resolve(&ty).unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let declaration = Rc::new(DynamicType::new(layout.clone()).register_owned_operation(
        "probe",
        move |_| {
            observed.set(observed.get() + 1);
            Ok(Value::from_i32(37))
        },
    ));
    let child = NativeRecordCodec::new()
        .into_native(
            Value::from_string("owned"),
            layout.option_item().unwrap().clone(),
        )
        .unwrap();
    let payload = DynamicValue::some(layout, child).unwrap();
    let initial = Value::Dynamic(DynamicObject::new(declaration.clone(), payload).unwrap());
    let mut slot = FieldSlot::new(ty, initial);

    let Value::Dynamic(moved) = slot.value.take().unwrap() else {
        unreachable!()
    };
    assert!(
        moved.into_value().is_ok(),
        "the slot must retain metadata without sharing owned storage"
    );
    assert!(slot.assign(Value::Bool(true)).is_err());
    assert!(slot.value.is_none());
    slot.assign(Value::Option {
        value: None,
        element_type: None,
    })
    .unwrap();
    assert!(slot.assign(Value::from_i32(1)).is_err());
    assert_eq!(slot.value.as_ref().unwrap().as_option().unwrap().0, None);

    let Value::Dynamic(replacement) = slot.value.take().unwrap() else {
        panic!("replacement lost its native layout")
    };
    assert!(Rc::ptr_eq(&declaration, &replacement.descriptor_handle()));
    assert_eq!(replacement.call_owned("probe").unwrap().as_i32(), Some(37));
    assert_eq!(calls.get(), 1);

    let mut copied_metadata = slot.clone();
    copied_metadata
        .assign(Value::Option {
            value: None,
            element_type: None,
        })
        .unwrap();
    let Value::Dynamic(value) = copied_metadata.value.take().unwrap() else {
        unreachable!()
    };
    assert!(Rc::ptr_eq(&declaration, &value.descriptor_handle()));
}
