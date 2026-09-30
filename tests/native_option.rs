use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn consuming_option_methods_move_non_copy_native_payloads() {
    for source in [
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Option<Vec<string>> = Some(items);
            let mut recovered = value.unwrap_or(Vec::new()); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Option<Vec<string>> = Some(items);
            let mut recovered = value.expect("missing"); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Option<Vec<string>> = Some(items);
            let mut recovered = value.or(None).unwrap(); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Option<Vec<string>> = Some(items);
            let mut recovered = value.xor(None).unwrap(); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let mut value: Option<Vec<string>> = Some(items);
            let mut recovered = value.take().unwrap(); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let mut value: Option<Vec<string>> = Some(items);
            let mut recovered = value.replace(Vec::new()).unwrap(); recovered.pop().unwrap()"#,
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_eq!(value.as_string().as_deref(), Some("first"), "{source}");
        }
    }
}

#[test]
fn mutable_option_methods_write_back_inline_copy_values() {
    let source = r#"
        let mut value: Option<i32> = Some(10);
        let old = value.replace(20).unwrap();
        let taken = value.take().unwrap();
        if old == 10 && taken == 20 && value.is_none() { 42 } else { 0 }
    "#;
    let compiled = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        eval_value(source).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(value.as_i32(), Some(42));
    }
}

fn assert_dynamic_option(
    value: Value,
    ty: &str,
    inline: bool,
    display: &str,
    debug: &str,
    stage: &str,
) {
    assert_eq!(value.to_string(), display);
    assert_eq!(format!("{value:?}"), debug);
    assert!(value.as_option().is_some());
    let Value::Dynamic(object) = value else {
        panic!("{ty} must use dynamic native storage in {stage}");
    };
    assert_eq!(object.descriptor().layout().rils_type().to_string(), ty);
    assert_eq!(object.is_inline(), inline);
}

#[test]
fn concrete_options_use_native_layout_in_both_backends() {
    for (source, ty, inline, display, debug) in [
        ("Some(3i8)", "Option<i8>", true, "Some(3)", "Some(3)"),
        ("Some(7i32)", "Option<i32>", true, "Some(7)", "Some(7)"),
        ("Some(4usize)", "Option<usize>", true, "Some(4)", "Some(4)"),
        (
            "Some(\"native\")",
            "Option<string>",
            false,
            "Some(native)",
            "Some(\"native\")",
        ),
    ] {
        assert_dynamic_option(
            eval_value(source).unwrap(),
            ty,
            inline,
            display,
            debug,
            "interpreter",
        );
        let compiled = compile(source).unwrap();
        assert_dynamic_option(
            compiled.execute_value().unwrap(),
            ty,
            inline,
            display,
            debug,
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute_value().unwrap(),
            ty,
            inline,
            display,
            debug,
            "loaded VM",
        );
    }
}

#[test]
fn options_of_native_containers_use_composed_layouts() {
    for (source, ty, present) in [
        ("Some(true)", "Option<bool>", true),
        ("Some('x')", "Option<char>", true),
        (
            "let value: Option<Option<i32>> = Some(Some(7)); value",
            "Option<Option<i32>>",
            true,
        ),
        (
            r#"
                let mut queue: VecDeque<i32> = VecDeque::new();
                queue.push_back(7);
                let value: Option<Option<VecDeque<i32>>> = Some(Some(queue));
                value
            "#,
            "Option<Option<VecDeque<i32>>>",
            true,
        ),
        (
            r#"
                let mut queue: VecDeque<i32> = VecDeque::new();
                queue.push_back(7);
                Some(queue)
            "#,
            "Option<VecDeque<i32>>",
            true,
        ),
        (
            "let value: Option<VecDeque<i32>> = None; value",
            "Option<VecDeque<i32>>",
            false,
        ),
        (
            r#"
                let mut heap: BinaryHeap<string> = BinaryHeap::new();
                heap.push("z");
                Some(heap)
            "#,
            "Option<BinaryHeap<string>>",
            true,
        ),
        (
            r#"
                let mut set: HashSet<i32> = HashSet::new();
                set.insert(7);
                Some(set)
            "#,
            "Option<HashSet<i32>>",
            true,
        ),
        (
            r#"
                let mut values: Vec<i32> = Vec::new();
                values.push(7);
                Some(values)
            "#,
            "Option<Vec<i32>>",
            true,
        ),
        (
            "let result: Result<i32, string> = Ok(7); Some(result)",
            "Option<Result<i32, string>>",
            true,
        ),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for (stage, value) in [
            ("interpreter", eval_value(source).unwrap()),
            ("VM", compiled.execute_value().unwrap()),
            ("loaded VM", loaded.execute_value().unwrap()),
        ] {
            let Value::Dynamic(object) = &value else {
                panic!(
                    "{ty} should use a native option layout in {stage}, found {}",
                    value.type_name()
                );
            };
            assert_eq!(object.descriptor().layout().rils_type().to_string(), ty);
            assert_eq!(value.as_option().unwrap().0.is_some(), present);
            assert_eq!(
                object.with(|payload| payload.is_some()).unwrap().unwrap(),
                present
            );
        }
    }
}

#[test]
fn options_with_unregistered_user_items_keep_legacy_storage() {
    let source = "struct Item { value: i32 } Some(Item { value: 7 })";
    assert!(matches!(eval_value(source).unwrap(), Value::Option { .. }));
    assert!(matches!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::Option { .. }
    ));
}

#[test]
fn options_move_native_containers_through_unwrap() {
    for (source, expected) in [
        (
            r#"
                let mut queue: VecDeque<i32> = VecDeque::new();
                queue.push_back(7);
                let value: Option<VecDeque<i32>> = Some(queue);
                let mut recovered = value.unwrap();
                recovered.pop_front().unwrap()
            "#,
            Value::from_i32(7),
        ),
        (
            r#"
                let mut heap: BinaryHeap<string> = BinaryHeap::new();
                heap.push("z");
                let value: Option<BinaryHeap<string>> = Some(heap);
                let mut recovered = value.unwrap();
                recovered.pop().unwrap()
            "#,
            Value::from_string("z"),
        ),
    ] {
        assert_eq!(eval_value(source).unwrap(), expected);
        assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
    }
}

#[test]
fn owned_some_moves_string_and_legacy_items_keep_their_value() {
    let source = "let text = \"moved\"; Some(text)";
    assert_dynamic_option(
        eval_value(source).unwrap(),
        "Option<string>",
        false,
        "Some(moved)",
        "Some(\"moved\")",
        "interpreter",
    );
    let compiled = compile(source).unwrap();
    assert_dynamic_option(
        compiled.execute_value().unwrap(),
        "Option<string>",
        false,
        "Some(moved)",
        "Some(\"moved\")",
        "VM",
    );
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_dynamic_option(
        loaded.execute_value().unwrap(),
        "Option<string>",
        false,
        "Some(moved)",
        "Some(\"moved\")",
        "loaded VM",
    );

    for value in [
        eval_value("Some(true)").unwrap(),
        compile("Some(true)").unwrap().execute_value().unwrap(),
    ] {
        assert!(matches!(value, Value::Dynamic(_)));
        assert_eq!(
            value.as_option(),
            Some((Some(Value::Bool(true)), rils::Type::Bool))
        );
    }
}

#[test]
fn numeric_option_families_use_generated_native_conversions() {
    for ty in [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64",
    ] {
        let option_type = format!("Option<{ty}>");
        for (source, display) in [
            (format!("Some(1{ty})"), "Some(1)"),
            (format!("let value: {option_type} = None; value"), "None"),
        ] {
            assert_dynamic_option(
                eval_value(&source).unwrap(),
                &option_type,
                true,
                display,
                display,
                "interpreter",
            );
            let compiled = compile(&source).unwrap();
            assert_dynamic_option(
                compiled.execute_value().unwrap(),
                &option_type,
                true,
                display,
                display,
                "VM",
            );
            let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
            assert_dynamic_option(
                loaded.execute_value().unwrap(),
                &option_type,
                true,
                display,
                display,
                "loaded VM",
            );
        }
        let methods = format!(
            "let some: {option_type} = Some(1{ty}); let none: {option_type} = None; some.is_some() && none.is_none()"
        );
        assert_eq!(eval_value(&methods).unwrap(), Value::Bool(true));
        let compiled = compile(&methods).unwrap();
        assert_eq!(compiled.execute_value().unwrap(), Value::Bool(true));
    }
}

#[test]
fn checked_integer_methods_return_native_options() {
    for ty in [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
    ] {
        let source = format!("1{ty}.checked_add(2{ty})");
        let option_type = format!("Option<{ty}>");
        assert_dynamic_option(
            eval_value(&source).unwrap(),
            &option_type,
            true,
            "Some(3)",
            "Some(3)",
            "interpreter",
        );
        let compiled = compile(&source).unwrap();
        assert_dynamic_option(
            compiled.execute_value().unwrap(),
            &option_type,
            true,
            "Some(3)",
            "Some(3)",
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute_value().unwrap(),
            &option_type,
            true,
            "Some(3)",
            "Some(3)",
            "loaded VM",
        );
    }

    for (source, expected) in [
        ("127i8.checked_add(1i8)", "None"),
        ("1i8.checked_add(2i8).unwrap()", "3"),
    ] {
        let interpreted = eval_value(source).unwrap();
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            interpreted,
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_eq!(value.to_string(), expected, "{source}");
            if expected == "None" {
                assert_dynamic_option(value, "Option<i8>", true, "None", "None", source);
            }
        }
    }
}

#[test]
fn string_methods_return_native_options() {
    for (source, ty, inline, display, debug) in [
        (
            "\"banana\".find(\"na\")",
            "Option<usize>",
            true,
            "Some(2)",
            "Some(2)",
        ),
        (
            "\"banana\".rfind(\"na\")",
            "Option<usize>",
            true,
            "Some(4)",
            "Some(4)",
        ),
        (
            "\"native\".strip_prefix(\"na\")",
            "Option<string>",
            false,
            "Some(tive)",
            "Some(\"tive\")",
        ),
        (
            "\"native\".strip_suffix(\"ve\")",
            "Option<string>",
            false,
            "Some(nati)",
            "Some(\"nati\")",
        ),
        (
            "\"native\".find(\"missing\")",
            "Option<usize>",
            true,
            "None",
            "None",
        ),
        (
            "\"native\".strip_prefix(\"missing\")",
            "Option<string>",
            false,
            "None",
            "None",
        ),
    ] {
        assert_dynamic_option(
            eval_value(source).unwrap(),
            ty,
            inline,
            display,
            debug,
            "interpreter",
        );
        let compiled = compile(source).unwrap();
        assert_dynamic_option(
            compiled.execute_value().unwrap(),
            ty,
            inline,
            display,
            debug,
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute_value().unwrap(),
            ty,
            inline,
            display,
            debug,
            "loaded VM",
        );
    }
}

#[test]
fn typed_none_preserves_option_semantics() {
    for (source, ty, inline) in [
        ("let value: Option<i8> = None; value", "Option<i8>", true),
        ("let value: Option<i32> = None; value", "Option<i32>", true),
        (
            "let value: Option<usize> = None; value",
            "Option<usize>",
            true,
        ),
        (
            "let value: Option<string> = None; value",
            "Option<string>",
            false,
        ),
        (
            "fn missing() -> Option<string> { None } missing()",
            "Option<string>",
            false,
        ),
        (
            "fn missing() -> Option<string> { return None; } missing()",
            "Option<string>",
            false,
        ),
        (
            "let value: Option<string> = <Option<string> as Default>::default(); value",
            "Option<string>",
            false,
        ),
    ] {
        assert_dynamic_option(
            eval_value(source).unwrap(),
            ty,
            inline,
            "None",
            "None",
            &format!("interpreter for `{source}`"),
        );
        let compiled = compile(source).unwrap();
        assert_dynamic_option(
            compiled.execute_value().unwrap(),
            ty,
            inline,
            "None",
            "None",
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute_value().unwrap(),
            ty,
            inline,
            "None",
            "None",
            "loaded VM",
        );
    }
}

#[test]
fn contextual_none_uses_native_storage_across_backends() {
    for (source, ty, inline) in [
        (
            "fn pass(value: Option<string>) -> Option<string> { value } pass(None)",
            "Option<string>",
            false,
        ),
        (
            "let mut value: Option<string> = Some(\"old\"); value = None; value",
            "Option<string>",
            false,
        ),
        (
            "let pair: (Option<string>, i32) = (None, 1i32); pair.0",
            "Option<string>",
            false,
        ),
        (
            "let values: [Option<i32>; 1] = [None]; values[0]",
            "Option<i32>",
            true,
        ),
        (
            "let values: [Option<i32>; 2] = [None; 2]; values[1]",
            "Option<i32>",
            true,
        ),
        (
            "let nested: Option<Option<string>> = Some(None); nested.unwrap()",
            "Option<string>",
            false,
        ),
        (
            "struct Holder { value: Option<string> } let holder = Holder { value: None }; holder.value",
            "Option<string>",
            false,
        ),
        (
            "fn nested() -> Option<Option<string>> { Some(None) } nested().unwrap()",
            "Option<string>",
            false,
        ),
        (
            "fn nested() -> Option<Option<string>> { return Some(None); } nested().unwrap()",
            "Option<string>",
            false,
        ),
        (
            "let value: Option<string> = if true { None } else { None }; value",
            "Option<string>",
            false,
        ),
        (
            "let value: Option<string> = match 1i32 { 1 => None, _ => None }; value",
            "Option<string>",
            false,
        ),
    ] {
        assert_dynamic_option(
            eval_value(source).unwrap(),
            ty,
            inline,
            "None",
            "None",
            &format!("interpreter for `{source}`"),
        );
        let compiled = compile(source).unwrap();
        assert_dynamic_option(
            compiled.execute_value().unwrap(),
            ty,
            inline,
            "None",
            "None",
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute_value().unwrap(),
            ty,
            inline,
            "None",
            "None",
            "loaded VM",
        );
    }
}

#[test]
fn native_options_work_as_collection_keys_and_match_values() {
    let source = include_str!("fixtures/native_option.rils");
    assert_eq!(eval_value(source).unwrap().as_i32(), Some(42));
    let compiled = compile(source).unwrap();
    assert_eq!(compiled.execute_value().unwrap().as_i32(), Some(42));
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap().as_i32(), Some(42));
}
