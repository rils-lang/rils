use rils_bytecode::{BytecodeModule, compile};
use rils_execution::{Type, Value, value::sum};
use rils_runtime::eval_value;

fn run_all(name: &str, source: &str) -> [Value; 3] {
    let interpreted =
        eval_value(source).unwrap_or_else(|error| panic!("{name}, interpreter: {error}"));
    let module = compile(source).unwrap_or_else(|error| panic!("{name}, compile: {error}"));
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    [
        interpreted,
        module
            .execute_value()
            .unwrap_or_else(|error| panic!("{name}, VM: {error}")),
        loaded
            .execute_value()
            .unwrap_or_else(|error| panic!("{name}, loaded VM: {error}")),
    ]
}

#[test]
fn constructors_move_non_clone_payloads_and_keep_both_type_witnesses() {
    for (name, source, success) in [
        (
            "ok",
            include_str!("fixtures/native_result_constructors/ok.rils"),
            true,
        ),
        (
            "err",
            include_str!("fixtures/native_result_constructors/err.rils"),
            false,
        ),
        (
            "generic_ok",
            include_str!("fixtures/native_result_constructors/generic_ok.rils"),
            true,
        ),
        (
            "generic_err",
            include_str!("fixtures/native_result_constructors/generic_err.rils"),
            false,
        ),
    ] {
        for value in run_all(name, source) {
            assert!(matches!(value, Value::Dynamic(_)), "{name}: {value:?}");
            assert_eq!(
                Type::of_value(&value),
                Some(Type::Result(
                    Box::new(Type::named("Item")),
                    Box::new(Type::named("Failure"))
                ))
            );
            assert!(!value.is_copy(), "Copy requires an explicit implementation");
            assert_eq!(
                sum::branch(&value).unwrap(),
                Some(if success {
                    rils_execution::value::borrowed_sum::Branch::Ok
                } else {
                    rils_execution::value::borrowed_sum::Branch::Err
                })
            );
        }
    }
}

#[test]
fn result_contexts_and_callbacks_agree_across_backends() {
    for (name, source) in [
        (
            "inferred",
            include_str!("fixtures/native_result_constructors/inferred.rils"),
        ),
        (
            "try_context",
            include_str!("fixtures/native_result_constructors/try_context.rils"),
        ),
        (
            "references",
            include_str!("fixtures/native_result_constructors/references.rils"),
        ),
        (
            "callbacks",
            include_str!("fixtures/native_result_constructors/callbacks.rils"),
        ),
        (
            "qualified",
            include_str!("fixtures/native_result_constructors/qualified.rils"),
        ),
        (
            "shadowed",
            include_str!("fixtures/native_result_constructors/shadowed.rils"),
        ),
    ] {
        for value in run_all(name, source) {
            assert_eq!(value, Value::from_i32(42), "{name}");
        }
    }
}

#[test]
fn constructors_reject_an_unresolved_inactive_branch() {
    for source in [
        include_str!("fixtures/native_result_constructors/unresolved_ok.rils"),
        include_str!("fixtures/native_result_constructors/unresolved_err.rils"),
    ] {
        let runtime = eval_value(source).unwrap_err();
        assert!(runtime.to_string().contains("cannot infer"), "{runtime}");
        let compile = compile(source).err().expect("unresolved Result must fail");
        assert!(compile.message.contains("cannot infer"), "{compile}");
    }
}
