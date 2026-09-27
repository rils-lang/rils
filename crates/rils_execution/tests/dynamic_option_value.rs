use rils_execution::{Type, Value, value::dynamic_option};

#[test]
fn registered_option_layouts_store_concrete_items() {
    for (item_type, item) in [
        (
            Type::Integer(rils_execution::IntegerType::I8),
            rils_execution::numeric::native_i8(3),
        ),
        (Type::I32, rils_execution::numeric::native_i32(7)),
        (Type::USIZE, rils_execution::numeric::native_usize(9)),
        (Type::String, Value::from_string("hello")),
    ] {
        let expected = item.clone_owned().unwrap();
        let dynamic_option::Construction::Native(value) =
            dynamic_option::construct(Some(item), &item_type).unwrap()
        else {
            panic!("registered item layout");
        };
        let Value::Dynamic(object) = &value else {
            panic!("Option must use dynamic native storage");
        };
        assert_eq!(object.is_inline(), item_type != Type::String);
        assert_eq!(value.is_copy(), item_type != Type::String);
        assert_eq!(
            dynamic_option::view(&value).unwrap().unwrap().0,
            Some(expected)
        );
        assert_eq!(value.clone_owned().unwrap(), value);
    }
}

#[test]
fn absent_option_has_a_concrete_runtime_type() {
    let dynamic_option::Construction::Native(value) =
        dynamic_option::construct(None, &Type::String).unwrap()
    else {
        panic!("registered item layout");
    };
    assert_eq!(
        Type::of_value(&value),
        Some(Type::Option(Box::new(Type::String)))
    );
    assert_eq!(dynamic_option::view(&value).unwrap().unwrap().0, None);
}

#[test]
fn unsupported_item_returns_its_ownership_to_the_caller() {
    let item = Value::Bool(true);
    let dynamic_option::Construction::Unsupported(Some(returned)) =
        dynamic_option::construct(Some(item), &Type::Bool).unwrap()
    else {
        panic!("bool has no native Option layout yet");
    };
    assert_eq!(returned, Value::Bool(true));
}
