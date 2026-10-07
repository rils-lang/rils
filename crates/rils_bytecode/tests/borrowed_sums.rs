use rils_bytecode::{BytecodeModule, compile};
use rils_execution::Value;
use rils_runtime::eval_value;

#[test]
fn borrowed_sums_and_nested_patterns_match_in_every_backend() {
    for (name, source) in [
        (
            "branches",
            include_str!("fixtures/borrowed_sums/branches.rils"),
        ),
        (
            "generic",
            include_str!("fixtures/borrowed_sums/generic.rils"),
        ),
        (
            "containers",
            include_str!("fixtures/borrowed_sums/containers.rils"),
        ),
        (
            "references",
            include_str!("fixtures/borrowed_sums/references.rils"),
        ),
        (
            "cleanup",
            include_str!("fixtures/borrowed_sums/cleanup.rils"),
        ),
        (
            "parameter",
            include_str!("fixtures/borrowed_sums/parameter.rils"),
        ),
        (
            "wildcard",
            include_str!("fixtures/borrowed_sums/wildcard.rils"),
        ),
        (
            "indexed",
            include_str!("fixtures/borrowed_sums/indexed.rils"),
        ),
    ] {
        assert_eq!(
            eval_value(source).unwrap_or_else(|e| panic!("{name}, interpreter: {e}")),
            Value::from_i32(42)
        );
        let module = compile(source).unwrap_or_else(|e| panic!("{name}, compile: {e}"));
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for (backend, module) in [("VM", module), ("loaded VM", loaded)] {
            assert_eq!(
                module
                    .execute_value()
                    .unwrap_or_else(|e| panic!("{name}, {backend}: {e}")),
                Value::from_i32(42)
            );
        }
    }
}

#[test]
fn replacing_a_sum_with_borrowed_children_is_rejected_in_every_backend() {
    for source in [
        include_str!("fixtures/borrowed_sums/option_replacement.rils"),
        include_str!("fixtures/borrowed_sums/result_replacement.rils"),
    ] {
        let error = eval_value(source).unwrap_err().to_string();
        assert!(
            error.contains("BorrowedTarget") || error.contains("referenced"),
            "interpreter: {error}"
        );
        let module = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for module in [module, loaded] {
            let error = module.execute_value().unwrap_err().to_string();
            assert!(
                error.contains("BorrowedTarget") || error.contains("referenced"),
                "VM: {error}"
            );
        }
    }
}

#[test]
fn borrowed_payloads_cannot_escape_local_sources() {
    let source = include_str!("fixtures/borrowed_sums/escape.rils");
    assert!(eval_value(source).is_err());
    assert!(compile(source).is_err());
}
