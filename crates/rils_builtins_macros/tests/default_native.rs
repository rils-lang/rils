#[path = "fixtures/default_native.rs"]
#[macro_use]
mod fixture;

#[derive(Debug, PartialEq)]
pub enum Value {
    Usize(usize),
    Bool(bool),
}

// The execution crate supplies the receiver conversion. The macro supplies
// the method call, so an additional query needs no method-name registration.
mod vector {
    use super::{Value, fixture};

    pub trait Output {
        fn into_value(self) -> Value;
    }
    impl Output for usize {
        fn into_value(self) -> Value {
            Value::Usize(self)
        }
    }
    impl Output for bool {
        fn into_value(self) -> Value {
            Value::Bool(self)
        }
    }
    pub fn with_shared<R: Output>(
        arguments: &[Value],
        call: impl FnOnce(&fixture::Vec<i32>) -> R,
    ) -> Result<Value, String> {
        let [Value::Usize(length)] = arguments else {
            return Err("invalid fixture receiver".into());
        };
        let receiver = fixture::Vec::from(vec![0; *length]);
        Ok(call(&receiver).into_value())
    }
}

mod generated {
    pub mod vector {
        use rils_builtins_macros::decl_rils_native;
        vec_definition!(decl_rils_native);
    }
    pub mod legacy {
        use rils_builtins_macros::decl_rils_native;
        legacy_definition!(decl_rils_native);
    }
}

#[test]
fn exported_queries_invoke_the_rust_body_without_a_native_marker_or_method_list() {
    for (name, length, expected) in [
        ("occupancy", 0, Value::Usize(7)),
        ("occupancy", 3, Value::Usize(10)),
        ("has_items", 0, Value::Bool(false)),
        ("has_items", 3, Value::Bool(true)),
    ] {
        let symbol = format!("core::fixture::vec::{name}");
        assert_eq!(
            generated::vector::call_symbol(&symbol, &[Value::Usize(length)]),
            Some(Ok(expected))
        );
    }
    let symbol = "core::fixture::vec::occupancy";
    for arguments in [vec![], vec![Value::Usize(1), Value::Usize(2)]] {
        assert!(
            generated::vector::call_symbol(symbol, &arguments)
                .unwrap()
                .unwrap_err()
                .contains("expects 1 arguments")
        );
    }
}

#[test]
fn explicit_compatibility_bindings_do_not_generate_native_calls() {
    assert_eq!(fixture::Legacy::new().old(), 1);
    for symbol in ["core::fixture::legacy::old", "core::fixture::legacy::new"] {
        assert_eq!(generated::legacy::call_symbol(symbol, &[]), None);
    }
}
