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
fn typed_none_preserves_option_semantics() {
    let source = "let value: Option<string> = None; value";
    let interpreted = eval(source).unwrap();
    assert_eq!(interpreted.to_string(), "None");
    assert!(matches!(interpreted, Value::Dynamic(_)));
    let compiled = compile(source).unwrap();
    let executed = compiled.execute().unwrap();
    assert_eq!(executed.to_string(), "None");
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute().unwrap().to_string(), "None");
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
