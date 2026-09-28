use rils_stdlib::{Type, native};
use rils_value::{DynamicLayout, DynamicValue};

#[test]
fn native_registry_collects_vec_and_string_definitions() {
    let registry = native::registry();
    let ty = Type::Named {
        name: "Vec".into(),
        arguments: vec![Type::String],
    };
    let layout = registry
        .layout(&ty, &mut |child| {
            assert_eq!(child, &Type::String);
            Ok(DynamicLayout::of::<rils_stdlib::stdlib::string::String>(
                Type::String,
            ))
        })
        .unwrap()
        .unwrap();
    assert_eq!(layout.rils_type(), &ty);
    assert!(registry.can_read_element(layout.sequence_item().unwrap()));

    let item = DynamicValue::from_rust(
        layout.sequence_item().unwrap().clone(),
        rils_stdlib::stdlib::string::String::from("native".to_owned()),
    )
    .unwrap();
    let cloned = registry.clone_borrowed_element(&item).unwrap();
    assert_eq!(
        cloned
            .with::<rils_stdlib::stdlib::string::String, _>(|text| text.len())
            .unwrap(),
        6
    );
}
