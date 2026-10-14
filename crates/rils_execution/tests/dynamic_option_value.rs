use rils_execution::{
    Type, Value,
    value::{dynamic_option, storage::TypedStorageContext},
};

#[test]
fn registered_option_layouts_store_concrete_items() {
    for (item_type, item) in [
        (
            Type::Integer(rils_execution::IntegerType::I8),
            Value::from(rils_execution::numeric::native_i8(3)),
        ),
        (
            Type::I32,
            Value::from(rils_execution::numeric::native_i32(7)),
        ),
        (
            Type::USIZE,
            Value::from(rils_execution::numeric::native_usize(9)),
        ),
        (
            Type::String,
            Value::from(rils_execution::value::native_string("hello")),
        ),
    ] {
        let expected = item.clone_owned().unwrap();
        let value = TypedStorageContext::new(&[], &[])
            .construct_option(&Type::Option(Box::new(item_type.clone())), Some(item))
            .unwrap();
        let Value::Dynamic(object) = &value else {
            panic!("Option must use dynamic native storage");
        };
        assert_eq!(object.is_inline(), item_type != Type::String);
        assert_eq!(value.is_copy(), item_type != Type::String);
        assert_eq!(value.clone_owned().unwrap(), value);
        assert_eq!(dynamic_option::take_owned(value).unwrap(), Some(expected));
    }
}

#[test]
fn absent_option_has_a_concrete_runtime_type() {
    let value = TypedStorageContext::new(&[], &[])
        .construct_option(&Type::Option(Box::new(Type::String)), None)
        .unwrap();
    assert_eq!(
        Type::of_value(&value),
        Some(Type::Option(Box::new(Type::String)))
    );
    assert!(
        !value
            .with_native_view(|view| view.option_is_some())
            .unwrap()
            .unwrap()
    );
}

#[test]
fn unsupported_item_rejects_construction_without_a_layout() {
    let item = Value::from(rils_execution::value::native_bool(true));
    assert!(
        TypedStorageContext::new(&[], &[])
            .construct_option(
                &Type::Option(Box::new(Type::named("Unregistered"))),
                Some(item)
            )
            .is_err()
    );
}
