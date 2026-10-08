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

#[test]
fn generated_integer_keys_support_raw_and_wrapped_storage_at_every_width() {
    use rils_stdlib::stdlib::integer::Number;
    use rils_value::NativeLeafRef;
    let registry = rils_stdlib::native::registry();
    macro_rules! check {
        ($ty:ty, $kind:ident, $identity:ident, $wide:ty) => {
            for raw in [<$ty>::MIN, 0, <$ty>::MAX] {
                let ty = Type::Integer(IntegerType::$kind);
                let wrapped = Number::<$ty>(raw);
                let expected = NativeKey::$identity(raw as $wide);
                let stored =
                    DynamicValue::from_rust(DynamicLayout::copy_of::<$ty>(ty.clone()), raw)
                        .unwrap();
                assert_eq!(
                    rils_stdlib::stdlib::collections::native_heap_key(stored.view()),
                    Ok(expected.clone())
                );
                for leaf in [
                    NativeLeafRef::from_rust(&raw, ty.clone()),
                    NativeLeafRef::from_rust(&wrapped, ty),
                ] {
                    assert_eq!(registry.key_leaf(&leaf).unwrap(), expected);
                    assert_eq!(registry.ordered_key_leaf(&leaf).unwrap(), expected);
                }
            }
        };
    }
    check!(i8, I8, Signed, i128);
    check!(i16, I16, Signed, i128);
    check!(i32, I32, Signed, i128);
    check!(i64, I64, Signed, i128);
    check!(i128, I128, Signed, i128);
    check!(isize, Isize, Signed, i128);
    check!(u8, U8, Unsigned, u128);
    check!(u16, U16, Unsigned, u128);
    check!(u32, U32, Unsigned, u128);
    check!(u64, U64, Unsigned, u128);
    check!(u128, U128, Unsigned, u128);
    check!(usize, Usize, Unsigned, u128);
}

#[test]
fn key_registrations_reject_wrong_physical_types_and_unsupported_ordering() {
    use rils_value::NativeLeafRef;
    let registry = rils_stdlib::native::registry();
    assert!(
        registry
            .key_leaf(&NativeLeafRef::from_rust(&42_u32, Type::I32))
            .is_err()
    );
    assert_eq!(
        registry.key_leaf(&NativeLeafRef::from_rust(&(), Type::Unit)),
        Ok(NativeKey::Unit)
    );
    assert!(
        registry
            .ordered_key_leaf(&NativeLeafRef::from_rust(&(), Type::Unit))
            .is_err()
    );
    assert!(
        registry
            .ordered_key_leaf(&NativeLeafRef::from_rust(&f64::NAN, Type::F64))
            .is_err()
    );
    let option = DynamicValue::none(
        DynamicLayout::option(DynamicLayout::copy_of::<i32>(Type::I32)).unwrap(),
    )
    .unwrap();
    assert_eq!(registry.key(option.view()), Ok(NativeKey::None));
    assert!(registry.ordered_key(option.view()).is_err());
    let boolean =
        DynamicValue::from_rust(DynamicLayout::copy_of::<bool>(Type::Bool), true).unwrap();
    assert_eq!(
        registry.ordered_key(boolean.view()),
        Ok(NativeKey::Bool(true))
    );
    assert!(
        rils_stdlib::stdlib::collections::native_heap_key(boolean.view())
            .unwrap_err()
            .contains("does not support ordering")
    );
}
