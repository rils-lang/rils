use rils_frontend::{Type, standard_library::builtin_member_type};

#[test]
fn native_map_iterators_keep_pair_item_types() {
    for name in ["HashMapIntoIter", "BTreeMapIntoIter"] {
        assert!(
            rils_builtins::builtin_module_members("core::collections").contains(&name),
            "{name} must be visible in core::collections"
        );
        let iterator = Type::Named {
            name: name.into(),
            arguments: vec![Type::I32, Type::String],
        };
        let next = builtin_member_type(&iterator, "next");
        assert_eq!(
            next,
            Some(Type::function(
                Vec::new(),
                Type::Option(Box::new(Type::Tuple(vec![Type::I32, Type::String])))
            )),
            "{name}"
        );
    }
}
