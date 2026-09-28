use rils::{BytecodeModule, Type, Value, compile, eval_value};

#[test]
fn character_literals_and_string_items_have_native_storage() {
    for source in [
        "'你'",
        "let mut chars = \"你\".chars(); chars.next().unwrap()",
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            let Value::Native(object) = &value else {
                panic!("char should use native storage: {source}");
            };
            assert!(object.is_inline());
            assert_eq!(object.descriptor().rils_type(), &Type::Char);
            assert_eq!(value.as_char(), Some('你'));
            assert_eq!(value.to_string(), "你");
            assert_eq!(format!("{value:?}"), "'你'");
        }
    }
}

#[test]
fn native_char_works_as_ordered_key_and_heap_element() {
    for (source, expected) in [
        (
            "let mut values: BTreeSet<char> = BTreeSet::new(); values.insert('b'); values.insert('a'); values.first_cloned().unwrap()",
            'a',
        ),
        (
            "let mut values: BinaryHeap<char> = BinaryHeap::new(); values.push('a'); values.push('z'); values.pop().unwrap()",
            'z',
        ),
    ] {
        assert_eq!(eval_value(source).unwrap().as_char(), Some(expected));
        assert_eq!(
            compile(source).unwrap().execute_value().unwrap().as_char(),
            Some(expected)
        );
    }
}
