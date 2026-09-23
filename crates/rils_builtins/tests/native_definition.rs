use rils_builtins::{
    BuiltinId, BuiltinKind, BuiltinMemberKind, ReceiverMode, TypePattern, builtin,
    native_definitions,
};

#[test]
fn rust_option_definition_matches_the_existing_public_catalog() {
    let generated = &native_definitions::DECLARATION;
    let published = builtin("Option").expect("Option is in the public catalog");
    assert_eq!(generated.path, published.path);
    assert_eq!(generated.kind, BuiltinKind::Enum);
    assert_eq!(generated.type_parameters, published.type_parameters);
    assert_eq!(generated.documentation, "An optional value.");

    for member in generated.members {
        let original = published
            .member(member.name)
            .expect("member exists in public catalog");
        assert_eq!(member.kind, original.kind);
        assert_eq!(member.value_type, original.value_type);
        assert_eq!(member.receiver, original.receiver);
        assert_eq!(member.builtin_id, original.builtin_id);
        if let (Some(left), Some(right)) = (member.signature, original.signature) {
            assert_eq!(left.parameters, right.parameters);
            assert_eq!(left.result, right.result);
        }
    }

    let is_some = generated.member("is_some").unwrap();
    assert_eq!(is_some.kind, BuiltinMemberKind::Method);
    assert_eq!(is_some.receiver, Some(ReceiverMode::Shared));
    assert_eq!(is_some.builtin_id, Some(BuiltinId::OptionIsSome));
    assert_eq!(is_some.signature.unwrap().result, TypePattern::Bool);
    assert_eq!(
        is_some.documentation,
        "Returns true when a value is present."
    );
    assert_eq!(native_definitions::DECLARATIONS.len(), 1);
}
