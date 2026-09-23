use std::rc::Rc;

use rils_builtins::BuiltinId;
use rils_execution::{Type, Value, runtime_builtins};

#[test]
fn native_option_is_some_handles_variants_and_invalid_receivers() {
    for (input, is_some) in [
        (
            Value::Option {
                value: Some(Rc::new(Value::I32(5))),
                element_type: Some(Type::I32),
            },
            true,
        ),
        (
            Value::Option {
                value: None,
                element_type: Some(Type::I32),
            },
            false,
        ),
    ] {
        assert_eq!(
            runtime_builtins::call(BuiltinId::OptionIsSome, std::slice::from_ref(&input)),
            Ok(Value::Bool(is_some))
        );
        assert_eq!(
            runtime_builtins::call(BuiltinId::OptionIsNone, &[input]),
            Ok(Value::Bool(!is_some))
        );
    }
    let error = runtime_builtins::call(BuiltinId::OptionIsSome, &[Value::I32(5)]).unwrap_err();
    assert!(error.contains("expects Option"));
    let error = runtime_builtins::call(BuiltinId::OptionIsSome, &[]).unwrap_err();
    assert!(error.contains("one receiver"));
}
