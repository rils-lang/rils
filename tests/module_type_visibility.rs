use rils::{compile, eval_value};

#[test]
fn layout_registry_does_not_make_private_source_types_accessible() {
    for source in [
        include_str!("fixtures/native_assignment_storage/module_private_access.rils"),
        include_str!("fixtures/native_assignment_storage/module_private_annotation.rils"),
        include_str!("fixtures/native_assignment_storage/module_private_enum.rils"),
    ] {
        assert!(
            eval_value(source).is_err(),
            "accepted private type: {source}"
        );
        let error = compile(source)
            .err()
            .expect("private type must be rejected before VM execution");
        assert!(error.message.contains("private"), "{error}");
        assert!(error.span.end > error.span.start);
    }
}
