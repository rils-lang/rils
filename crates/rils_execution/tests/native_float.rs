use rils_builtins::{BuiltinId, FloatConstantId};
use rils_execution::{
    FloatType, Value,
    numeric::{execute_intrinsic, float_constant},
};

#[test]
fn float_family_uses_rust_methods_for_both_widths() {
    for (value, expected, target) in [
        (Value::F32(-3.5), Value::F32(3.5), FloatType::F32),
        (Value::F64(-3.5), Value::F64(3.5), FloatType::F64),
    ] {
        assert_eq!(
            execute_intrinsic(BuiltinId::FloatAbs, None, &[value]),
            Ok(expected)
        );
        assert!(
            matches!(float_constant(target, FloatConstantId::Nan), Value::F32(v) if v.is_nan())
                || matches!(float_constant(target, FloatConstantId::Nan), Value::F64(v) if v.is_nan())
        );
    }
    assert_eq!(
        execute_intrinsic(
            BuiltinId::FloatClamp,
            None,
            &[Value::F32(1.0), Value::F32(f32::NAN), Value::F32(2.0)]
        ),
        Err("float clamp requires non-NaN bounds with min <= max".into()),
    );
}

#[test]
fn every_float_method_and_constant_has_a_native_binding() {
    for method in rils_builtins::FLOAT_INTRINSICS {
        for receiver in [Value::F32(1.5), Value::F64(1.5)] {
            let arguments =
                std::iter::repeat_n(receiver.clone(), method.signature.parameters.len() + 1)
                    .collect::<Vec<_>>();
            let result = execute_intrinsic(method.id, None, &arguments);
            assert!(result.is_ok(), "{}: {result:?}", method.name);
        }
    }
    for target in [FloatType::F32, FloatType::F64] {
        for constant in rils_builtins::FLOAT_CONSTANTS {
            let value = float_constant(target, constant.id);
            assert!(matches!(
                (target, value),
                (FloatType::F32, Value::F32(_)) | (FloatType::F64, Value::F64(_))
            ));
        }
    }
    assert!(
        execute_intrinsic(
            BuiltinId::FloatCopysign,
            None,
            &[Value::F32(1.0), Value::F64(2.0)]
        )
        .is_err()
    );
}
