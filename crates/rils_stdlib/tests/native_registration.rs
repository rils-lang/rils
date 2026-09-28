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

#[test]
fn collection_layouts_are_registered_from_stdlib_definitions() {
    let registry = native::registry();
    for (name, arguments) in [
        ("Vec", vec![Type::I32]),
        ("VecDeque", vec![Type::I32]),
        ("BinaryHeap", vec![Type::I32]),
        ("HashSet", vec![Type::I32]),
        ("BTreeSet", vec![Type::I32]),
        ("HashMap", vec![Type::I32, Type::String]),
        ("BTreeMap", vec![Type::I32, Type::String]),
    ] {
        let ty = Type::Named {
            name: name.into(),
            arguments,
        };
        let layout = registry
            .layout(&ty, &mut |child| match child {
                child if child == &Type::I32 => Ok(DynamicLayout::copy_of::<i32>(Type::I32)),
                child if child == &Type::String => Ok(DynamicLayout::of::<
                    rils_stdlib::stdlib::string::String,
                >(Type::String)),
                _ => Err(format!("unexpected collection child {child}")),
            })
            .unwrap_or_else(|| panic!("{name} has no stdlib layout"))
            .unwrap();
        assert_eq!(layout.rils_type(), &ty);
        let item = layout.sequence_item().unwrap();
        if name.ends_with("Map") {
            assert_eq!(item.record_fields().unwrap().len(), 2);
        } else {
            assert_eq!(item.rils_type(), &Type::I32);
        }
    }
}
