use rils_bytecode::{BytecodeModule, compile};
use rils_execution::Value;
use rils_runtime::eval_value;

#[test]
fn native_reference_and_callable_fields_work_in_all_backends() {
    for (name, source) in [
        (
            "optional_aliases",
            include_str!("fixtures/runtime_native_fields/optional_aliases.rils"),
        ),
        (
            "returned_reference",
            include_str!("fixtures/runtime_native_fields/returned_reference.rils"),
        ),
        (
            "generic_reference",
            include_str!("fixtures/runtime_native_fields/generic_reference.rils"),
        ),
        (
            "vector_references",
            include_str!("fixtures/runtime_native_fields/vector_references.rils"),
        ),
        (
            "optional_callback",
            include_str!("fixtures/runtime_native_fields/optional_callback.rils"),
        ),
        (
            "vector_callbacks",
            include_str!("fixtures/runtime_native_fields/vector_callbacks.rils"),
        ),
        (
            "result_callback",
            include_str!("fixtures/runtime_native_fields/result_callback.rils"),
        ),
        (
            "callback_state",
            include_str!("fixtures/runtime_native_fields/callback_state.rils"),
        ),
        (
            "owned_method",
            include_str!("fixtures/runtime_native_fields/owned_method.rils"),
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
fn composed_native_references_do_not_escape_or_allow_source_moves() {
    for (name, source) in [
        (
            "local_escape",
            include_str!("fixtures/runtime_native_fields/local_escape.rils"),
        ),
        (
            "source_move",
            include_str!("fixtures/runtime_native_fields/source_move.rils"),
        ),
        (
            "closure_capture",
            include_str!("fixtures/runtime_native_fields/closure_capture.rils"),
        ),
        (
            "bound_method_escape",
            include_str!("fixtures/runtime_native_fields/bound_method_escape.rils"),
        ),
    ] {
        let check_error = |error: String| {
            let message = error.to_lowercase();
            assert!(
                message.contains("referenc") || message.contains("borrow"),
                "{name}: {error}"
            );
        };
        check_error(eval_value(source).unwrap_err().to_string());
        match compile(source) {
            Ok(module) => {
                check_error(module.execute_value().unwrap_err().to_string());
                let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
                check_error(loaded.execute_value().unwrap_err().to_string());
            }
            Err(error) => check_error(error.to_string()),
        }
    }
}
