use rils_builtins::{
    BuiltinKind, BuiltinMemberKind, ReceiverMode, TypePattern, builtin, native_definitions,
};
use rils_builtins_macros::decl_rils_source;

#[test]
fn rust_option_definition_matches_the_existing_public_catalog() {
    let generated = &native_definitions::DECLARATION;
    let published = builtin("Option").expect("Option is in the public catalog");
    assert_eq!(generated.path, published.path);
    assert_eq!(generated.kind, BuiltinKind::Enum);
    assert_eq!(generated.type_parameters, published.type_parameters);
    assert_eq!(generated.documentation, "An optional value.");
    assert_eq!(generated.members.len(), published.members.len());

    for member in generated.members {
        let original = published
            .member(member.name)
            .expect("member exists in public catalog");
        assert_eq!(member.kind, original.kind);
        assert_eq!(member.value_type, original.value_type);
        assert_eq!(member.receiver, original.receiver);
        assert_eq!(member.builtin_id, original.builtin_id);
        assert_eq!(member.documentation, original.documentation);
        if let (Some(left), Some(right)) = (member.signature, original.signature) {
            assert_eq!(left.parameters, right.parameters);
            assert_eq!(left.result, right.result);
        }
    }

    let is_some = generated.member("is_some").unwrap();
    assert!(generated.member("has_value").is_none());
    assert_eq!(is_some.kind, BuiltinMemberKind::Method);
    assert_eq!(is_some.receiver, Some(ReceiverMode::Shared));
    assert_eq!(is_some.builtin_id, None);
    assert_eq!(is_some.native_symbol, Some("core::option::option::is_some"));
    assert_eq!(is_some.signature.unwrap().result, TypePattern::Bool);
    assert_eq!(
        is_some.documentation,
        "Returns true when a value is present."
    );
}

#[test]
fn rust_range_definition_matches_the_public_catalog() {
    let generated = &native_definitions::range::DECLARATION;
    let published = builtin("Range").expect("Range is in the public catalog");
    assert_eq!(generated.path, published.path);
    assert_eq!(generated.kind, BuiltinKind::Struct);
    assert_eq!(generated.type_parameters, &["T"]);
    assert_eq!(generated.documentation, "A half-open integer range.");
    for name in ["next", "into_iter"] {
        let method = generated.member(name).expect("generated method");
        let public = published.member(name).expect("published method");
        assert_eq!(method.builtin_id, public.builtin_id);
        assert_eq!(method.receiver, public.receiver);
        let generated_signature = method.signature.expect("generated signature");
        let public_signature = public.signature.expect("published signature");
        assert_eq!(generated_signature.parameters, public_signature.parameters);
        assert_eq!(generated_signature.result, public_signature.result);
    }
}

#[test]
fn rust_vec_deque_definition_matches_the_public_catalog() {
    let generated = &native_definitions::vec_deque::DECLARATION;
    let published = builtin("VecDeque").expect("VecDeque is in the public catalog");
    assert_eq!(generated.kind, BuiltinKind::Struct);
    assert_eq!(generated.type_parameters, &["T"]);
    assert_eq!(generated.members.len(), published.members.len());
    for method in generated.members {
        let public = published.member(method.name).expect("published method");
        assert_eq!(method.kind, public.kind);
        assert_eq!(method.builtin_id, public.builtin_id);
        assert_eq!(method.receiver, public.receiver);
        let generated_signature = method.signature.expect("generated signature");
        let public_signature = public.signature.expect("published signature");
        assert_eq!(generated_signature.parameters, public_signature.parameters);
        assert_eq!(generated_signature.result, public_signature.result);
    }
}

#[test]
fn rust_binary_heap_definition_matches_the_public_catalog() {
    let generated = &native_definitions::binary_heap::DECLARATION;
    let published = builtin("BinaryHeap").expect("BinaryHeap is in the public catalog");
    assert_eq!(generated.kind, BuiltinKind::Struct);
    assert_eq!(generated.type_parameters, &["T"]);
    assert_eq!(generated.members.len(), published.members.len());
    for method in generated.members {
        let public = published.member(method.name).expect("published method");
        assert_eq!(method.kind, public.kind);
        assert_eq!(method.builtin_id, public.builtin_id);
        assert_eq!(method.receiver, public.receiver);
        assert_eq!(method.documentation, public.documentation);
        let generated_signature = method.signature.expect("generated signature");
        let public_signature = public.signature.expect("published signature");
        assert_eq!(generated_signature.parameters, public_signature.parameters);
        assert_eq!(generated_signature.result, public_signature.result);
    }
}

#[test]
fn rust_trait_definitions_supply_the_public_catalog() {
    let clone = builtin("Clone").expect("Clone is in the public catalog");
    let copy = builtin("Copy").expect("Copy is in the public catalog");
    assert_eq!(clone.kind, BuiltinKind::Trait);
    assert_eq!(copy.kind, BuiltinKind::Trait);
    assert_eq!(clone.documentation, "Explicit owned duplication.");
    assert_eq!(copy.documentation, "Values duplicated by ordinary reads.");
    assert_eq!(clone.members.len(), 1);
    assert!(copy.members.is_empty());
    assert!(clone.supertraits.is_empty());
    assert_eq!(copy.supertraits, &["Clone"]);
    let member = clone.member("clone").unwrap();
    assert_eq!(member.kind, BuiltinMemberKind::Method);
    assert_eq!(member.receiver, Some(ReceiverMode::Shared));
    assert_eq!(
        member.builtin_id,
        Some(rils_builtins::builtin_id!("core::clone"))
    );
    assert_eq!(member.signature.unwrap().result, TypePattern::SelfType);
    assert_eq!(
        member.documentation,
        "Explicitly duplicates an owned value."
    );
    assert_eq!(native_definitions::clone::DECLARATION.path, clone.path);
    assert_eq!(native_definitions::copy::DECLARATION.path, copy.path);
}

#[test]
fn grouped_native_traits_match_the_public_catalog() {
    for (path, generated) in [
        ("Default", &native_definitions::default::DECLARATION),
        ("Eq", &native_definitions::eq::DECLARATION),
        ("Hash", &native_definitions::hash::DECLARATION),
        ("BitFlags", &native_definitions::bit_flags::DECLARATION),
    ] {
        let published = builtin(path).expect("trait is in the public catalog");
        assert_eq!(generated.path, published.path);
        assert_eq!(generated.kind, published.kind);
        assert_eq!(generated.documentation, published.documentation);
        assert_eq!(generated.backend, published.backend);
        assert_eq!(generated.members.len(), published.members.len());
        for member in generated.members {
            let original = published
                .member(member.name)
                .expect("trait member is exported");
            assert_eq!(member.kind, original.kind);
            assert_eq!(member.receiver, original.receiver);
            assert_eq!(member.builtin_id, original.builtin_id);
            match (member.signature, original.signature) {
                (Some(left), Some(right)) => {
                    assert_eq!(left.parameters, right.parameters);
                    assert_eq!(left.result, right.result);
                    assert_eq!(left.variadic, right.variadic);
                }
                (None, None) => {}
                _ => panic!("trait signature mismatch for {path}::{}", member.name),
            }
        }
    }
}

#[test]
fn rust_result_definition_matches_the_existing_public_catalog() {
    let generated = &native_definitions::result::DECLARATION;
    let published = builtin("Result").expect("Result is in the public catalog");
    assert_eq!(generated.path, published.path);
    assert_eq!(generated.kind, BuiltinKind::Enum);
    assert_eq!(generated.type_parameters, published.type_parameters);
    assert_eq!(generated.documentation, published.documentation);
    assert_eq!(generated.members.len(), published.members.len());

    for member in generated.members {
        let original = published
            .member(member.name)
            .expect("member exists in public catalog");
        assert_eq!(member.kind, original.kind);
        assert_eq!(member.value_type, original.value_type);
        assert_eq!(member.receiver, original.receiver);
        assert_eq!(member.builtin_id, original.builtin_id);
        assert_eq!(member.documentation, original.documentation);
        if let (Some(left), Some(right)) = (member.signature, original.signature) {
            assert_eq!(left.parameters, right.parameters);
            assert_eq!(left.result, right.result);
        }
    }
    for name in ["is_ok", "is_err", "ok", "err"] {
        assert!(generated.member(name).is_some());
    }
}

#[test]
fn integer_family_matches_the_existing_integer_api() {
    let generated = &native_definitions::integer::INTRINSICS;
    assert_eq!(generated.len(), rils_builtins::INTEGER_INTRINSICS.len());
    for method in *generated {
        let published = rils_builtins::intrinsic(method.id).unwrap();
        assert_eq!(method.name, published.name);
        assert_eq!(method.kind, published.kind);
        assert_eq!(method.signature.parameters, published.signature.parameters);
        assert_eq!(method.signature.result, published.signature.result);
        assert_eq!(method.documentation, published.documentation);
    }
    let constants = native_definitions::integer::CONSTANTS;
    assert_eq!(constants.len(), rils_builtins::INTEGER_CONSTANTS.len());
    for constant in constants {
        let published = rils_builtins::integer_constant(constant.name).unwrap();
        assert_eq!(constant.id, published.id);
        assert_eq!(constant.value_type, published.value_type);
        assert_eq!(constant.documentation, published.documentation);
    }
    let source = rils_stdlib::integer_definition!(decl_rils_source);
    assert!(source.contains("impl i32"));
    assert!(!source.contains("struct Number"));
    let tokens = rils_syntax::lex(source).unwrap();
    let program = rils_syntax::parser::parse_builtin_declarations(tokens).unwrap();
    assert_eq!(program.statements.len(), 12);
    assert!(
        program
            .statements
            .iter()
            .all(|statement| matches!(statement, rils_syntax::ast::Stmt::Impl { .. }))
    );
}

#[test]
fn float_family_and_string_match_the_public_catalog() {
    assert_eq!(
        native_definitions::float::INTRINSICS.len(),
        rils_builtins::FLOAT_INTRINSICS.len()
    );
    for method in native_definitions::float::INTRINSICS {
        let published = rils_builtins::intrinsic(method.id).unwrap();
        assert_eq!(method.name, published.name);
        assert_eq!(method.kind, published.kind);
        assert_eq!(method.signature.parameters, published.signature.parameters);
        assert_eq!(method.signature.result, published.signature.result);
    }
    assert_eq!(
        native_definitions::float::CONSTANTS.len(),
        rils_builtins::FLOAT_CONSTANTS.len()
    );

    let string = builtin("string").expect("string is in the public catalog");
    let native = &native_definitions::string::DECLARATION;
    assert_eq!(string.kind, BuiltinKind::Primitive);
    assert_eq!(string.documentation, native.documentation);
    assert_eq!(string.members.len(), native.members.len());
    for member in native.members {
        let published = string.member(member.name).unwrap();
        assert_eq!(member.builtin_id, published.builtin_id);
        assert_eq!(member.receiver, published.receiver);
        assert_eq!(
            member.signature.unwrap().parameters,
            published.signature.unwrap().parameters
        );
        assert_eq!(
            member.signature.unwrap().result,
            published.signature.unwrap().result
        );
    }
}
