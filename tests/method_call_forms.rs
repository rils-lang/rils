use rils::{Value, compile, eval_value};

#[test]
fn inherent_and_trait_methods_accept_all_call_forms() {
    let source = r#"
        trait Read { fn read(&self) -> i32; }
        struct Number { value: i32 }
        impl Number {
            fn doubled(&self) -> i32 { self.value * 2 }
        }
        impl Read for Number {
            fn read(&self) -> i32 { self.value }
        }
        let number = Number { value: 7 };
        number.doubled() + Number::doubled(&number)
            + number.read() + Number::read(&number) + Read::read(&number)
    "#;
    let expected = Value::from_i32(49);
    assert_eq!(eval_value(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
}

#[test]
fn type_paths_preserve_owned_and_mutable_receivers() {
    for (source, expected) in [
        (
            r#"
                struct Counter { value: i32 }
                impl Counter {
                    fn add(&mut self, amount: i32) { self.value = self.value + amount; }
                    fn finish(self) -> i32 { self.value }
                }
                let mut counter = Counter { value: 5 };
                Counter::add(&mut counter, 2);
                Counter::finish(counter)
            "#,
            7,
        ),
        (
            r#"
                trait Read { fn read(&self) -> i32; }
                enum Number { One(i32) }
                impl Read for Number {
                    fn read(&self) -> i32 {
                        match self { Number::One(value) => *value }
                    }
                }
                let number = Number::One(7);
                Number::read(&number)
            "#,
            7,
        ),
    ] {
        let expected = Value::from_i32(expected);
        assert_eq!(eval_value(source).unwrap(), expected);
        assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
    }
}

#[test]
fn type_paths_preserve_inherent_priority_and_trait_ambiguity() {
    let source = r#"
        trait Left { fn value(&self) -> i32; }
        trait Right { fn value(&self) -> i32; }
        struct Number { inner: i32 }
        impl Left for Number { fn value(&self) -> i32 { 1 } }
        impl Right for Number { fn value(&self) -> i32 { 2 } }
        impl Number { fn value(&self) -> i32 { self.inner } }
        let number = Number { inner: 7 };
        Number::value(&number)
    "#;
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(7));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(7)
    );

    let ambiguous = source.replace("impl Number { fn value(&self) -> i32 { self.inner } }", "");
    assert!(eval_value(&ambiguous).is_err());
    assert!(compile(&ambiguous).is_err());
}

#[test]
fn native_type_paths_pass_the_receiver_as_the_first_argument() {
    let source = r#"
        let mut items: Vec<string> = Vec::new();
        Vec::push(&mut items, "first");
        Vec::pop(&mut items).unwrap()
    "#;
    let expected = Value::from_string("first");
    assert_eq!(eval_value(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
}

#[test]
fn type_paths_can_call_trait_default_methods() {
    let source = r#"
        trait Read {
            fn value(&self) -> i32;
            fn doubled(&self) -> i32 { self.value() * 2 }
        }
        struct Number { inner: i32 }
        impl Read for Number {
            fn value(&self) -> i32 { self.inner }
        }
        let number = Number { inner: 7 };
        number.doubled() + Number::doubled(&number) + Read::doubled(&number)
    "#;
    let expected = Value::from_i32(42);
    assert_eq!(eval_value(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
}

#[test]
fn native_trait_paths_accept_the_receiver_explicitly() {
    let source = r#"
        let mut values: Vec<i32> = Vec::new();
        values.push(7);
        let mut iterator = IntoIterator::into_iter(values);
        Iterator::next(&mut iterator).unwrap()
    "#;
    let expected = Value::from_i32(7);
    assert_eq!(eval_value(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
}
