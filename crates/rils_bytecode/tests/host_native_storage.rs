use std::collections::HashSet;

use rils_bytecode::{BytecodeHost, BytecodeModule, HostContract, compile_with_host};
use rils_execution::{FunctionSignature, Type, Value, value::owned_sum};
use rils_runtime::{Engine, OpaqueHostHandle, opaque_host_handle, opaque_host_value_typed};

fn contract() -> HostContract {
    let mut contract = HostContract::new();
    contract
        .register_type(
            "host::Item",
            None::<String>,
            rils_bytecode::HostTypeTransport::HostHandle,
        )
        .unwrap();
    contract
        .register_function(
            1,
            "host::item",
            FunctionSignature::fixed(vec![], Type::named("host::Item")),
            "host",
        )
        .unwrap();
    contract
}

fn item() -> Value {
    opaque_host_value_typed(
        OpaqueHostHandle {
            object_id: 42,
            generation: 3,
            type_id: 7,
        },
        "host::Item",
        HashSet::new(),
    )
}

#[test]
fn enum_copy_checks_inactive_host_payloads_before_vm_execution() {
    let source = include_str!("fixtures/copy/host_enum.rils");
    let contract = contract();
    let mut engine = Engine::new();
    let handle = engine.register_native_type("host", "Item").unwrap();
    let mut host = BytecodeHost::standard();
    host.allow_capability("host");
    host.register_host_type(handle.runtime_declaration())
        .unwrap();
    host.register_host_contract(&contract).unwrap();
    host.register_function(
        "host::item",
        FunctionSignature::fixed(vec![], Type::named("host::Item")),
        "host",
        move |_| {
            Ok(handle.value(OpaqueHostHandle {
                object_id: 42,
                generation: 3,
                type_id: 7,
            }))
        },
    )
    .unwrap();
    let module = compile_with_host(source, &contract).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for module in [&module, &loaded] {
        assert!(
            module
                .execute_value_with_host(&host)
                .unwrap_err()
                .message
                .contains("non-Copy fields")
        );
    }
    let mut portable = BytecodeHost::standard();
    portable.allow_capability("host");
    portable.register_host_contract(&contract).unwrap();
    portable
        .register_function(
            "host::item",
            FunctionSignature::fixed(vec![], Type::named("host::Item")),
            "host",
            |_| Ok(item()),
        )
        .unwrap();
    assert_eq!(
        loaded
            .execute_value_with_host(&portable)
            .unwrap()
            .to_string(),
        "Choice::Empty"
    );
}

fn check_native(value: Value, name: &str, copy: bool) {
    assert!(
        matches!(value, Value::Dynamic(_)),
        "{name}: {}",
        value.type_name()
    );
    assert_eq!(value.is_copy(), copy, "{name}: Copy policy");
    let value = owned_sum::materialize(value, &[], &[]).unwrap();
    if name == "none" || name == "default" {
        assert!(matches!(value, Value::Option { value: None, .. }));
    } else if name == "result_error" {
        assert!(matches!(value, Value::Result { value: Err(_), .. }));
    } else if name == "nested_record" {
        let Value::Option {
            value: Some(value), ..
        } = value
        else {
            panic!("record option")
        };
        let Value::Dynamic(record) = value.as_ref() else {
            panic!("record lost native storage");
        };
        assert!(
            matches!(record.descriptor().layout().rils_type(), Type::Named { name, .. } if name == "Holder")
        );
    } else {
        let Value::Option {
            value: Some(value), ..
        } = value
        else {
            panic!("host option")
        };
        assert_eq!(opaque_host_handle(&value).unwrap().object_id, 42, "{name}");
    }
}

#[test]
fn host_context_composes_storage_in_interpreter_vm_and_loaded_bytecode() {
    let fixtures = [
        (
            "parameter_return",
            include_str!("fixtures/host_native_storage/parameter_return.rils"),
        ),
        (
            "none",
            include_str!("fixtures/host_native_storage/none.rils"),
        ),
        (
            "default",
            include_str!("fixtures/host_native_storage/default.rils"),
        ),
        (
            "result_error",
            include_str!("fixtures/host_native_storage/result_error.rils"),
        ),
        (
            "vector",
            include_str!("fixtures/host_native_storage/vector.rils"),
        ),
        (
            "array_conversion",
            include_str!("fixtures/host_native_storage/array_conversion.rils"),
        ),
        (
            "boxed",
            include_str!("fixtures/host_native_storage/boxed.rils"),
        ),
        (
            "ref_cell",
            include_str!("fixtures/host_native_storage/ref_cell.rils"),
        ),
        (
            "callback",
            include_str!("fixtures/host_native_storage/callback.rils"),
        ),
        (
            "nested_record",
            include_str!("fixtures/host_native_storage/nested_record.rils"),
        ),
    ];
    for copy in [true, false] {
        for (name, source) in fixtures {
            let contract = contract();
            let mut engine = Engine::new();
            let mut host = BytecodeHost::standard();
            host.allow_capability("host");
            if copy {
                engine.register_host_contract_types(&contract).unwrap();
                engine
                    .register_module_typed_function(
                        "host",
                        "item",
                        vec![],
                        Type::named("host::Item"),
                        |_| Ok(item()),
                    )
                    .unwrap();
                host.register_host_contract(&contract).unwrap();
                host.register_function(
                    "host::item",
                    FunctionSignature::fixed(vec![], Type::named("host::Item")),
                    "host",
                    |_| Ok(item()),
                )
                .unwrap();
            } else {
                let handle = engine.register_native_type("host", "Item").unwrap();
                host.register_host_type(handle.runtime_declaration())
                    .unwrap();
                host.register_host_contract(&contract).unwrap();
                let interpreter_handle = handle.clone();
                engine
                    .register_module_typed_function(
                        "host",
                        "item",
                        vec![],
                        Type::named("host::Item"),
                        move |_| {
                            Ok(interpreter_handle.value(OpaqueHostHandle {
                                object_id: 42,
                                generation: 3,
                                type_id: 7,
                            }))
                        },
                    )
                    .unwrap();
                host.register_function(
                    "host::item",
                    FunctionSignature::fixed(vec![], Type::named("host::Item")),
                    "host",
                    move |_| {
                        Ok(handle.value(OpaqueHostHandle {
                            object_id: 42,
                            generation: 3,
                            type_id: 7,
                        }))
                    },
                )
                .unwrap();
            }
            check_native(
                engine
                    .eval_value(source)
                    .unwrap_or_else(|error| panic!("interpreter {name} Copy={copy}: {error}")),
                name,
                copy && name != "nested_record",
            );
            let module = compile_with_host(source, &contract)
                .unwrap_or_else(|error| panic!("compile {name}: {error}"));
            check_native(
                module
                    .execute_value_with_host(&host)
                    .unwrap_or_else(|error| panic!("VM {name} Copy={copy}: {error}")),
                name,
                copy && name != "nested_record",
            );
            let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
            check_native(
                loaded
                    .execute_value_with_host(&host)
                    .unwrap_or_else(|error| panic!("loaded VM {name} Copy={copy}: {error}")),
                name,
                copy && name != "nested_record",
            );
        }
    }
}

#[test]
fn manifest_inline_values_enums_and_raw_flags_keep_their_declared_layouts() {
    use rils_bytecode::{HostTypeTransport, HostValueLayout};
    use rils_runtime::{
        host_enum_raw, host_enum_value, inline_host_value, inline_host_value_typed,
    };

    for kind in ["handle", "inline", "enum", "flags"] {
        let mut contract =
            HostContract::with_versions(rils_bytecode::BYTECODE_HOST_ABI_VERSION, 2).unwrap();
        match kind {
            "handle" => contract
                .register_type("host::Data", None::<String>, HostTypeTransport::HostHandle)
                .unwrap(),
            "inline" => contract
                .register_value_type("host::Data", HostValueLayout::F32x2)
                .unwrap(),
            _ => contract
                .register_enum_type(
                    "host::Data",
                    rils_execution::IntegerType::U32,
                    kind == "flags",
                    [("Ready".into(), 1)],
                )
                .unwrap(),
        }
        let signature = FunctionSignature::fixed(vec![], Type::named("host::Data"));
        contract
            .register_function(1, "host::value", signature.clone(), "host")
            .unwrap();
        let definition = contract
            .host_type("host::Data")
            .unwrap()
            .enum_definition
            .clone();
        let make = move || match kind {
            "handle" => opaque_host_value_typed(
                OpaqueHostHandle {
                    object_id: 42,
                    generation: 1,
                    type_id: 1,
                },
                "host::Data",
                HashSet::new(),
            ),
            "inline" => inline_host_value_typed([42; 16], "host::Data"),
            _ => host_enum_value(
                "host::Data",
                definition.as_ref().unwrap(),
                if kind == "flags" { 7 } else { 1 },
            )
            .unwrap(),
        };
        let mut engine = Engine::new();
        engine.register_host_contract_types(&contract).unwrap();
        let engine_make = make.clone();
        engine
            .register_module_typed_function(
                "host",
                "value",
                vec![],
                Type::named("host::Data"),
                move |_| Ok(engine_make()),
            )
            .unwrap();
        let mut host = BytecodeHost::standard();
        host.allow_capability("host");
        host.register_host_contract(&contract).unwrap();
        host.register_host_contract(&contract).unwrap();
        host.register_function("host::value", signature, "host", move |_| Ok(make()))
            .unwrap();
        let source = include_str!("fixtures/host_native_storage/transport.rils");
        let module = compile_with_host(source, &contract).unwrap();
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for value in [
            engine.eval_value(source).unwrap(),
            module.execute_value_with_host(&host).unwrap(),
            loaded.execute_value_with_host(&host).unwrap(),
        ] {
            assert!(matches!(value, Value::Dynamic(_)), "{kind}");
            assert!(value.is_copy(), "{kind}");
            let value = owned_sum::materialize(value, &[], &[]).unwrap();
            let Value::Option {
                value: Some(value), ..
            } = value
            else {
                panic!("{kind}: optional transport")
            };
            match kind {
                "handle" => assert_eq!(opaque_host_handle(&value).unwrap().object_id, 42),
                "inline" => assert_eq!(inline_host_value(&value).unwrap().bytes, [42; 16]),
                _ => assert_eq!(
                    host_enum_raw(
                        &value,
                        "host::Data",
                        contract
                            .host_type("host::Data")
                            .unwrap()
                            .enum_definition
                            .as_ref()
                            .unwrap()
                    )
                    .unwrap(),
                    if kind == "flags" { 7 } else { 1 }
                ),
            }
        }
    }
}

#[test]
fn rejected_host_contracts_do_not_change_existing_layouts() {
    let contract = contract();
    let mut host = BytecodeHost::standard();
    host.allow_capability("host");
    host.register_host_contract(&contract).unwrap();
    host.register_function(
        "host::item",
        FunctionSignature::fixed(vec![], Type::named("host::Item")),
        "host",
        |_| Ok(item()),
    )
    .unwrap();
    let mut conflict = HostContract::new();
    conflict
        .register_value_type("host::Item", rils_bytecode::HostValueLayout::F32x2)
        .unwrap();
    assert!(host.register_host_contract(&conflict).is_err());
    let source = include_str!("fixtures/host_native_storage/parameter_return.rils");
    let module = compile_with_host(source, &contract).unwrap();
    check_native(
        module.execute_value_with_host(&host).unwrap(),
        "parameter_return",
        true,
    );
}

#[test]
fn composed_base_types_preserve_the_actual_host_identity() {
    let mut contract = HostContract::new();
    contract
        .register_type(
            "host::Base",
            None::<String>,
            rils_bytecode::HostTypeTransport::HostHandle,
        )
        .unwrap();
    contract
        .register_type(
            "host::Data",
            Some("host::Base"),
            rils_bytecode::HostTypeTransport::HostHandle,
        )
        .unwrap();
    let signature = FunctionSignature::fixed(vec![], Type::named("host::Base"));
    contract
        .register_function(1, "host::value", signature.clone(), "host")
        .unwrap();
    let make = || {
        opaque_host_value_typed(
            OpaqueHostHandle {
                object_id: 42,
                generation: 1,
                type_id: 2,
            },
            "host::Data",
            HashSet::from(["host::Base".into()]),
        )
    };
    let mut engine = Engine::new();
    engine.register_host_contract_types(&contract).unwrap();
    engine
        .register_module_typed_function(
            "host",
            "value",
            vec![],
            Type::named("host::Base"),
            move |_| Ok(make()),
        )
        .unwrap();
    let mut host = BytecodeHost::standard();
    host.allow_capability("host");
    host.register_host_contract(&contract).unwrap();
    host.register_function("host::value", signature, "host", move |_| Ok(make()))
        .unwrap();
    let source = include_str!("fixtures/host_native_storage/base_type.rils");
    let module = compile_with_host(source, &contract).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for value in [
        engine.eval_value(source).unwrap(),
        module.execute_value_with_host(&host).unwrap(),
        loaded.execute_value_with_host(&host).unwrap(),
    ] {
        assert!(matches!(value, Value::Dynamic(_)));
        assert_eq!(
            Type::of_value(&value),
            Some(Type::Option(Box::new(Type::named("host::Base"))))
        );
        let value = owned_sum::materialize(value, &[], &[]).unwrap();
        let Value::Option {
            value: Some(value), ..
        } = value
        else {
            panic!("base option")
        };
        assert_eq!(value.type_name(), "host::Data");
        assert_eq!(opaque_host_handle(&value).unwrap().type_id, 2);
    }
}
