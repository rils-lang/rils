use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn native_range_steps_match_in_interpreter_and_vm() {
    for (source, expected) in [
        (
            "let mut total = 39; for value in 1..3 { total = total + value; } total",
            Value::from_i32(42),
        ),
        (
            "let mut total = 0u32; for value in 254u32..255u32 { total = total + value; } total - 212u32",
            Value::U32(42),
        ),
    ] {
        assert_eq!(eval_value(source).unwrap(), expected);
        let compiled = compile(source).unwrap();
        assert_eq!(compiled.execute_value().unwrap(), expected);
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_eq!(loaded.execute_value().unwrap(), expected);
    }

    for direct in [
        "let mut range = 1i8..3i8; if range.next() == Some(1i8) && range.next() == Some(2i8) && range.next() == None { 42 } else { 0 }",
        "let mut range = 254u8..255u8; if range.next() == Some(254u8) && range.next() == None { 42 } else { 0 }",
    ] {
        assert_eq!(eval_value(direct).unwrap(), Value::from_i32(42));
        let compiled = compile(direct).unwrap();
        assert_eq!(compiled.execute_value().unwrap(), Value::from_i32(42));
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_eq!(loaded.execute_value().unwrap(), Value::from_i32(42));
    }
}

#[test]
fn trait_qualified_range_next_uses_the_native_symbol_in_both_backends() {
    let source = include_str!("fixtures/trait_path_range.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    let compiled = compile(source).unwrap();
    assert_eq!(compiled.execute_value().unwrap(), Value::from_i32(42));
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap(), Value::from_i32(42));
}

#[test]
fn range_into_iter_uses_the_iterator_blanket_impl() {
    for source in [
        "let mut values = (1..3).into_iter(); values.next() == Some(1) && values.next() == Some(2) && values.next() == None",
        "let mut values = <Range<i32> as IntoIterator>::into_iter(1..3); values.next() == Some(1) && values.next() == Some(2) && values.next() == None",
    ] {
        assert_eq!(eval_value(source).unwrap(), Value::Bool(true));
        let compiled = compile(source).unwrap();
        assert_eq!(compiled.execute_value().unwrap(), Value::Bool(true));
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_eq!(loaded.execute_value().unwrap(), Value::Bool(true));
    }
}

#[test]
fn range_values_use_native_storage_in_both_backends() {
    let interpreted = eval_value("1..3").unwrap();
    assert!(matches!(interpreted, Value::Native(_)));
    assert_eq!(interpreted.type_name(), "Range<i32>");
    assert_eq!(interpreted.to_string(), "1..3");

    let compiled = compile("1..3").unwrap();
    assert_eq!(compiled.execute_value().unwrap(), interpreted);
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap(), interpreted);
}

#[test]
fn every_integer_range_uses_the_same_native_iterator_bridge() {
    for suffix in [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
    ] {
        let type_name = format!("type_of(1{suffix}..3{suffix})");
        let expected_type_name = format!("Range<{suffix}>");
        assert_eq!(
            eval_value(&type_name).unwrap().to_string(),
            expected_type_name,
            "{suffix} type_of"
        );
        assert_eq!(
            compile(&type_name)
                .unwrap()
                .execute_value()
                .unwrap()
                .to_string(),
            expected_type_name,
            "{suffix} VM type_of"
        );
        let source = format!(
            "let mut range = 1{suffix}..3{suffix};
             if range.next() == Some(1{suffix})
                && range.next() == Some(2{suffix})
                && range.next() == None {{ 42 }} else {{ 0 }}"
        );
        assert_eq!(
            eval_value(&source).unwrap(),
            Value::from_i32(42),
            "{suffix} interpreter"
        );
        assert_eq!(
            compile(&source).unwrap().execute_value().unwrap(),
            Value::from_i32(42),
            "{suffix} VM"
        );
    }
}
