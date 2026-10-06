use rils::analysis::{DiagnosticSeverity, analyze};
use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn explicit_copy_survives_compilation_and_image_loading() {
    for (name, source) in [
        (
            "derived",
            include_str!("fixtures/explicit_copy/derived.rils"),
        ),
        ("manual", include_str!("fixtures/explicit_copy/manual.rils")),
        (
            "forward impl",
            include_str!("fixtures/explicit_copy/forward_impl.rils"),
        ),
    ] {
        assert_eq!(
            eval_value(source).unwrap(),
            Value::from_i32(42),
            "{name}: interpreter"
        );
        let module = compile(source).unwrap();
        assert_eq!(
            module.execute_value().unwrap(),
            Value::from_i32(42),
            "{name}: VM"
        );
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        assert_eq!(
            loaded.execute_value().unwrap(),
            Value::from_i32(42),
            "{name}: loaded VM"
        );
    }
}

#[test]
fn nominal_copy_requires_an_explicit_validated_declaration() {
    for (name, source, expected) in [
        (
            "struct",
            include_str!("fixtures/explicit_copy/unmarked_struct.rils"),
            "moved",
        ),
        (
            "enum",
            include_str!("fixtures/explicit_copy/unmarked_enum.rils"),
            "moved",
        ),
        (
            "empty",
            include_str!("fixtures/explicit_copy/empty.rils"),
            "moved",
        ),
        (
            "Clone only",
            include_str!("fixtures/explicit_copy/clone_only.rils"),
            "moved",
        ),
        (
            "inactive variant",
            include_str!("fixtures/explicit_copy/inactive_noncopy.rils"),
            "non-Copy fields",
        ),
        (
            "nested unmarked",
            include_str!("fixtures/explicit_copy/nested_unmarked.rils"),
            "non-Copy fields",
        ),
        (
            "missing Clone",
            include_str!("fixtures/explicit_copy/missing_clone.rils"),
            "supertrait `Clone`",
        ),
        (
            "partial generic impl",
            include_str!("fixtures/explicit_copy/partial_generic.rils"),
            "conditional Copy",
        ),
        (
            "native Rc is non-Copy",
            include_str!("fixtures/explicit_copy/rc_field.rils"),
            "non-Copy fields",
        ),
    ] {
        let analysis = analyze(source).unwrap();
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(
                    |diagnostic| diagnostic.severity == DiagnosticSeverity::Error
                        && diagnostic.message.contains(expected)
                ),
            "{name}: {:?}",
            analysis.diagnostics
        );
        assert!(
            eval_value(source)
                .unwrap_err()
                .to_string()
                .contains(expected),
            "{name}: interpreter"
        );
        assert!(
            compile(source)
                .err()
                .unwrap()
                .to_string()
                .contains(expected),
            "{name}: compiler"
        );
    }
}
