use rils_execution::{
    Type, Value,
    numeric::{execute_integer_intrinsic, native_usize},
};

#[test]
fn generated_usize_descriptor_keeps_typed_inline_storage_and_method_results() {
    let value = native_usize(usize::MAX);
    let Value::Native(object) = &value else {
        panic!("usize must use native storage");
    };
    assert!(object.is_inline());
    assert_eq!(object.descriptor().rils_type(), &Type::USIZE);
    assert_eq!(
        object.with::<rils_stdlib::stdlib::integer::Number<usize>, _>(|number| number.0),
        Ok(usize::MAX)
    );
    assert!(
        object
            .with::<rils_stdlib::stdlib::integer::Number<u32>, _>(|number| number.0)
            .is_err()
    );

    let result = execute_integer_intrinsic(
        "core::integer::wrapping_add",
        None,
        &[value.clone(), native_usize(1)],
    )
    .unwrap();
    assert_eq!(result.as_usize(), Some(0));
    assert!(matches!(result, Value::Native(_)));
    assert_eq!(
        execute_integer_intrinsic("core::integer::wrapping_add", None, &[value, Value::U32(1)],)
            .unwrap_err(),
        "expected usize, found u32"
    );
}
