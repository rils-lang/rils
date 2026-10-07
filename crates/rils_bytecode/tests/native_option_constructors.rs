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
fn constructors_and_defaults_keep_nominal_layouts_without_legacy_options() {
    for (name, source, present) in [
        (
            "owned",
            include_str!("fixtures/native_option_constructors/owned.rils"),
            true,
        ),
        (
            "none",
            include_str!("fixtures/native_option_constructors/none.rils"),
            false,
        ),
        (
            "generic",
            include_str!("fixtures/native_option_constructors/generic.rils"),
            true,
        ),
        (
            "generic_none",
            include_str!("fixtures/native_option_constructors/generic_none.rils"),
            false,
        ),
        (
            "default",
            include_str!("fixtures/native_option_constructors/default.rils"),
            false,
        ),
    ] {
        for value in run_all(name, source) {
            assert!(matches!(value, Value::Dynamic(_)), "{name}: {value:?}");
            assert_eq!(
                Type::of_value(&value),
                Some(Type::Option(Box::new(Type::named("Item"))))
            );
            assert!(
                !value.is_copy(),
                "{name}: Copy must be explicitly implemented"
            );
            assert_eq!(
                sum::branch(&value).unwrap(),
                Some(if present {
                    rils_execution::value::borrowed_sum::Branch::Some
                } else {
                    rils_execution::value::borrowed_sum::Branch::None
                }),
            );
        }
    }
}

#[test]
fn option_operations_move_non_clone_values_and_preserve_mutable_aliases() {
    for (name, source) in [
        (
            "operations",
            include_str!("fixtures/native_option_constructors/operations.rils"),
        ),
        (
            "references",
            include_str!("fixtures/native_option_constructors/references.rils"),
        ),
        (
            "inferred",
            include_str!("fixtures/native_option_constructors/inferred.rils"),
        ),
        (
            "trait_paths",
            include_str!("fixtures/native_option_constructors/trait_paths.rils"),
        ),
        (
            "blanket_trait_paths",
            include_str!("fixtures/native_option_constructors/blanket_trait_paths.rils"),
        ),
    ] {
        for value in run_all(name, source) {
            assert_eq!(value, Value::from_i32(42), "{name}");
        }
    }
}

#[test]
fn unresolved_none_requires_a_concrete_item_type() {
    let source = include_str!("fixtures/native_option_constructors/unresolved.rils");
    assert!(eval_value(source).is_err());
    assert!(compile(source).is_err());
}
