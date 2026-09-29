use std::rc::Rc;

use rils_execution::{Type, Value, value::ReferenceValue};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

#[test]
fn borrowed_native_set_item_keeps_sequence_stable() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let set_type = Type::Named {
        name: "HashSet".into(),
        arguments: vec![Type::I32],
    };
    let layout = DynamicLayout::sequence(set_type, item.clone());
    let payload = DynamicValue::sequence(
        layout.clone(),
        vec![DynamicValue::from_rust(item.clone(), 7_i32).unwrap()],
    )
    .unwrap();
    let object: DynamicObject<Value> =
        DynamicObject::new(Rc::new(DynamicType::new(layout)), payload).unwrap();

    let reference =
        ReferenceValue::new_guarded_dynamic_indexed_element(object.clone(), 0, false, None)
            .unwrap();
    assert_eq!(reference.read().unwrap(), Value::from_i32(7));
    let blocked = object
        .with_mut(|payload| {
            payload.push_sequence_item(DynamicValue::from_rust(item.clone(), 8_i32).unwrap())
        })
        .unwrap();
    assert!(blocked.unwrap_err().contains("referenced"));

    drop(reference);
    object
        .with_mut(|payload| {
            payload.push_sequence_item(DynamicValue::from_rust(item, 8_i32).unwrap())
        })
        .unwrap()
        .unwrap();
    assert_eq!(object.with(DynamicValue::sequence_len).unwrap().unwrap(), 2);
}
