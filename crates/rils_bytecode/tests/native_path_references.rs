use rils_bytecode::{BytecodeModule, compile};
use rils_execution::Value;
use rils_runtime::eval_value;

#[test]
fn nested_native_references_preserve_the_owner_in_all_backends() {
    for (name, source) in [
        (
            "vector",
            include_str!("fixtures/native_references/vector.rils"),
        ),
        (
            "nested",
            include_str!("fixtures/native_references/nested.rils"),
        ),
        (
            "aliases",
            include_str!("fixtures/native_references/aliases.rils"),
        ),
        (
            "indexed",
            include_str!("fixtures/native_references/indexed.rils"),
        ),
        (
            "borrowed_iterator",
            include_str!("fixtures/native_references/borrowed_iterator.rils"),
        ),
        (
            "parameter",
            include_str!("fixtures/native_references/parameter.rils"),
        ),
        (
            "method",
            include_str!("fixtures/native_references/method.rils"),
        ),
        (
            "nominal_copy",
            include_str!("fixtures/native_references/nominal_copy.rils"),
        ),
        (
            "trait_method",
            include_str!("fixtures/native_references/trait_method.rils"),
        ),
    ] {
        assert_eq!(
            eval_value(source).unwrap_or_else(|error| panic!("{name}: {error}")),
            Value::from_i32(42),
            "interpreter: {name}"
        );
        let module = compile(source).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            module
                .execute_value()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            Value::from_i32(42),
            "VM: {name}"
        );
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        assert_eq!(
            loaded
                .execute_value()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            Value::from_i32(42),
            "loaded VM: {name}"
        );
    }
}

#[test]
fn native_references_reject_parent_replacement_and_implicit_non_copy_moves() {
    for (source, messages) in [
        (
            include_str!("fixtures/native_references/parent_replacement.rils"),
            &["referenced", "BorrowedTarget"][..],
        ),
        (
            include_str!("fixtures/native_references/non_copy.rils"),
            &["Copy"][..],
        ),
        (
            include_str!("fixtures/native_references/nominal_non_copy.rils"),
            &["Copy"][..],
        ),
    ] {
        let error = eval_value(source).unwrap_err().to_string();
        assert!(
            messages.iter().any(|message| error.contains(message)),
            "interpreter: {error}"
        );
        let error = compile(source)
            .unwrap()
            .execute_value()
            .unwrap_err()
            .to_string();
        assert!(
            messages.iter().any(|message| error.contains(message)),
            "VM: {error}"
        );
    }
}
