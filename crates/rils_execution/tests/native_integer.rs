use rils_execution::{
    Type, Value,
    numeric::{execute_integer_intrinsic, integer_constant},
};

#[test]
fn native_i32_wrapping_add_and_existing_other_integer_dispatch() {
    assert_eq!(
        execute_integer_intrinsic(
            "core::integer::wrapping_add",
            None,
            &[
                Value::from(rils_execution::value::native_i32(i32::MAX)),
                Value::from(rils_execution::value::native_i32(1))
            ],
        ),
        Ok(Value::from(rils_execution::value::native_i32(i32::MIN)))
    );
    assert_eq!(
        execute_integer_intrinsic(
            "core::integer::wrapping_add",
            None,
            &[
                Value::from(rils_execution::value::native_u32(u32::MAX)),
                Value::from(rils_execution::value::native_u32(1))
            ],
        ),
        Ok(Value::from(rils_execution::value::native_u32(0)))
    );
    assert!(
        execute_integer_intrinsic(
            "core::integer::wrapping_add",
            None,
            &[
                Value::from(rils_execution::value::native_i32(1)),
                Value::from(rils_execution::value::native_u32(2))
            ],
        )
        .unwrap_err()
        .contains("expected i32")
    );
}

#[test]
fn native_i32_methods_cover_boundary_shapes() {
    let cases = [
        (
            "core::integer::wrapping_neg",
            vec![Value::from(rils_execution::value::native_i32(i32::MIN))],
            Value::from(rils_execution::value::native_i32(i32::MIN)),
        ),
        (
            "core::integer::saturating_neg",
            vec![Value::from(rils_execution::value::native_i32(i32::MIN))],
            Value::from(rils_execution::value::native_i32(i32::MAX)),
        ),
        (
            "core::integer::saturating_abs",
            vec![Value::from(rils_execution::value::native_i32(i32::MIN))],
            Value::from(rils_execution::value::native_i32(i32::MAX)),
        ),
        (
            "core::integer::saturating_pow",
            vec![
                Value::from(rils_execution::value::native_i32(i32::MAX)),
                Value::from(rils_execution::value::native_u32(2)),
            ],
            Value::from(rils_execution::value::native_i32(i32::MAX)),
        ),
        (
            "core::integer::count_ones",
            vec![Value::from(rils_execution::value::native_i32(-1))],
            Value::from(rils_execution::value::native_u32(32)),
        ),
        (
            "core::integer::rotate_left",
            vec![
                Value::from(rils_execution::value::native_i32(1)),
                Value::from(rils_execution::value::native_u32(31)),
            ],
            Value::from(rils_execution::value::native_i32(i32::MIN)),
        ),
        (
            "core::integer::to_f64",
            vec![Value::from(rils_execution::value::native_i32(-5))],
            Value::from(rils_execution::value::native_f64(-5.0)),
        ),
    ];
    for (id, arguments, expected) in cases {
        assert_eq!(
            execute_integer_intrinsic(id, None, &arguments),
            Ok(expected),
            "{id:?}"
        );
    }
    assert!(
        execute_integer_intrinsic(
            "core::integer::abs",
            None,
            &[Value::from(rils_execution::value::native_i32(i32::MIN))]
        )
        .unwrap_err()
        .contains("integer overflow")
    );
    assert!(
        execute_integer_intrinsic(
            "core::integer::div_euclid",
            None,
            &[
                Value::from(rils_execution::value::native_i32(1)),
                Value::from(rils_execution::value::native_i32(0))
            ]
        )
        .unwrap_err()
        .contains("division by zero")
    );
    assert_eq!(
        execute_integer_intrinsic(
            "core::integer::checked_abs",
            None,
            &[Value::from(rils_execution::value::native_i32(i32::MIN))]
        ),
        Ok(
            rils_execution::value::storage::TypedStorageContext::new(&[], &[])
                .construct_option(&Type::Option(Box::new(Type::I32)), None)
                .unwrap()
        )
    );
    assert_eq!(
        integer_constant(
            rils_execution::IntegerType::I32,
            rils_builtins::IntegerConstantId::Bits
        ),
        Value::from(rils_execution::value::native_u32(32))
    );
    assert!(
        execute_integer_intrinsic(
            "core::integer::try_from",
            None,
            &[Value::from(rils_execution::value::native_i32(1))]
        )
        .unwrap_err()
        .contains("missing its target")
    );
    let result = execute_integer_intrinsic(
        "core::integer::try_from",
        Some(rils_execution::IntegerType::I32),
        &[Value::from(rils_execution::value::native_u128(u128::MAX))],
    )
    .unwrap();
    assert!(matches!(result, Value::Dynamic(_)));
    assert_eq!(
        Type::of_value(&result),
        Some(Type::Result(Box::new(Type::I32), Box::new(Type::String)))
    );
    let error = rils_execution::value::dynamic_result::take_owned(result)
        .unwrap()
        .unwrap_err();
    assert!(
        error
            .get_cloned::<String>()
            .ok()
            .is_some_and(|message| message.contains("outside the `i32` range"))
    );
}

#[test]
fn generated_integer_family_runs_for_every_width() {
    let cases = [
        (
            Value::from(rils_execution::value::native_i8(-1)),
            Value::from(rils_execution::value::native_i8(-2)),
        ),
        (
            Value::from(rils_execution::value::native_i16(-1)),
            Value::from(rils_execution::value::native_i16(-2)),
        ),
        (
            Value::from(rils_execution::value::native_i32(-1)),
            Value::from(rils_execution::value::native_i32(-2)),
        ),
        (
            Value::from(rils_execution::value::native_i64(-1)),
            Value::from(rils_execution::value::native_i64(-2)),
        ),
        (
            Value::from(rils_execution::value::native_i128(-1)),
            Value::from(rils_execution::value::native_i128(-2)),
        ),
        (
            Value::from(rils_execution::value::native_isize(-1)),
            Value::from(rils_execution::value::native_isize(-2)),
        ),
        (
            Value::from(rils_execution::value::native_u8(1)),
            Value::from(rils_execution::value::native_u8(2)),
        ),
        (
            Value::from(rils_execution::value::native_u16(1)),
            Value::from(rils_execution::value::native_u16(2)),
        ),
        (
            Value::from(rils_execution::value::native_u32(1)),
            Value::from(rils_execution::value::native_u32(2)),
        ),
        (
            Value::from(rils_execution::value::native_u64(1)),
            Value::from(rils_execution::value::native_u64(2)),
        ),
        (
            Value::from(rils_execution::value::native_u128(1)),
            Value::from(rils_execution::value::native_u128(2)),
        ),
        (
            Value::from(rils_execution::value::native_usize(1)),
            Value::from(rils_execution::value::native_usize(2)),
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(
            execute_integer_intrinsic("core::integer::wrapping_add", None, &[input.clone(), input]),
            Ok(expected)
        );
    }
    for target in rils_execution::IntegerType::ALL {
        assert_eq!(
            integer_constant(target, rils_builtins::IntegerConstantId::Bits),
            Value::from(rils_execution::value::native_u32(target.bits()))
        );
        let result = execute_integer_intrinsic(
            "core::integer::try_from",
            Some(target),
            &[Value::from(rils_execution::value::native_i32(1))],
        )
        .unwrap();
        assert!(matches!(result, Value::Dynamic(_)));
        assert_eq!(
            Type::of_value(&result),
            Some(Type::Result(
                Box::new(Type::Integer(target)),
                Box::new(Type::String)
            ))
        );
        let value = rils_execution::value::dynamic_result::take_owned(result)
            .unwrap()
            .unwrap();
        assert_eq!(Type::of_value(&value), Some(Type::Integer(target)));
    }
    assert_eq!(
        execute_integer_intrinsic(
            "core::integer::saturating_neg",
            None,
            &[Value::from(rils_execution::value::native_u8(3))]
        ),
        Ok(Value::from(rils_execution::value::native_u8(0)))
    );
    assert_eq!(
        execute_integer_intrinsic(
            "core::integer::abs",
            None,
            &[Value::from(rils_execution::value::native_u8(3))]
        ),
        Ok(Value::from(rils_execution::value::native_u8(3)))
    );
}
