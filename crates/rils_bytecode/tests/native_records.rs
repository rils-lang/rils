use rils_bytecode::{BytecodeModule, compile};
use rils_execution::{RilsValue, Type, Value};
use rils_runtime::eval_value;

fn run_all(source: &str) -> [Value; 3] {
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    [
        eval_value(source).unwrap(),
        module.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ]
}

#[test]
fn standalone_and_generic_records_retain_native_storage() {
    for (source, name) in [
        (include_str!("fixtures/native_records/stored.rils"), "Item"),
        (
            include_str!("fixtures/native_records/generic.rils"),
            "Holder<i32>",
        ),
        (
            include_str!("fixtures/native_records/returned.rils"),
            "Item",
        ),
        (include_str!("fixtures/native_records/empty.rils"), "Empty"),
    ] {
        for (backend, value) in run_all(source).into_iter().enumerate() {
            let Value::Dynamic(ref object) = value else {
                panic!("backend {backend}: retained legacy record: {value:?}");
            };
            assert!(!object.is_inline(), "references require a stable owner");
            assert_eq!(Type::of_value(&value).unwrap().to_string(), name);
            assert!(!value.is_copy(), "fields do not implicitly implement Copy");
        }
    }
    for value in run_all(include_str!("fixtures/native_records/stored.rils")) {
        let host = RilsValue::new(value);
        assert_eq!(host.field(0).unwrap().get_cloned::<i32>().unwrap(), 42);
        assert_eq!(
            host.field(1).unwrap().get_cloned::<String>().unwrap(),
            "owned"
        );
    }
}

#[test]
fn record_ownership_methods_and_patterns_agree_across_backends() {
    for (name, source, expected) in [
        (
            "partial move",
            include_str!("fixtures/native_records/partial.rils"),
            Value::from_i32(42),
        ),
        (
            "receivers",
            include_str!("fixtures/native_records/receivers.rils"),
            Value::from_i32(42),
        ),
        (
            "nested owned receiver",
            include_str!("fixtures/native_records/nested_receiver.rils"),
            Value::from_i32(42),
        ),
        (
            "owned receiver returns native composition",
            include_str!("fixtures/native_records/owned_return.rils"),
            Value::from_i32(42),
        ),
        (
            "owned pattern",
            include_str!("fixtures/native_records/owned_pattern.rils"),
            Value::from_i32(42),
        ),
        (
            "borrowed pattern",
            include_str!("fixtures/native_records/borrowed_pattern.rils"),
            Value::from_i32(42),
        ),
        (
            "Copy owners",
            include_str!("fixtures/native_records/copy.rils"),
            Value::from_i32(42),
        ),
        (
            "native collection receivers",
            include_str!("fixtures/native_records/collections.rils"),
            Value::from_i32(42),
        ),
        (
            "iterator scope cleanup",
            include_str!("fixtures/native_records/loop_cleanup.rils"),
            Value::from_i32(42),
        ),
        (
            "Clone owners",
            include_str!("fixtures/native_records/clone.rils"),
            Value::from_i32(42),
        ),
    ] {
        for value in run_all(source) {
            assert_eq!(value, expected, "{name}: {source}");
        }
    }
}

#[test]
fn native_collection_receivers_retain_element_leases() {
    let source = include_str!("fixtures/native_records/referenced_collection.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    let errors = [
        eval_value(source).unwrap_err().to_string(),
        module.execute_value().unwrap_err().to_string(),
        loaded.execute_value().unwrap_err().to_string(),
    ];
    for error in errors {
        assert!(error.contains("referenced"), "{error}");
    }
}
