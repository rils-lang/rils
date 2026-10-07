use rils_bytecode::{BytecodeModule, compile};
use rils_execution::{RilsValue, Type, Value, value::dynamic_result};
use rils_runtime::eval_value;

#[test]
fn generated_sum_methods_move_non_clone_items_in_every_backend() {
    for (name, source) in [
        (
            "owned",
            include_str!("fixtures/native_sum_methods/owned.rils"),
        ),
        (
            "mutable",
            include_str!("fixtures/native_sum_methods/mutable.rils"),
        ),
        (
            "generic native return",
            include_str!("fixtures/native_sum_methods/generic_return.rils"),
        ),
    ] {
        assert_eq!(
            eval_value(source).unwrap_or_else(|error| panic!("{name}, interpreter: {error}")),
            Value::from_i32(42)
        );
        let module = compile(source).unwrap_or_else(|error| panic!("{name}, compile: {error}"));
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for module in [module, loaded] {
            assert_eq!(
                module
                    .execute_value()
                    .unwrap_or_else(|error| panic!("{name}, VM: {error}")),
                Value::from_i32(42)
            );
        }
    }
}

#[test]
fn standard_fs_results_keep_native_storage_and_both_type_witnesses() {
    let mut host = rils_bytecode::BytecodeHost::standard();
    host.enable_standard_fs().unwrap();
    for (source, ok_type, success) in [
        (
            include_str!("fixtures/native_sum_methods/fs_error.rils"),
            Type::String,
            false,
        ),
        (
            include_str!("fixtures/native_sum_methods/fs_ok.rils"),
            Type::Bool,
            true,
        ),
    ] {
        let module = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            module.execute_value_with_host(&host).unwrap(),
            loaded.execute_value_with_host(&host).unwrap(),
        ] {
            assert!(matches!(&value, Value::Dynamic(_)));
            assert_eq!(
                Type::of_value(&value),
                Some(Type::Result(
                    Box::new(ok_type.clone()),
                    Box::new(Type::named("std::io::Error"))
                )),
            );
            let branch = dynamic_result::take_owned(value).unwrap();
            assert_eq!(branch.is_ok(), success);
            if let Err(error) = branch {
                assert!(matches!(&error, Value::Dynamic(_)));
                RilsValue::new(error)
                    .field(2)
                    .unwrap()
                    .with_native_view(|view| {
                        assert_eq!(
                            view.layout().unwrap().rils_type(),
                            &Type::Option(Box::new(Type::String))
                        );
                        assert!(view.option_is_some().unwrap());
                    })
                    .unwrap();
            }
        }
    }
}

#[test]
fn mutable_sum_methods_reject_live_payload_references() {
    for source in [
        include_str!("fixtures/native_sum_methods/borrowed_take.rils"),
        include_str!("fixtures/native_sum_methods/borrowed_replace.rils"),
    ] {
        assert!(
            eval_value(source)
                .unwrap_err()
                .to_string()
                .contains("referenced")
        );
        let module = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for module in [module, loaded] {
            assert!(
                module
                    .execute_value()
                    .unwrap_err()
                    .message
                    .contains("referenced")
            );
        }
    }
}

#[test]
fn generated_sum_failures_preserve_messages_and_source_spans() {
    for (source, expected) in [
        (
            include_str!("fixtures/native_sum_methods/none_unwrap.rils"),
            "called `unwrap` on `None`",
        ),
        (
            include_str!("fixtures/native_sum_methods/none_expect.rils"),
            "missing",
        ),
        (
            include_str!("fixtures/native_sum_methods/err_unwrap.rils"),
            "called `unwrap` on Err(bad)",
        ),
        (
            include_str!("fixtures/native_sum_methods/err_expect.rils"),
            "failed: bad",
        ),
        (
            include_str!("fixtures/native_sum_methods/ok_unwrap_err.rils"),
            "called `unwrap_err` on Ok(7)",
        ),
        (
            include_str!("fixtures/native_sum_methods/ok_expect_err.rils"),
            "unexpected: 7",
        ),
    ] {
        let interpreted = eval_value(source).unwrap_err();
        assert!(interpreted.to_string().contains(expected));
        let module = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for module in [module, loaded] {
            let error = module.execute_value().unwrap_err();
            assert!(error.message.contains(expected));
            assert_eq!(error.span, interpreted.span());
        }
    }
}
