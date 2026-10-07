use rils_bytecode::{BytecodeModule, compile};
use rils_execution::Value;
use rils_runtime::eval_value;

#[test]
fn native_comparisons_agree_in_interpreter_vm_and_loaded_vm() {
    for (name, source) in [
        (
            "nested",
            include_str!("fixtures/native_comparisons/nested.rils"),
        ),
        (
            "contains",
            include_str!("fixtures/native_comparisons/contains.rils"),
        ),
        (
            "scalar",
            include_str!("fixtures/native_comparisons/scalar.rils"),
        ),
        (
            "references",
            include_str!("fixtures/native_comparisons/references.rils"),
        ),
    ] {
        assert_eq!(
            eval_value(source).unwrap_or_else(|error| panic!("{name}, interpreter: {error:?}")),
            Value::from_i32(42)
        );
        let module = compile(source).unwrap_or_else(|error| panic!("{name}, compile: {error}"));
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for (backend, module) in [("VM", module), ("loaded VM", loaded)] {
            assert_eq!(
                module
                    .execute_value()
                    .unwrap_or_else(|error| panic!("{name}, {backend}: {error}")),
                Value::from_i32(42)
            );
        }
    }
}
