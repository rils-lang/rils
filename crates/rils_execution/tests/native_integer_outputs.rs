use rils_execution::{
    IntegerType, Type, Value,
    numeric::{self, execute_integer_intrinsic},
    value::{dynamic_option, dynamic_result, native_instance::NativeInstancePlace},
};

fn call(method: &str, values: Vec<Value>) -> Value {
    execute_integer_intrinsic(&format!("core::integer::{method}"), None, &values).unwrap()
}

fn check_tuple(value: Value, expected: Value, overflow: bool) {
    let expected_type = Type::of_value(&expected).unwrap();
    assert_eq!(
        Type::of_value(&value),
        Some(Type::Tuple(vec![expected_type, Type::Bool]))
    );
    assert!(value.is_copy());
    let copy = value.clone_owned().unwrap();
    let Value::Dynamic(object) = value else {
        panic!("overflow result must be native");
    };
    let place = NativeInstancePlace::new(object).unwrap();
    assert_eq!(place.field("0").unwrap().take().unwrap(), expected);
    assert_eq!(
        place.field("1").unwrap().take().unwrap(),
        Value::Bool(overflow)
    );
    place
        .field("1")
        .unwrap()
        .assign(Value::Bool(!overflow))
        .unwrap();
    let Value::Dynamic(copy) = copy else {
        panic!("tuple copy must be native");
    };
    assert_eq!(
        NativeInstancePlace::new(copy)
            .unwrap()
            .field("1")
            .unwrap()
            .take()
            .unwrap(),
        Value::Bool(overflow)
    );
}

macro_rules! check_width {
    ($rust:ty, $constructor:path, $abs:expr) => {{
        let inputs = [0, 1, <$rust>::MIN, <$rust>::MAX];
        for input in inputs {
            for rhs in [0, 1, 2] {
                for (method, expected) in [
                    ("overflowing_add", input.overflowing_add(rhs)),
                    ("overflowing_sub", input.overflowing_sub(rhs)),
                    ("overflowing_mul", input.overflowing_mul(rhs)),
                ] {
                    check_tuple(
                        call(method, vec![$constructor(input), $constructor(rhs)]),
                        $constructor(expected.0),
                        expected.1,
                    );
                }
            }
            let expected = input.overflowing_neg();
            check_tuple(
                call("overflowing_neg", vec![$constructor(input)]),
                $constructor(expected.0),
                expected.1,
            );
            let expected = ($abs)(input);
            check_tuple(
                call("overflowing_abs", vec![$constructor(input)]),
                $constructor(expected.0),
                expected.1,
            );
            for argument in [0, 1, <$rust>::BITS, <$rust>::BITS + 1] {
                for (method, expected) in [
                    ("overflowing_shl", input.overflowing_shl(argument)),
                    ("overflowing_shr", input.overflowing_shr(argument)),
                    ("overflowing_pow", input.overflowing_pow(argument)),
                ] {
                    check_tuple(
                        call(
                            method,
                            vec![$constructor(input), numeric::native_u32(argument)],
                        ),
                        $constructor(expected.0),
                        expected.1,
                    );
                }
            }
            for (method, expected, other) in [
                ("checked_add", input.checked_add(1), 1),
                ("checked_mul", input.checked_mul(2), 2),
                ("checked_div", input.checked_div(0), 0),
            ] {
                let result = call(method, vec![$constructor(input), $constructor(other)]);
                assert!(matches!(result, Value::Dynamic(_)));
                assert_eq!(
                    Type::of_value(&result),
                    Some(Type::Option(Box::new(
                        Type::of_value(&$constructor(input)).unwrap()
                    )))
                );
                assert!(result.is_copy());
                assert_eq!(
                    dynamic_option::take_owned(result).unwrap(),
                    expected.map($constructor)
                );
            }
        }
    }};
}

#[test]
fn generated_composite_outputs_match_rust_for_all_integer_widths() {
    check_width!(i8, numeric::native_i8, i8::overflowing_abs);
    check_width!(i16, numeric::native_i16, i16::overflowing_abs);
    check_width!(i32, numeric::native_i32, i32::overflowing_abs);
    check_width!(i64, numeric::native_i64, i64::overflowing_abs);
    check_width!(i128, numeric::native_i128, i128::overflowing_abs);
    check_width!(isize, numeric::native_isize, isize::overflowing_abs);
    check_width!(u8, numeric::native_u8, |value| (value, false));
    check_width!(u16, numeric::native_u16, |value| (value, false));
    check_width!(u32, numeric::native_u32, |value| (value, false));
    check_width!(u64, numeric::native_u64, |value| (value, false));
    check_width!(u128, numeric::native_u128, |value| (value, false));
    check_width!(usize, numeric::native_usize, |value| (value, false));
}

#[test]
fn native_conversion_results_keep_inactive_types_and_copy_policy() {
    for target in IntegerType::ALL {
        for (source, success) in [
            (numeric::native_i32(1), true),
            (
                if target == IntegerType::U128 {
                    numeric::native_i32(-1)
                } else {
                    numeric::native_u128(u128::MAX)
                },
                false,
            ),
        ] {
            let value =
                execute_integer_intrinsic("core::integer::try_from", Some(target), &[source])
                    .unwrap();
            assert!(matches!(value, Value::Dynamic(_)));
            assert_eq!(
                Type::of_value(&value),
                Some(Type::Result(
                    Box::new(Type::Integer(target)),
                    Box::new(Type::String)
                ))
            );
            // The inactive string error branch prevents Copy even for Ok(1).
            assert!(!value.is_copy());
            let result = dynamic_result::take_owned(value).unwrap();
            assert_eq!(result.is_ok(), success);
            match result {
                Ok(number) => assert_eq!(Type::of_value(&number), Some(Type::Integer(target))),
                Err(error) => assert!(error.as_string().unwrap().contains("outside the")),
            }
        }
    }
}
