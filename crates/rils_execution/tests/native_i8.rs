use rils_execution::{
    Type, Value, formatting,
    numeric::{execute_integer_intrinsic, native_i8},
};
use rils_frontend::format::{FormatKind, FormatSpec};

#[test]
fn generated_i8_descriptor_stores_inline_and_calls_rust_methods() {
    let value = native_i8(i8::MAX);
    let Value::Native(object) = &value else {
        panic!("i8 must use native storage");
    };
    assert!(object.is_inline());
    assert_eq!(
        object.descriptor().rils_type(),
        &Type::Integer(rils_execution::IntegerType::I8)
    );
    assert_eq!(
        object.with::<rils_stdlib::stdlib::integer::Number<i8>, _>(|number| number.0),
        Ok(i8::MAX)
    );
    assert!(
        object
            .with::<rils_stdlib::stdlib::integer::Number<u8>, _>(|number| number.0)
            .is_err()
    );

    let result = execute_integer_intrinsic(
        "core::integer::wrapping_add",
        None,
        &[value.clone(), native_i8(1)],
    )
    .unwrap();
    assert_eq!(result, native_i8(i8::MIN));
    assert!(matches!(result, Value::Native(_)));
    assert_eq!(
        execute_integer_intrinsic("core::integer::wrapping_add", None, &[value, Value::U8(1)],)
            .unwrap_err(),
        "expected i8, found u8"
    );

    let none = execute_integer_intrinsic(
        "core::integer::checked_add",
        None,
        &[native_i8(i8::MAX), native_i8(1)],
    )
    .unwrap();
    let Value::Dynamic(object) = &none else {
        panic!("checked_add must return native Option<i8>");
    };
    assert!(object.is_inline());
    let item_type = Type::Integer(rils_execution::IntegerType::I8);
    assert_eq!(
        object.descriptor().layout().rils_type(),
        &Type::Option(Box::new(item_type.clone()))
    );
    assert_eq!(none.as_option(), Some((None, item_type)));

    let overflow = execute_integer_intrinsic(
        "core::integer::overflowing_add",
        None,
        &[native_i8(i8::MAX), native_i8(1)],
    )
    .unwrap();
    let Value::Tuple(fields) = overflow else {
        panic!("overflowing_add returns a tuple");
    };
    let fields = fields.elements.borrow();
    assert!(matches!(fields[0].value, Some(Value::Native(_))));
    assert_eq!(fields[0].value, Some(native_i8(i8::MIN)));
    assert_eq!(fields[1].value, Some(Value::Bool(true)));

    assert_eq!(
        execute_integer_intrinsic("core::integer::abs", None, &[native_i8(i8::MIN)]).unwrap_err(),
        "integer overflow"
    );
}

#[test]
fn native_i8_keeps_existing_format_and_hash_semantics() {
    let value = native_i8(-42);
    assert_eq!(value.to_string(), "-42");
    assert_eq!(
        rils_execution::value::HashKey::from_value(&value).unwrap(),
        rils_execution::value::HashKey::from_value(&Value::from_i8(-42)).unwrap()
    );
    let spec = FormatSpec {
        kind: FormatKind::LowerHex,
        ..Default::default()
    };
    assert_eq!(
        formatting::format_value(&native_i8(42), &spec).unwrap(),
        "2a"
    );
}
