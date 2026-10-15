extern crate self as rils_stdlib;
extern crate self as rils_value;
extern crate self as storage_fixture;
pub type DynamicValue = ();
#[path = "support/storage_schema.rs"]
pub mod storage;

#[path = "fixtures/default_native.rs"]
#[macro_use]
mod fixture;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Usize(usize),
    Bool(bool),
    Items(std::rc::Rc<std::cell::RefCell<Vec<i32>>>),
}

impl Value {
    fn get_cloned<T: TryFrom<usize>>(&self) -> Result<T, String> {
        match self {
            Self::Usize(value) => T::try_from(*value).map_err(|_| "invalid argument".into()),
            _ => Err("expected usize".into()),
        }
    }
}

// The execution crate supplies the receiver conversion. The macro supplies
// the method call, so an additional query needs no method-name registration.
mod sequence_access {
    use super::{Value, fixture, storage::EntrySchema};

    pub enum ResultShape {
        Unit,
        Bool,
        Usize,
    }
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
    pub struct Query<'a>(&'a Value);
    impl<'a> Query<'a> {
        pub fn new(value: &'a Value) -> Self {
            Self(value)
        }
    }
    impl fixture::Search<Query<'_>> for fixture::Length {
        fn matches(&self, value: &Query<'_>) -> Result<bool, String> {
            fixture::Search::<usize>::matches(self, &value.0.get_cloned::<usize>()?)
        }
    }
    pub struct Call<'a>(&'a Value);
    impl<'a> Call<'a> {
        pub fn prepare(value: &'a Value, schema: &EntrySchema, _: bool) -> Result<Self, String> {
            assert_eq!(schema.owner, "Vec");
            assert_eq!(schema.parameters, 1);
            Ok(Self(value))
        }
        pub fn shared<R: Output>(
            self,
            _: ResultShape,
            call: impl FnOnce(fixture::Length) -> Result<R, String>,
        ) -> Result<Value, String> {
            let Value::Usize(length) = self.0 else {
                return Err("invalid fixture receiver".into());
            };
            call(fixture::Length(*length)).map(Output::into_value)
        }
        pub fn mutable(
            self,
            _: ResultShape,
            call: impl FnOnce(fixture::Mutable<'_>) -> Result<(), String>,
        ) -> Result<Value, String> {
            let Value::Items(items) = self.0 else {
                return Err("invalid mutable receiver".into());
            };
            let mut items = items.try_borrow_mut().map_err(|_| "borrowed fixture")?;
            call(fixture::Mutable(&mut items))?;
            Ok(Value::Usize(items.len()))
        }
    }
    pub fn with_borrowed_sequence(
        arguments: &[Value],
        schema: &EntrySchema,
        method: &str,
        call: impl FnOnce(fixture::Length) -> Result<usize, String>,
    ) -> Result<Value, String> {
        assert_eq!(schema.owner, "Vec");
        assert_eq!(method, "scan");
        let [Value::Usize(length)] = arguments else {
            return Err("invalid iterator receiver".into());
        };
        call(fixture::Length(*length)).map(Value::Usize)
    }
}

mod generated {
    pub mod vector {
        use rils_stdlib_macros::decl_rils_native;
        vec_definition!(decl_rils_native);
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
fn custom_mutation_methods_execute_their_bodies_and_preserve_failures() {
    use std::{cell::RefCell, rc::Rc};
    let items = Rc::new(RefCell::new(vec![1, 2, 3, 4]));
    let receiver = Value::Items(items.clone());
    let symbol = "core::fixture::vec::keep_prefix";
    assert_eq!(
        generated::vector::call_symbol(symbol, &[receiver.clone(), Value::Usize(14)]),
        Some(Err("fixture backend refusal".into()))
    );
    assert_eq!(*items.borrow(), [1, 2, 3, 4]);
    assert_eq!(
        generated::vector::call_symbol(symbol, &[receiver.clone(), Value::Usize(3)]),
        Some(Ok(Value::Usize(2)))
    );
    assert_eq!(*items.borrow(), [1, 2]);
    assert!(
        generated::vector::call_symbol(symbol, &[receiver.clone(), Value::Bool(true)])
            .unwrap()
            .is_err()
    );
    assert!(
        generated::vector::call_symbol(symbol, std::slice::from_ref(&receiver))
            .unwrap()
            .is_err()
    );
    assert_eq!(*items.borrow(), [1, 2]);
    assert_eq!(
        generated::vector::call_symbol(
            "core::fixture::vec::keep_difference",
            &[receiver.clone(), Value::Usize(4), Value::Usize(2)]
        ),
        Some(Ok(Value::Usize(1)))
    );
    assert_eq!(*items.borrow(), [1]);
    assert_eq!(
        generated::vector::call_symbol("core::fixture::vec::reset", &[receiver]),
        Some(Ok(Value::Usize(0)))
    );
    assert!(items.borrow().is_empty());
}

#[test]
fn custom_shared_element_queries_execute_the_body_and_propagate_errors() {
    let symbol = "core::fixture::vec::lacks";
    for (needle, expected) in [
        (3, Ok(Value::Bool(false))),
        (4, Ok(Value::Bool(true))),
        (13, Err("query refused".into())),
    ] {
        assert_eq!(
            generated::vector::call_symbol(symbol, &[Value::Usize(3), Value::Usize(needle)]),
            Some(expected)
        );
    }
    assert!(
        generated::vector::call_symbol(symbol, &[Value::Usize(3)])
            .unwrap()
            .is_err()
    );
    assert!(
        generated::vector::call_symbol(symbol, &[Value::Usize(3), Value::Bool(true)])
            .unwrap()
            .is_err()
    );
}

#[test]
fn renamed_iterator_method_executes_its_body_and_preserves_errors() {
    let symbol = "core::fixture::vec::scan";
    for (length, expected) in [
        (0, Ok(Value::Usize(3))),
        (4, Ok(Value::Usize(7))),
        (13, Err("iteration refused".into())),
    ] {
        assert_eq!(
            generated::vector::call_symbol(symbol, &[Value::Usize(length)]),
            Some(expected)
        );
    }
    assert!(
        generated::vector::call_symbol(symbol, &[])
            .unwrap()
            .is_err()
    );
    assert!(
        generated::vector::call_symbol(symbol, &[Value::Bool(true)])
            .unwrap()
            .is_err()
    );
}
