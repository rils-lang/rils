use rils::{BytecodeModule, Value, compile, eval};

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

#[test]
fn derived_copy_supports_enum_variants_without_moving_the_original() {
    let source = r#"
        #[derive(Clone, Copy)]
        enum Signal { Stop, Point(i32, i32), Named { value: i32 } }
        let stop = Signal::Stop;
        let point = Signal::Point(20, 21);
        let named = Signal::Named { value: 1 };
        let stop_again = stop;
        let point_again = point;
        let named_again = named;
        let a = match stop { Signal::Stop => 1, _ => 0 };
        let b = match point { Signal::Point(x, y) => x + y, _ => 0 };
        let c = match named { Signal::Named { value } => value, _ => 0 };
        let d = match stop_again { Signal::Stop => 1, _ => 0 };
        let e = match point_again { Signal::Point(x, y) => x + y, _ => 0 };
        let f = match named_again { Signal::Named { value } => value, _ => 0 };
        a + b + c + d + e + f
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(86));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(86));
}

#[test]
fn derived_copy_rejects_enum_with_non_copy_payload() {
    let source = "#[derive(Clone, Copy)] enum Message { Text(string) }";
    assert!(eval(source).unwrap_err().to_string().contains("Copy"));
}

#[test]
fn native_default_derive_handles_fields_and_unit_structs() {
    let source = r#"
        #[derive(Default)]
        struct Wrapper { value: i32 }
        #[derive(Default)]
        struct Marker;
        let wrapped = <Wrapper as Default>::default();
        let marker = <Marker as Default>::default();
        if type_of(marker) == "Marker" { wrapped.value + 42 } else { 0 }
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn native_default_derive_rejects_fields_without_a_default() {
    let source = "#[derive(Default)] struct Bad { callback: fn() -> () }";
    let error = eval(source).unwrap_err().to_string();
    assert!(error.contains("field `callback`"), "{error}");
}

#[test]
fn native_default_derive_rejects_user_fields_without_an_impl() {
    let source = "struct Inner { value: i32 } #[derive(Default)] struct Outer { value: Inner } let value = <Outer as Default>::default();";
    assert!(eval(source).is_err());
    assert!(compile(source).is_err());
}

#[test]
fn native_default_derive_rejects_explicit_impl() {
    let source = "#[derive(Default)] struct Value; impl Default for Value { fn default() -> Self { Value } }";
    let error = eval(source).unwrap_err().to_string();
    assert!(error.contains("both derive Default"), "{error}");
    assert!(compile(source).is_err());
}

#[test]
fn derived_eq_and_hash_support_struct_and_enum_collection_keys() {
    let source = r#"
        #[derive(Eq, Hash)]
        struct Key { code: i32, label: string }
        #[derive(Eq, Hash)]
        enum Signal { Stop, Number(i32), Named { label: string } }
        let mut map: HashMap<Key, i32> = HashMap::new();
        map.insert(Key { code: 7, label: "seven" }, 40);
        let matching = Key { code: 7, label: "seven" };
        let mut set: HashSet<Signal> = HashSet::new();
        set.insert(Signal::Stop);
        set.insert(Signal::Number(2));
        set.insert(Signal::Named { label: "ok" });
        let stop = Signal::Stop;
        let number = Signal::Number(2);
        let named = Signal::Named { label: "ok" };
        if set.contains(&stop) && set.contains(&number) && set.contains(&named) && set.len() == 3usize {
            map.get_cloned(&matching).unwrap() + 2
        } else { 0 }
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    let module = compile(source).unwrap();
    assert_eq!(module.execute().unwrap(), Value::I32(42));
    let restored = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(restored.execute().unwrap(), Value::I32(42));
}

#[test]
fn hash_and_eq_derive_reject_floats_and_script_bitflags() {
    for source in [
        "#[derive(Eq)] struct Bad { value: f32 }",
        "#[derive(Hash)] enum Bad { Value(f64) }",
        "#[derive(BitFlags)] enum Flags { Read, Write }",
        "enum Flags { Read, Write } impl BitFlags for Flags {}",
    ] {
        assert!(eval(source).is_err(), "{source}");
        assert!(compile(source).is_err(), "{source}");
    }
}

#[test]
fn derived_structural_key_handles_composite_fields_and_replacement() {
    let source = r#"
        #[derive(Eq, Hash)]
        struct Key { parts: (i32, string), optional: Option<i32> }
        let mut map: HashMap<Key, i32> = HashMap::new();
        let first = Key { parts: (1, "one"), optional: Some(2) };
        let same = Key { parts: (1, "one"), optional: Some(2) };
        let different = Key { parts: (1, "one"), optional: None };
        let lookup = Key { parts: (1, "one"), optional: Some(2) };
        map.insert(first, 20);
        let prior = map.insert(same, 22).unwrap();
        if map.len() == 1usize && !map.contains_key(&different) {
            prior + map.get_cloned(&lookup).unwrap()
        } else { 0 }
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}

#[test]
fn hash_collections_require_both_markers() {
    for source in [
        "#[derive(Eq)] struct Key { value: i32 } let values: HashSet<Key> = HashSet::new();",
        "#[derive(Hash)] struct Key { value: i32 } let values: HashSet<Key> = HashSet::new();",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}
