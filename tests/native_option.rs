use rils::{BytecodeModule, Value, compile, eval};

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
            eval(source).unwrap(),
            ty,
            inline,
            display,
            debug,
            "interpreter",
        );
        let compiled = compile(source).unwrap();
        assert_dynamic_option(
            compiled.execute().unwrap(),
            ty,
            inline,
            display,
            debug,
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute().unwrap(),
            ty,
            inline,
            display,
            debug,
            "loaded VM",
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
                eval(&source).unwrap(),
                &option_type,
                true,
                display,
                display,
                "interpreter",
            );
            let compiled = compile(&source).unwrap();
            assert_dynamic_option(
                compiled.execute().unwrap(),
                &option_type,
                true,
                display,
                display,
                "VM",
            );
            let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
            assert_dynamic_option(
                loaded.execute().unwrap(),
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
        assert_eq!(eval(&methods).unwrap(), Value::Bool(true));
        let compiled = compile(&methods).unwrap();
        assert_eq!(compiled.execute().unwrap(), Value::Bool(true));
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
            eval(&source).unwrap(),
            &option_type,
            true,
            "Some(3)",
            "Some(3)",
            "interpreter",
        );
        let compiled = compile(&source).unwrap();
        assert_dynamic_option(
            compiled.execute().unwrap(),
            &option_type,
            true,
            "Some(3)",
            "Some(3)",
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute().unwrap(),
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
        let interpreted = eval(source).unwrap();
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            interpreted,
            compiled.execute().unwrap(),
            loaded.execute().unwrap(),
        ] {
            assert_eq!(value.to_string(), expected, "{source}");
            if expected == "None" {
                assert_dynamic_option(value, "Option<i8>", true, "None", "None", source);
            }
        }
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
            eval(source).unwrap(),
            ty,
            inline,
            "None",
            "None",
            "interpreter",
        );
        let compiled = compile(source).unwrap();
        assert_dynamic_option(
            compiled.execute().unwrap(),
            ty,
            inline,
            "None",
            "None",
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute().unwrap(),
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
            eval(source).unwrap(),
            ty,
            inline,
            "None",
            "None",
            "interpreter",
        );
        let compiled = compile(source).unwrap();
        assert_dynamic_option(
            compiled.execute().unwrap(),
            ty,
            inline,
            "None",
            "None",
            "VM",
        );
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_dynamic_option(
            loaded.execute().unwrap(),
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
    assert_eq!(eval(source).unwrap().as_i32(), Some(42));
    let compiled = compile(source).unwrap();
    assert_eq!(compiled.execute().unwrap().as_i32(), Some(42));
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute().unwrap().as_i32(), Some(42));
}
