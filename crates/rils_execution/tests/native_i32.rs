use rils_execution::{
    Type, Value,
    numeric::{execute_integer_intrinsic, native_i32},
};

#[test]
fn generated_i32_descriptor_keeps_typed_inline_storage_and_method_results() {
    let value = Value::from(native_i32(i32::MAX));
    let Value::Native(object) = &value else {
        panic!("i32 must use native storage");
    };
    assert!(object.is_inline());
    assert_eq!(object.descriptor().rils_type(), &Type::I32);
    assert_eq!(
        object.with::<rils_stdlib::stdlib::integer::Number<i32>, _>(|number| number.0),
        Ok(i32::MAX)
    );
    assert!(
        object
            .with::<rils_stdlib::stdlib::integer::Number<i8>, _>(|number| number.0)
            .is_err()
    );

    let result = execute_integer_intrinsic(
        "core::integer::wrapping_add",
        None,
        &[value.clone(), Value::from(native_i32(1))],
    )
    .unwrap();
    assert_eq!(result.get_cloned::<i32>().ok(), Some(i32::MIN));
    assert!(matches!(result, Value::Native(_)));
    assert_eq!(
        execute_integer_intrinsic(
            "core::integer::wrapping_add",
            None,
            &[value, Value::from(rils_execution::value::native_u32(1))],
        )
        .unwrap_err(),
        "expected i32, found u32"
    );
}
