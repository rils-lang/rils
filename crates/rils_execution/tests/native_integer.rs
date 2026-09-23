use rils_builtins::BuiltinId;
use rils_execution::{
    Type, Value,
    numeric::{execute_integer_intrinsic, integer_constant},
};

#[test]
fn native_i32_wrapping_add_and_existing_other_integer_dispatch() {
    assert_eq!(
        execute_integer_intrinsic(
            BuiltinId::IntegerWrappingAdd,
            None,
            &[Value::I32(i32::MAX), Value::I32(1)],
        ),
        Ok(Value::I32(i32::MIN))
    );
    assert_eq!(
        execute_integer_intrinsic(
            BuiltinId::IntegerWrappingAdd,
            None,
            &[Value::U32(u32::MAX), Value::U32(1)],
        ),
        Ok(Value::U32(0))
    );
    assert!(
        execute_integer_intrinsic(
            BuiltinId::IntegerWrappingAdd,
            None,
            &[Value::I32(1), Value::U32(2)],
        )
        .unwrap_err()
        .contains("expected i32")
    );
}

#[test]
fn native_i32_methods_cover_boundary_shapes() {
    let cases = [
        (
            BuiltinId::IntegerWrappingNeg,
            vec![Value::I32(i32::MIN)],
            Value::I32(i32::MIN),
        ),
        (
            BuiltinId::IntegerSaturatingNeg,
            vec![Value::I32(i32::MIN)],
            Value::I32(i32::MAX),
        ),
        (
            BuiltinId::IntegerSaturatingAbs,
            vec![Value::I32(i32::MIN)],
            Value::I32(i32::MAX),
        ),
        (
            BuiltinId::IntegerSaturatingPow,
            vec![Value::I32(i32::MAX), Value::U32(2)],
            Value::I32(i32::MAX),
        ),
        (
            BuiltinId::IntegerCountOnes,
            vec![Value::I32(-1)],
            Value::U32(32),
        ),
        (
            BuiltinId::IntegerRotateLeft,
            vec![Value::I32(1), Value::U32(31)],
            Value::I32(i32::MIN),
        ),
        (
            BuiltinId::IntegerToF64,
            vec![Value::I32(-5)],
            Value::F64(-5.0),
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
        execute_integer_intrinsic(BuiltinId::IntegerAbs, None, &[Value::I32(i32::MIN)])
            .unwrap_err()
            .contains("integer overflow")
    );
    assert!(
        execute_integer_intrinsic(
            BuiltinId::IntegerDivEuclid,
            None,
            &[Value::I32(1), Value::I32(0)]
        )
        .unwrap_err()
        .contains("division by zero")
    );
    assert_eq!(
        execute_integer_intrinsic(BuiltinId::IntegerCheckedAbs, None, &[Value::I32(i32::MIN)]),
        Ok(Value::Option {
            value: None,
            element_type: Some(Type::I32)
        })
    );
    assert_eq!(
        integer_constant(
            rils_execution::IntegerType::I32,
            rils_builtins::IntegerConstantId::Bits
        ),
        Value::U32(32)
    );
    assert!(
        execute_integer_intrinsic(BuiltinId::IntegerTryFrom, None, &[Value::I32(1)])
            .unwrap_err()
            .contains("missing its target")
    );
    match execute_integer_intrinsic(
        BuiltinId::IntegerTryFrom,
        Some(rils_execution::IntegerType::I32),
        &[Value::U128(u128::MAX)],
    )
    .unwrap()
    {
        Value::Result {
            value: Err(error),
            ok_type,
            error_type,
        } => {
            assert_eq!(ok_type, Some(Type::I32));
            assert_eq!(error_type, Some(Type::String));
            assert!(
                matches!(error.as_ref(), Value::String(message) if message.contains("outside the `i32` range"))
            );
        }
        value => panic!("expected failed conversion, found {value:?}"),
    }
}

#[test]
fn generated_integer_family_runs_for_every_width() {
    let cases = [
        (Value::I8(-1), Value::I8(-2)),
        (Value::I16(-1), Value::I16(-2)),
        (Value::I32(-1), Value::I32(-2)),
        (Value::I64(-1), Value::I64(-2)),
        (Value::I128(-1), Value::I128(-2)),
        (Value::Isize(-1), Value::Isize(-2)),
        (Value::U8(1), Value::U8(2)),
        (Value::U16(1), Value::U16(2)),
        (Value::U32(1), Value::U32(2)),
        (Value::U64(1), Value::U64(2)),
        (Value::U128(1), Value::U128(2)),
        (Value::Usize(1), Value::Usize(2)),
    ];
    for (input, expected) in cases {
        assert_eq!(
            execute_integer_intrinsic(BuiltinId::IntegerWrappingAdd, None, &[input.clone(), input]),
            Ok(expected)
        );
    }
    for target in rils_execution::IntegerType::ALL {
        assert_eq!(
            integer_constant(target, rils_builtins::IntegerConstantId::Bits),
            Value::U32(target.bits())
        );
        match execute_integer_intrinsic(BuiltinId::IntegerTryFrom, Some(target), &[Value::I32(1)])
            .unwrap()
        {
            Value::Result {
                value: Ok(value),
                ok_type,
                error_type,
            } => {
                assert_eq!(ok_type, Some(Type::Integer(target)));
                assert_eq!(error_type, Some(Type::String));
                assert_eq!(Type::of_value(value.as_ref()), Some(Type::Integer(target)));
            }
            value => panic!("expected successful {target} conversion, found {value:?}"),
        }
    }
    assert_eq!(
        execute_integer_intrinsic(BuiltinId::IntegerSaturatingNeg, None, &[Value::U8(3)]),
        Ok(Value::U8(0))
    );
    assert_eq!(
        execute_integer_intrinsic(BuiltinId::IntegerAbs, None, &[Value::U8(3)]),
        Ok(Value::U8(3))
    );
}
