use rils_bytecode::{BytecodeModule, compile};
use rils_execution::{Type, Value};
use rils_runtime::eval_value;

fn run_all(source: &str) -> [Value; 3] {
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    [
        eval_value(source).unwrap_or_else(|e| panic!("interpreter: {e} from {source}")),
        module
            .execute_value()
            .unwrap_or_else(|e| panic!("VM: {e} from {source}")),
        loaded
            .execute_value()
            .unwrap_or_else(|e| panic!("loaded: {e} from {source}")),
    ]
}

#[test]
fn standalone_variants_and_generic_returns_keep_native_storage() {
    for (source, name, variant) in [
        (
            include_str!("fixtures/native_enums/unit.rils"),
            "Choice",
            "Empty",
        ),
        (
            include_str!("fixtures/native_enums/tuple.rils"),
            "Choice<string>",
            "Tuple",
        ),
        (
            include_str!("fixtures/native_enums/record.rils"),
            "Choice",
            "Record",
        ),
        (
            include_str!("fixtures/native_enums/generic_return.rils"),
            "Choice<i32>",
            "Tuple",
        ),
        (
            include_str!("fixtures/native_enums/generic_unit.rils"),
            "Choice<i32>",
            "Empty",
        ),
        (
            include_str!("fixtures/native_enums/explicit_unit.rils"),
            "Choice<i32>",
            "Empty",
        ),
    ] {
        for (backend, value) in run_all(source).into_iter().enumerate() {
            let Value::Dynamic(ref object) = value else {
                panic!("backend {backend}: legacy enum: {value:?} from {source}");
            };
            assert!(!object.is_inline());
            assert_eq!(Type::of_value(&value).unwrap().to_string(), name);
            assert!(!value.is_copy());
            let active = rils_execution::value::native_instance::enum_variant(&value)
                .unwrap()
                .unwrap();
            assert_eq!(active.name(), variant);
        }
    }
}

#[test]
fn enum_patterns_receivers_and_containers_agree_across_backends() {
    for (name, source) in [
        (
            "owned tuple and failed arm",
            include_str!("fixtures/native_enums/owned.rils"),
        ),
        (
            "borrowed record",
            include_str!("fixtures/native_enums/borrowed.rils"),
        ),
        (
            "nested enum",
            include_str!("fixtures/native_enums/nested.rils"),
        ),
        (
            "receivers",
            include_str!("fixtures/native_enums/receivers.rils"),
        ),
        (
            "containers",
            include_str!("fixtures/native_enums/containers.rils"),
        ),
        (
            "explicit Copy",
            include_str!("fixtures/native_enums/copy.rils"),
        ),
        ("Clone", include_str!("fixtures/native_enums/clone.rils")),
        (
            "generic patterns",
            include_str!("fixtures/native_enums/generic_pattern.rils"),
        ),
        (
            "generic references",
            include_str!("fixtures/native_enums/references.rils"),
        ),
        (
            "structural keys",
            include_str!("fixtures/native_enums/hash.rils"),
        ),
        (
            "declaration identity",
            include_str!("fixtures/native_enums/modules.rils"),
        ),
        (
            "Iterator and blanket IntoIterator",
            include_str!("fixtures/native_enums/iterator.rils"),
        ),
        (
            "pattern leases on loop exits",
            include_str!("fixtures/native_enums/loop_exits.rils"),
        ),
    ] {
        for value in run_all(source) {
            assert_eq!(value.as_i32(), Some(42), "{name}: {source}");
        }
    }
}

#[test]
fn derived_debug_reads_each_native_variant_shape() {
    use std::{cell::RefCell, rc::Rc};
    let source = include_str!("fixtures/native_enums/formatting.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    let output = Rc::new(RefCell::new(String::new()));
    let capture = output.clone();
    let handler = move |text: &str, newline: bool| {
        capture.borrow_mut().push_str(text);
        if newline {
            capture.borrow_mut().push('\n');
        }
        Ok(())
    };
    let mut engine = rils_runtime::Engine::new();
    engine.set_output_handler(handler.clone());
    let mut host = rils_bytecode::BytecodeHost::standard();
    host.set_output_handler(handler).unwrap();
    let expected = "Choice::Empty\nChoice::Tuple(\"owned\", 42)\nChoice::Record { text: \"owned\", value: 42 }\n";
    assert_eq!(engine.eval_value(source).unwrap().as_i32(), Some(42));
    assert_eq!(*output.borrow(), expected);
    for module in [module, loaded] {
        output.borrow_mut().clear();
        assert_eq!(
            module.execute_value_with_host(&host).unwrap().as_i32(),
            Some(42)
        );
        assert_eq!(*output.borrow(), expected);
    }
}

#[test]
fn enum_copy_is_explicit_and_checks_inactive_variants() {
    for source in [
        include_str!("fixtures/native_enums/non_copy.rils"),
        include_str!("fixtures/native_enums/invalid_copy.rils"),
    ] {
        assert!(compile(source).is_err(), "{source}");
        assert!(eval_value(source).is_err(), "{source}");
    }
}
