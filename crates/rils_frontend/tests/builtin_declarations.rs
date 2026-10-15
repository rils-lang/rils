use rils_frontend::{
    Type,
    standard_library::{
        builtin_associated_type, builtin_into_iterator_item_type, builtin_iterator_item_type,
    },
};

#[test]
fn collection_items_follow_exported_signatures_and_associated_types() {
    for name in ["Vec", "VecDeque", "BinaryHeap", "HashSet", "BTreeSet"] {
        let ty = Type::Named {
            name: name.into(),
            arguments: vec![Type::I32],
        };
        assert_eq!(
            builtin_associated_type(&ty, "IntoIterator", "Item"),
            Some(Type::I32),
            "{name}"
        );
        assert_eq!(
            builtin_into_iterator_item_type(&ty),
            Some(Type::I32),
            "{name}"
        );
    }
    for name in ["HashMap", "BTreeMap"] {
        let ty = Type::Named {
            name: name.into(),
            arguments: vec![Type::I32, Type::String],
        };
        let item = Type::Tuple(vec![Type::I32, Type::String]);
        assert_eq!(
            builtin_associated_type(&ty, "IntoIterator", "Item"),
            Some(item.clone()),
            "{name}"
        );
        assert_eq!(builtin_into_iterator_item_type(&ty), Some(item), "{name}");
    }
    for name in ["Range", "Iter"] {
        let ty = Type::Named {
            name: name.into(),
            arguments: vec![Type::I32],
        };
        assert_eq!(
            builtin_associated_type(&ty, "Iterator", "Item"),
            Some(Type::I32),
            "{name}"
        );
        assert_eq!(builtin_iterator_item_type(&ty), Some(Type::I32), "{name}");
    }
    assert_eq!(
        builtin_iterator_item_type(&Type::named("UnknownIterator")),
        None
    );
    assert_eq!(
        builtin_associated_type(&Type::named("Formatter"), "Iterator", "Item"),
        None
    );
}
