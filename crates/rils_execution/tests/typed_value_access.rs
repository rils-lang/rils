use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    RilsHostType, Type, Value,
    environment::StorageSlot,
    value::{DynamicObject, ReferenceValue, native_i32},
};
use rils_value::{DynamicLayout, DynamicType, DynamicValue};

#[test]
fn generated_mapping_reads_standalone_composed_and_referenced_payloads() {
    let layout = DynamicLayout::copy_of::<i32>(Type::I32);
    let payload = DynamicValue::from_rust(layout.clone(), 42_i32).unwrap();
    let composed: Value = DynamicObject::new(Rc::new(DynamicType::new(layout)), payload)
        .unwrap()
        .into();
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(native_i32(42).into());
    let slot = Rc::new(RefCell::new(slot));
    let reference = Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true)));
    for value in [Value::from(native_i32(42)), composed, reference] {
        assert_eq!(value.with_ref::<i32, _>(|number| *number).unwrap(), 42);
        assert_eq!(value.get_cloned::<i32>().unwrap(), 42);
        assert!(value.get_cloned::<u32>().is_err());
    }
    assert!(slot.borrow_mut().take().is_ok());
}

struct Payload(String);
impl RilsHostType for Payload {
    type Native = Self;
    fn from_native(value: Self) -> Self {
        value
    }
    fn as_native_ref(value: &Self) -> &Self {
        value
    }
}

#[test]
fn generic_access_borrows_non_clone_host_payloads_without_an_accessor_table() {
    use rils_execution::value::{NativeObject, NativeType};
    let native = NativeObject::new(
        Rc::new(NativeType::new::<Payload>(Type::named("Payload"))),
        Payload("owned".into()),
    )
    .unwrap();
    let value = Value::from(native);
    assert_eq!(
        value
            .with_ref::<Payload, _>(|payload| payload.0.len())
            .unwrap(),
        5
    );
    assert!(value.get_cloned::<String>().is_err());
}
