use rils::{Value, compile, eval};

#[test]
fn rust_registered_clone_derive_runs_in_both_backends() {
    let source = r#"
        #[derive(Clone)]
        struct Label { text: string, value: i32 }
        let original = Label { text: "hi", value: 41 };
        let duplicated = original.clone();
        duplicated.value + 1
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn derived_clone_calls_a_fields_custom_clone() {
    let source = r#"
        struct Inner { value: i32 }
        impl Clone for Inner {
            fn clone(&self) -> Self { Inner { value: self.value + 1 } }
        }
        #[derive(Clone)]
        struct Outer { inner: Inner }
        let original = Outer { inner: Inner { value: 41 } };
        let duplicated = original.clone();
        duplicated.inner.value
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn derived_clone_supports_generic_structs() {
    let source = r#"
        #[derive(Clone)]
        struct Holder<T> { value: T }
        let original = Holder { value: 42 };
        let duplicated = original.clone();
        duplicated.value
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn rust_registered_copy_derive_preserves_owned_value() {
    let source = r#"
        #[derive(Clone, Copy)]
        struct Point { x: i32 }
        let point = Point { x: 21 };
        let other = point;
        point.x + other.x
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn native_derive_rejects_an_explicit_impl_of_the_same_trait() {
    let source = r#"
        #[derive(Clone)]
        struct Label { value: i32 }
        impl Clone for Label {
            fn clone(&self) -> Self { Label { value: self.value } }
        }
    "#;
    let error = eval(source).unwrap_err().to_string();
    assert!(error.contains("cannot both derive Clone"), "{error}");
}

#[test]
fn native_clone_derive_supports_every_enum_variant_shape() {
    let source = r#"
        #[derive(Clone)]
        enum Message {
            Quit,
            Move(i32, i32),
            Write { text: string },
        }
        let quit = Message::Quit;
        let moved = Message::Move(20, 21);
        let written = Message::Write { text: "hi" };
        let a = quit.clone();
        let b = moved.clone();
        let c = written.clone();
        let first = match a { Message::Quit => 1, _ => 0 };
        let second = match b { Message::Move(x, y) => x + y, _ => 0 };
        let third = match c { Message::Write { text } => if text == "hi" { 0 } else { 1 }, _ => 1 };
        first + second + third
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn derived_clone_of_enum_calls_custom_field_clone() {
    let source = r#"
        struct Inner { value: i32 }
        impl Clone for Inner {
            fn clone(&self) -> Self { Inner { value: self.value + 1 } }
        }
        #[derive(Clone)]
        enum Envelope { Empty, One(Inner), Named { value: Inner } }
        let empty = Envelope::Empty.clone();
        let one = Envelope::One(Inner { value: 20 }).clone();
        let named = Envelope::Named { value: Inner { value: 20 } }.clone();
        let a = match empty { Envelope::Empty => 0, _ => 1 };
        let b = match one { Envelope::One(inner) => inner.value, _ => 0 };
        let c = match named { Envelope::Named { value } => value.value, _ => 0 };
        a + b * 100 + c
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(2121));
    assert_eq!(
        compile(source).unwrap().execute().unwrap(),
        Value::I32(2121)
    );
}

#[test]
fn derived_clone_supports_generic_enums() {
    let source = r#"
        #[derive(Clone)]
        enum Envelope<T> { Empty, One(T), Named { value: T } }
        let empty = Envelope::<i32>::Empty.clone();
        let one = Envelope::One(20).clone();
        let named = Envelope::Named { value: 21 }.clone();
        let a = match empty { Envelope::Empty => 1, _ => 0 };
        let b = match one { Envelope::One(value) => value, _ => 0 };
        let c = match named { Envelope::Named { value } => value, _ => 0 };
        a + b + c
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn derived_copy_rejects_non_copy_fields() {
    let source = "#[derive(Copy)] struct Text { field: string }";
    let error = eval(source).unwrap_err().to_string();
    assert!(error.contains("Copy"), "{error}");
}
