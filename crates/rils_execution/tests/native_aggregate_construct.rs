use rils_execution::{
    RilsValue, Type, Value,
    value::{native_instance::NativeInstancePlace, storage::TypedStorageContext},
};

#[test]
fn direct_aggregate_construction_moves_strings_and_validates_shape() {
    let context = TypedStorageContext::new(&[], &[]);
    for array in [false, true] {
        let ty = if array {
            Type::Array {
                element: Box::new(Type::String),
                length: 1,
            }
        } else {
            Type::Tuple(vec![Type::String])
        };
        let build = |values| {
            if array {
                context.construct_array(&ty, values)
            } else {
                context.construct_tuple(&ty, values)
            }
        };
        let text = "original owned buffer".to_owned();
        let address = text.as_ptr() as usize;
        let value = build(vec![Value::from(rils_execution::value::native_string(
            text,
        ))])
        .unwrap();
        assert!(!value.is_copy());
        assert_eq!(Type::of_value(&value), Some(ty.clone()));
        let Value::Dynamic(object) = value else {
            panic!("native aggregate");
        };
        let field = NativeInstancePlace::new(object)
            .unwrap()
            .field("0")
            .unwrap()
            .take()
            .unwrap();
        assert_eq!(
            RilsValue::new(field)
                .with_ref::<String, _>(|text| text.as_ptr() as usize)
                .unwrap(),
            address
        );
        assert!(build(vec![]).is_err());
        assert!(build(vec![Value::from(rils_execution::value::native_i32(1))]).is_err());
    }
    assert!(
        context
            .construct_array(
                &Type::Array {
                    element: Box::new(Type::Unknown),
                    length: 0
                },
                vec![]
            )
            .is_err()
    );
    let empty = context
        .construct_array(
            &Type::Array {
                element: Box::new(Type::String),
                length: 0,
            },
            vec![],
        )
        .unwrap();
    assert!(!empty.is_copy());
    assert!(!empty.clone_owned().unwrap().is_copy());
}

#[test]
fn repeat_array_validates_count_before_allocating_elements() {
    let context = TypedStorageContext::new(&[], &[]);
    let ty = Type::Array {
        element: Box::new(Type::I32),
        length: 2,
    };
    assert!(
        context
            .construct_repeat_array(
                &ty,
                Value::from(rils_execution::value::native_i32(1)),
                usize::MAX
            )
            .is_err()
    );
    let value = context
        .construct_repeat_array(&ty, Value::from(rils_execution::value::native_i32(1)), 2)
        .unwrap();
    assert!(matches!(value, Value::Dynamic(_)));
    assert_eq!(Type::of_value(&value), Some(ty));
    let ty = Type::Array {
        element: Box::new(Type::String),
        length: 0,
    };
    assert!(
        context
            .construct_repeat_array(
                &ty,
                Value::from(rils_execution::value::native_string("owned")),
                0
            )
            .is_err()
    );
}
