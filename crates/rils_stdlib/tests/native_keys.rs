use rils_native::NativeKey;
use rils_syntax::{IntegerType, Type};
use rils_value::{DynamicLayout, DynamicValue};

fn key(value: &DynamicValue) -> NativeKey {
    rils_stdlib::native::registry().key(value.view()).unwrap()
}

#[test]
fn native_keys_preserve_signed_order_and_string_contents() {
    let integer = DynamicLayout::copy_of::<i64>(Type::Integer(IntegerType::I64));
    let low = DynamicValue::from_rust(integer.clone(), -12_i64).unwrap();
    let high = DynamicValue::from_rust(integer, 7_i64).unwrap();
    assert!(key(&low) < key(&high));

    let text = DynamicLayout::of::<rils_stdlib::stdlib::string::String>(Type::String);
    let first = DynamicValue::from_rust(
        text.clone(),
        rils_stdlib::stdlib::string::String::from("alpha".to_owned()),
    )
    .unwrap();
    let second = DynamicValue::from_rust(
        text,
        rils_stdlib::stdlib::string::String::from("alpha".to_owned()),
    )
    .unwrap();
    assert_eq!(key(&first), key(&second));
    assert_eq!(key(&first), NativeKey::String("alpha".into()));
}

#[test]
fn native_keys_follow_composed_layouts() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let option = DynamicLayout::option(item.clone()).unwrap();
    let absent = DynamicValue::none(option.clone()).unwrap();
    let present = DynamicValue::some(
        option,
        DynamicValue::from_rust(item.clone(), 9_i32).unwrap(),
    )
    .unwrap();
    assert_eq!(key(&absent), NativeKey::None);
    assert_eq!(
        key(&present),
        NativeKey::Some(Box::new(NativeKey::Signed(9)))
    );

    let pair_type = Type::Tuple(vec![Type::I32, Type::Bool]);
    let pair = DynamicLayout::aggregate(
        pair_type,
        vec![
            ("0".into(), item.clone()),
            ("1".into(), DynamicLayout::copy_of::<bool>(Type::Bool)),
        ],
    )
    .unwrap();
    let value = DynamicValue::record(
        pair,
        vec![
            DynamicValue::from_rust(item, 9_i32).unwrap(),
            DynamicValue::from_rust(DynamicLayout::copy_of::<bool>(Type::Bool), true).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        key(&value),
        NativeKey::Fields(vec![NativeKey::Signed(9), NativeKey::Bool(true)])
    );

    let alternatives = vec![
        DynamicLayout::copy_of::<i32>(Type::I32),
        DynamicLayout::copy_of::<i32>(Type::I32),
    ];
    let choice = DynamicLayout::variant(Type::named("Choice"), alternatives.clone()).unwrap();
    let left = DynamicValue::variant(
        choice.clone(),
        0,
        DynamicValue::from_rust(alternatives[0].clone(), 9_i32).unwrap(),
    )
    .unwrap();
    let right = DynamicValue::variant(
        choice,
        1,
        DynamicValue::from_rust(alternatives[1].clone(), 9_i32).unwrap(),
    )
    .unwrap();
    assert_ne!(key(&left), key(&right));
}

#[test]
fn native_key_rejects_unregistered_leaf() {
    let layout = DynamicLayout::of::<f64>(Type::F64);
    let value = DynamicValue::from_rust(layout, 1.5_f64).unwrap();
    assert!(
        rils_stdlib::native::registry()
            .key(value.view())
            .unwrap_err()
            .contains("no registered native key")
    );
}
