use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn integer_widths_use_native_storage_in_both_backends() {
    for (source, expected) in [
        ("1i8 + 2i8", Value::from_i8(3)),
        ("1i16 + 2i16", Value::from_i16(3)),
        ("1i32 + 2i32", Value::from_i32(3)),
        ("1i64 + 2i64", Value::from_i64(3)),
        ("1i128 + 2i128", Value::from_i128(3)),
        ("1isize + 2isize", Value::from_isize(3)),
        ("1u8 + 2u8", Value::from_u8(3)),
        ("1u16 + 2u16", Value::from_u16(3)),
        ("1u32 + 2u32", Value::from_u32(3)),
        ("1u64 + 2u64", Value::from_u64(3)),
        ("1u128 + 2u128", Value::from_u128(3)),
        ("1usize + 2usize", Value::from_usize(3)),
    ] {
        let interpreted = eval_value(source).unwrap();
        assert!(matches!(&interpreted, Value::Native(_)), "{source}");
        assert_eq!(interpreted, expected, "{source}");

        let compiled = compile(source).unwrap();
        let restored = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for actual in [
            compiled.execute_value().unwrap(),
            restored.execute_value().unwrap(),
        ] {
            assert!(matches!(&actual, Value::Native(_)), "{source}");
            assert_eq!(actual, expected, "{source}");
        }
    }
}

#[test]
fn migrated_integer_keys_and_heap_order_match_in_both_backends() {
    for (ty, low, high) in [
        ("i16", "-1i16", "2i16"),
        ("u8", "1u8", "2u8"),
        ("u128", "1u128", "2u128"),
    ] {
        let source = format!(
            "let mut keys: HashSet<{ty}> = HashSet::new(); \
             let key = {high}; keys.insert(key); \
             let mut heap: BinaryHeap<{ty}> = BinaryHeap::new(); \
             heap.push({low}); heap.push({high}); \
             keys.contains(&key) && heap.pop().unwrap() == {high}"
        );
        assert_eq!(eval_value(&source).unwrap(), Value::Bool(true), "{ty}");
        assert_eq!(
            compile(&source).unwrap().execute_value().unwrap(),
            Value::Bool(true),
            "{ty}"
        );
    }
}
