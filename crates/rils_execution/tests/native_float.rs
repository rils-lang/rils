use rils_builtins::FloatConstantId;
use rils_execution::{
    FloatType, Value,
    numeric::{execute_intrinsic, float_constant},
};

#[test]
fn float_family_uses_rust_methods_for_both_widths() {
    for (value, expected, target) in [
        (Value::from_f32(-3.5), Value::from_f32(3.5), FloatType::F32),
        (Value::from_f64(-3.5), Value::from_f64(3.5), FloatType::F64),
    ] {
        assert_eq!(
            execute_intrinsic("core::float::abs", None, &[value]),
            Ok(expected)
        );
        let nan = float_constant(target, FloatConstantId::Nan);
        assert!(matches!(&nan, Value::Native(_)));
        assert!(nan.as_f32().is_some_and(f32::is_nan) || nan.as_f64().is_some_and(f64::is_nan));
    }
    assert_eq!(
        execute_intrinsic(
            "core::float::clamp",
            None,
            &[Value::F32(1.0), Value::F32(f32::NAN), Value::F32(2.0)]
        ),
        Err("float clamp requires non-NaN bounds with min <= max".into()),
    );
}

#[test]
fn every_float_method_and_constant_has_a_native_binding() {
    for method in rils_builtins::FLOAT_INTRINSICS {
        for receiver in [Value::from_f32(1.5), Value::from_f64(1.5)] {
            let arguments =
                std::iter::repeat_n(receiver.clone(), method.signature.parameters.len() + 1)
                    .collect::<Vec<_>>();
            let result = execute_intrinsic(method.symbol, None, &arguments);
            assert!(result.is_ok(), "{}: {result:?}", method.name);
        }
    }
    for target in [FloatType::F32, FloatType::F64] {
        for constant in rils_builtins::FLOAT_CONSTANTS {
            let value = float_constant(target, constant.id);
            assert!(matches!(&value, Value::Native(_)));
            assert!(match target {
                FloatType::F32 => value.as_f32().is_some(),
                FloatType::F64 => value.as_f64().is_some(),
            });
        }
    }
    assert!(
        execute_intrinsic(
            "core::float::copysign",
            None,
            &[Value::F32(1.0), Value::F64(2.0)]
        )
        .is_err()
    );
}

#[test]
fn native_float_values_preserve_bits_and_accept_legacy_inputs() {
    let negative_zero = Value::from_f32(-0.0);
    assert!(matches!(negative_zero, Value::Native(_)));
    assert_eq!(
        negative_zero.as_f32().unwrap().to_bits(),
        (-0.0f32).to_bits()
    );
    let negative_zero = Value::from_f64(-0.0);
    assert!(matches!(negative_zero, Value::Native(_)));
    assert_eq!(
        negative_zero.as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );

    assert_eq!(
        execute_intrinsic("core::float::abs", None, &[Value::F32(-3.5)]),
        Ok(Value::from_f32(3.5))
    );
    assert_eq!(
        execute_intrinsic("core::float::abs", None, &[Value::F64(-3.5)]),
        Ok(Value::from_f64(3.5))
    );
}
