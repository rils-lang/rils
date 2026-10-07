use rils_bytecode::compile;
use rils_runtime::eval_value;

#[test]
fn exported_combinators_invoke_rils_functions_and_captured_closures() {
    let source = include_str!("fixtures/native_callbacks.rils");
    let expected = eval_value(source).expect("interpreter accepts callback fixture");
    let module = compile(source).expect("bytecode accepts callback fixture");
    assert_eq!(
        module.execute_value().expect("VM invokes callbacks"),
        expected
    );
    let loaded = rils_bytecode::BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(
        loaded.execute_value().expect("loaded VM invokes callbacks"),
        expected
    );
}

#[test]
fn callbacks_move_native_non_clone_payloads_and_preserve_scoped_references() {
    for (name, source) in [
        (
            "sum branches",
            include_str!("fixtures/native_sum_callbacks/owned.rils"),
        ),
        (
            "generic calls",
            include_str!("fixtures/native_sum_callbacks/generic.rils"),
        ),
        (
            "lexical references",
            include_str!("fixtures/native_sum_callbacks/references.rils"),
        ),
        (
            "free functions",
            include_str!("fixtures/native_sum_callbacks/free.rils"),
        ),
    ] {
        assert_eq!(
            eval_value(source).unwrap_or_else(|error| panic!("{name}, interpreter: {error}")),
            rils_execution::Value::from_i32(42)
        );
        let module = compile(source).unwrap_or_else(|error| panic!("{name}, compiler: {error}"));
        let loaded =
            rils_bytecode::BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for module in [module, loaded] {
            assert_eq!(
                module
                    .execute_value()
                    .unwrap_or_else(|error| panic!("{name}, VM: {error}")),
                rils_execution::Value::from_i32(42)
            );
        }
    }
}

#[test]
fn callback_results_retain_standard_error_declarations_on_both_branches() {
    use rils_execution::{Type, Value, value::dynamic_result};
    let mut host = rils_bytecode::BytecodeHost::standard();
    host.enable_standard_fs().unwrap();
    for (source, item, success) in [
        (
            include_str!("fixtures/native_sum_callbacks/fs_error.rils"),
            Type::USIZE,
            false,
        ),
        (
            include_str!("fixtures/native_sum_callbacks/fs_ok.rils"),
            Type::String,
            true,
        ),
    ] {
        let module = compile(source).unwrap();
        let loaded =
            rils_bytecode::BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            module.execute_value_with_host(&host).unwrap(),
            loaded.execute_value_with_host(&host).unwrap(),
        ] {
            assert!(matches!(&value, Value::Dynamic(_)));
            assert_eq!(
                Type::of_value(&value),
                Some(Type::Result(
                    Box::new(item.clone()),
                    Box::new(Type::named("std::io::Error"))
                ))
            );
            let branch = dynamic_result::take_owned(value).unwrap();
            assert_eq!(branch.is_ok(), success);
            if let Err(error) = branch {
                assert!(matches!(error, Value::Dynamic(_)));
            }
        }
    }
}

#[test]
fn callback_errors_preserve_their_source_span() {
    let source = "fn fail(value: i32) -> i32 { let absent: Option<i32> = None; absent.unwrap() } Some(1).map(fail)";
    let interpreted = eval_value(source).unwrap_err();
    let compiled = compile(source).unwrap().execute_value().unwrap_err();
    assert!(interpreted.to_string().contains("unwrap"));
    assert!(compiled.message.contains("unwrap"));
    assert_eq!(compiled.span, interpreted.span());
}

#[test]
fn callback_execution_counts_toward_the_vm_budget() {
    let source = "fn spin(value: i32) -> i32 { loop {} } Some(1).map(spin)";
    let error = compile(source)
        .unwrap()
        .execute_value_with_limit(64)
        .unwrap_err();
    assert!(error.message.contains("step limit"));
}
