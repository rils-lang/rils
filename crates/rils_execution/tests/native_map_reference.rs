use std::rc::Rc;

use rils_execution::{Type, Value, value::ReferenceValue};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

#[test]
fn projected_native_map_fields_keep_entry_stable() {
    let key = DynamicLayout::copy_of::<i32>(Type::I32);
    let value = DynamicLayout::copy_of::<bool>(Type::Bool);
    let pair = DynamicLayout::aggregate(
        Type::Tuple(vec![Type::I32, Type::Bool]),
        vec![("0".into(), key.clone()), ("1".into(), value.clone())],
    )
    .unwrap();
    let map_type = Type::Named {
        name: "HashMap".into(),
        arguments: vec![Type::I32, Type::Bool],
    };
    let layout = DynamicLayout::sequence(map_type, pair.clone());
    let entry = DynamicValue::record(
        pair.clone(),
        vec![
            DynamicValue::from_rust(key.clone(), 7_i32).unwrap(),
            DynamicValue::from_rust(value.clone(), true).unwrap(),
        ],
    )
    .unwrap();
    let payload = DynamicValue::sequence(layout.clone(), vec![entry]).unwrap();
    let object: DynamicObject<Value> =
        DynamicObject::new(Rc::new(DynamicType::new(layout)), payload).unwrap();

    let key_ref =
        ReferenceValue::new_guarded_dynamic_indexed_field(object.clone(), 0, Some(0), false, None)
            .unwrap();
    let value_ref =
        ReferenceValue::new_guarded_dynamic_indexed_field(object.clone(), 0, Some(1), false, None)
            .unwrap();
    assert_eq!(key_ref.read().unwrap(), Value::from_i32(7));
    assert_eq!(value_ref.read().unwrap(), Value::Bool(true));

    let another = DynamicValue::record(
        pair,
        vec![
            DynamicValue::from_rust(key, 8_i32).unwrap(),
            DynamicValue::from_rust(value, false).unwrap(),
        ],
    )
    .unwrap();
    let blocked = object
        .with_mut(|payload| payload.push_sequence_item(another))
        .unwrap();
    assert!(blocked.unwrap_err().contains("referenced"));
    drop(key_ref);
    drop(value_ref);
    assert_eq!(object.with(DynamicValue::sequence_len).unwrap().unwrap(), 1);
}
