use rils_execution::{IntegerType, RilsValue, Type, Value, numeric};
use rils_frontend::ast::BinaryOp;

#[test]
fn unit_and_bool_have_native_copy_and_host_semantics() {
    let unit = Value::from(rils_execution::value::native_unit());
    assert!(matches!(&unit, Value::Native(_)));
    assert!(unit.is_copy());
    assert!(unit.is_unit());
    assert!(!unit.is_truthy());
    assert_eq!(unit.clone_owned().unwrap(), unit);
    assert_eq!(RilsValue::new(unit).into_owned::<()>().unwrap(), ());
    for flag in [false, true] {
        let value = Value::from(rils_execution::value::native_bool(flag));
        assert!(matches!(&value, Value::Native(_)));
        assert_eq!(Type::of_value(&value), Some(Type::Bool));
        assert_eq!(value.get_cloned::<bool>().ok(), Some(flag));
        assert_eq!(value.is_truthy(), flag);
        assert!(value.is_copy());
        assert_eq!(value.clone_owned().unwrap(), value);
        assert_eq!(RilsValue::new(value).into_owned::<bool>().unwrap(), flag);
    }
    assert_ne!(
        Value::from(rils_execution::value::native_bool(false)),
        Value::from(rils_execution::value::native_unit())
    );
}

#[test]
fn every_integer_width_uses_native_arithmetic_and_checked_boundaries() {
    macro_rules! check {
        ($($rust:ty, $kind:ident, $make:ident, $read:ident;)*) => {$(
            let value = numeric::integer_binary_typed(
                Value::from(rils_execution::value::$make(19)), IntegerType::$kind, BinaryOp::Add, Value::from(rils_execution::value::$make(23)),
            ).unwrap();
            assert!(matches!(&value, Value::Native(_)));
            assert_eq!(value.get_cloned::<$rust>().ok(), Some(42));
            assert_eq!(numeric::binary(Value::from(rils_execution::value::$make(19)), BinaryOp::Add, Value::from(rils_execution::value::$make(23))).unwrap(), value);
            assert!(numeric::binary(Value::from(rils_execution::value::$make(<$rust>::MAX)), BinaryOp::Add, Value::from(rils_execution::value::$make(1))).is_err());
            assert!(numeric::binary(Value::from(rils_execution::value::$make(1)), BinaryOp::Divide, Value::from(rils_execution::value::$make(0))).is_err());
            let comparison = numeric::integer_binary_typed(
                Value::from(rils_execution::value::$make(19)), IntegerType::$kind, BinaryOp::Less, Value::from(rils_execution::value::$make(23)),
            ).unwrap();
            assert!(matches!(&comparison, Value::Native(_)));
            assert_eq!(comparison.get_cloned::<bool>().ok(), Some(true));
        )*};
    }
    check! {
        i8, I8, native_i8, as_i8;
        i16, I16, native_i16, as_i16;
        i32, I32, native_i32, as_i32;
        i64, I64, native_i64, as_i64;
        i128, I128, native_i128, as_i128;
        isize, Isize, native_isize, as_isize;
        u8, U8, native_u8, as_u8;
        u16, U16, native_u16, as_u16;
        u32, U32, native_u32, as_u32;
        u64, U64, native_u64, as_u64;
        u128, U128, native_u128, as_u128;
        usize, Usize, native_usize, as_usize;
    }
    assert!(
        numeric::integer_binary_typed(
            Value::from(rils_execution::value::native_i32(1)),
            IntegerType::U32,
            BinaryOp::Equal,
            Value::from(rils_execution::value::native_i32(1)),
        )
        .is_err()
    );
    assert!(
        numeric::binary(
            Value::from(rils_execution::value::native_i16(1)),
            BinaryOp::Add,
            Value::from(rils_execution::value::native_u16(1))
        )
        .is_err()
    );
}

#[test]
fn formatter_success_is_a_native_result() {
    let value = rils_execution::formatting::format_ok().unwrap();
    assert!(matches!(&value, Value::Dynamic(_)));
    let result = rils_execution::value::dynamic_result::take_owned(value).unwrap();
    assert!(result.unwrap().is_unit());
}
