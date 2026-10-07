use rils_bytecode::{BytecodeModule, compile};
use rils_execution::Value;
use rils_runtime::eval_value;

#[test]
fn native_sum_consumption_agrees_in_interpreter_vm_and_loaded_vm() {
    for (name, source) in [
        (
            "queries",
            include_str!("fixtures/native_sum_consumers/queries.rils"),
        ),
        (
            "patterns",
            include_str!("fixtures/native_sum_consumers/patterns.rils"),
        ),
        (
            "try_return",
            include_str!("fixtures/native_sum_consumers/try_return.rils"),
        ),
        (
            "copy",
            include_str!("fixtures/native_sum_consumers/copy.rils"),
        ),
        (
            "bindings",
            include_str!("fixtures/native_sum_consumers/bindings.rils"),
        ),
        (
            "keys",
            include_str!("fixtures/native_sum_consumers/keys.rils"),
        ),
    ] {
        assert_eq!(
            eval_value(source).unwrap_or_else(|error| panic!("{name}, interpreter: {error}")),
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
