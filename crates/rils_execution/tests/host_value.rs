use rils_execution::{RilsValue, Value};

#[test]
fn borrows_clones_and_moves_exact_native_values() {
    let result = RilsValue::new(Value::from_i32(42));
    assert_eq!(result.with_ref::<i32, _>(|value| *value).unwrap(), 42);
    assert_eq!(result.get_cloned::<i32>().unwrap(), 42);
    assert_eq!(result.into_owned::<i32>().ok(), Some(42));
}

#[test]
fn failed_move_preserves_the_owned_handle() {
    let result = RilsValue::new(Value::from_i32(42));
    let (result, error) = *result.into_owned::<i64>().err().unwrap();
    assert!(error.contains("Number"));
    assert_eq!(result.get_cloned::<i32>().unwrap(), 42);
}

#[test]
fn borrowed_string_can_be_read_without_cloning() {
    let result = RilsValue::new(Value::from_string("hello"));
    assert_eq!(result.with_ref::<String, _>(|text| text.len()).unwrap(), 5);
    assert_eq!(result.get_cloned::<String>().unwrap(), "hello");
    assert_eq!(result.into_owned::<String>().ok().as_deref(), Some("hello"));
}

#[test]
fn a_reference_cannot_be_moved_out() {
    use std::{cell::RefCell, rc::Rc};

    use rils_execution::{environment::StorageSlot, value::ReferenceValue};

    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    storage.borrow_mut().initialize(Value::from_i32(7));
    let reference = Value::Reference(Rc::new(ReferenceValue::new_storage(storage, false)));
    let result = RilsValue::new(reference);
    assert_eq!(result.with_ref::<i32, _>(|value| *value).unwrap(), 7);
    assert_eq!(result.get_cloned::<i32>().unwrap(), 7);
    let (result, error) = *result.into_owned::<i32>().err().unwrap();
    assert!(error.contains("not an owned"));
    assert_eq!(result.get_cloned::<i32>().unwrap(), 7);
}

#[test]
fn composed_scalar_borrows_and_owned_conversion_use_the_actual_leaf_type() {
    use rils_execution::RilsHostType;
    use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};
    use std::{fmt::Debug, rc::Rc};

    fn check<T: RilsHostType + Copy + PartialEq + Debug>(ty: rils_execution::Type, number: T) {
        let layout = DynamicLayout::copy_of::<T>(ty);
        let value = DynamicValue::from_rust(layout.clone(), number).unwrap();
        let object = DynamicObject::new(Rc::new(DynamicType::new(layout)), value).unwrap();
        let value = RilsValue::new(Value::Dynamic(object));
        assert_eq!(value.with_ref::<T, _>(|number| *number).unwrap(), number);
        assert_eq!(value.get_cloned::<T>().unwrap(), number);
        assert_eq!(value.into_owned::<T>().ok(), Some(number));
    }
    use rils_execution::{FloatType, IntegerType, Type};
    check(Type::Integer(IntegerType::I8), i8::MIN);
    check(Type::Integer(IntegerType::I16), i16::MIN);
    check(Type::I32, i32::MIN);
    check(Type::Integer(IntegerType::I64), i64::MIN);
    check(Type::Integer(IntegerType::I128), i128::MIN);
    check(Type::Integer(IntegerType::Isize), isize::MIN);
    check(Type::Integer(IntegerType::U8), u8::MAX);
    check(Type::Integer(IntegerType::U16), u16::MAX);
    check(Type::Integer(IntegerType::U32), u32::MAX);
    check(Type::Integer(IntegerType::U64), u64::MAX);
    check(Type::Integer(IntegerType::U128), u128::MAX);
    check(Type::Integer(IntegerType::Usize), usize::MAX);
    check(Type::Float(FloatType::F32), 42_f32);
    check(Type::F64, 42_f64);
    check(Type::Bool, true);
    check(Type::Char, '字');
}
