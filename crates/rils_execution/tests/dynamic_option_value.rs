use rils_execution::{Type, Value, value::dynamic_option};

#[test]
fn registered_option_layouts_store_concrete_items() {
    for (item_type, item) in [
        (Type::I32, rils_execution::numeric::native_i32(7)),
        (Type::USIZE, rils_execution::numeric::native_usize(9)),
        (Type::String, Value::from_string("hello")),
    ] {
        let value = dynamic_option::construct(Some(&item), &item_type)
            .expect("registered item layout")
            .unwrap();
        let Value::Dynamic(object) = &value else {
            panic!("Option must use dynamic native storage");
        };
        assert_eq!(object.is_inline(), item_type != Type::String);
        assert_eq!(value.is_copy(), item_type != Type::String);
        assert_eq!(dynamic_option::view(&value).unwrap().unwrap().0, Some(item));
        assert_eq!(value.clone_owned().unwrap(), value);
    }
}

#[test]
fn absent_option_has_a_concrete_runtime_type() {
    let value = dynamic_option::construct(None, &Type::String)
        .expect("registered item layout")
        .unwrap();
    assert_eq!(
        Type::of_value(&value),
        Some(Type::Option(Box::new(Type::String)))
    );
    assert_eq!(dynamic_option::view(&value).unwrap().unwrap().0, None);
}
