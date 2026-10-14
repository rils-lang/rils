use rils_execution::{IntegerType, Value, numeric, value::*};
use rils_frontend::ast::BinaryOp;

#[test]
fn generated_signed_negation_and_division_keep_overflow_checks() {
    macro_rules! check {
        ($($rust:ty, $make:ident;)*) => {$(
            assert_eq!(numeric::negate($make(<$rust>::MIN).into()).unwrap_err(), "integer overflow");
            assert_eq!(numeric::negate($make(42).into()).unwrap().get_cloned::<$rust>().unwrap(), -42);
            for operator in [BinaryOp::Divide, BinaryOp::Remainder] {
                assert_eq!(numeric::binary($make(<$rust>::MIN).into(), operator, $make(-1).into()).unwrap_err(), "integer overflow");
                assert_eq!(numeric::binary($make(1).into(), operator, $make(0).into()).unwrap_err(), "division by zero");
            }
        )*};
    }
    check! { i8, native_i8; i16, native_i16; i32, native_i32; i64, native_i64; i128, native_i128; isize, native_isize; }
    for value in [
        Value::from(native_u8(0)),
        native_u16(0).into(),
        native_u32(0).into(),
        native_u64(0).into(),
        native_u128(0).into(),
        native_usize(0).into(),
    ] {
        assert!(
            numeric::negate(value)
                .unwrap_err()
                .contains("signed number")
        );
    }
}

#[test]
fn generated_float_operators_preserve_nan_infinity_and_signed_zero() {
    macro_rules! check {
        ($rust:ty, $make:ident) => {
            for operator in [BinaryOp::Divide, BinaryOp::Remainder] {
                for zero in [0.0, -0.0] {
                    assert_eq!(
                        numeric::binary($make(1.0).into(), operator, $make(zero).into())
                            .unwrap_err(),
                        "division by zero"
                    );
                }
            }
            assert!(
                numeric::binary(
                    $make(<$rust>::MAX).into(),
                    BinaryOp::Multiply,
                    $make(2.0).into()
                )
                .unwrap()
                .get_cloned::<$rust>()
                .unwrap()
                .is_infinite()
            );
            assert!(
                numeric::negate($make(0.0).into())
                    .unwrap()
                    .get_cloned::<$rust>()
                    .unwrap()
                    .is_sign_negative()
            );
            assert!(
                numeric::binary($make(<$rust>::NAN).into(), BinaryOp::Add, $make(1.0).into())
                    .unwrap()
                    .get_cloned::<$rust>()
                    .unwrap()
                    .is_nan()
            );
            for operator in [
                BinaryOp::Less,
                BinaryOp::Greater,
                BinaryOp::LessEqual,
                BinaryOp::GreaterEqual,
            ] {
                assert!(
                    !numeric::binary($make(<$rust>::NAN).into(), operator, $make(1.0).into())
                        .unwrap()
                        .get_cloned::<bool>()
                        .unwrap()
                );
            }
        };
    }
    check!(f32, native_f32);
    check!(f64, native_f64);
    assert!(numeric::binary(native_i32(1).into(), BinaryOp::Add, native_f32(1.0).into()).is_err());
}

#[test]
fn integer_conversion_preserves_full_range_rules_and_wide_values() {
    let cases = [
        (
            Value::from(native_i8(i8::MIN)),
            IntegerType::I128,
            Value::from(native_i128(i8::MIN as i128)),
        ),
        (
            native_u64(u64::MAX).into(),
            IntegerType::I128,
            native_i128(u64::MAX as i128).into(),
        ),
        (
            native_u128(u128::MAX).into(),
            IntegerType::U128,
            native_u128(u128::MAX).into(),
        ),
        (
            native_i128(i128::MIN).into(),
            IntegerType::I128,
            native_i128(i128::MIN).into(),
        ),
        (native_i8(1).into(), IntegerType::U64, native_u64(1).into()),
    ];
    for (source, target, expected) in cases {
        assert_eq!(numeric::cast_integer(source, target).unwrap(), expected);
    }
    for (source, target) in [
        (Value::from(native_i128(1)), IntegerType::I64),
        (native_u128(1).into(), IntegerType::I128),
        (native_u64(1).into(), IntegerType::I64),
    ] {
        assert!(
            numeric::cast_integer(source, target)
                .unwrap_err()
                .contains("full range")
        );
    }
    assert!(
        numeric::cast_integer(native_i8(-1).into(), IntegerType::U64)
            .unwrap_err()
            .contains("without losing information")
    );
    assert!(
        numeric::cast_integer(native_bool(true).into(), IntegerType::I32)
            .unwrap_err()
            .contains("expects an integer")
    );
}
