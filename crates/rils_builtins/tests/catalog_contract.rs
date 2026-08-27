use rils_builtins::{
    BUILTINS, BuiltinId, BuiltinKind, BuiltinMemberKind, FLOAT_CONSTANTS, FLOAT_INTRINSICS,
    INTEGER_CONSTANTS, INTEGER_INTRINSICS, IntrinsicKind, TypePattern, builtin, builtin_member,
    intrinsic, runtime_member,
};
use rils_syntax::{FloatType, IntegerType, Type, ast::Stmt, lex, parse};

#[test]
fn builtin_id_macro_resolves_the_configured_stable_id() {
    const VEC_PUSH: BuiltinId = rils_builtins::builtin_id!("core::vec::push");

    assert_eq!(VEC_PUSH, BuiltinId::VecPush);
    assert_eq!(VEC_PUSH.as_raw(), 0x0200);
    assert_eq!(VEC_PUSH.canonical_path(), Some("core::vec::push"));
    assert_eq!(VEC_PUSH.member_name(), Some("push"));
}

#[test]
fn type_pattern_macro_resolves_nested_types_without_manual_construction() {
    const PATTERN: TypePattern = rils_builtins::type_pattern!(Result<Vec<string>, std::io::Error>);

    assert_eq!(
        PATTERN,
        TypePattern::Result {
            ok: &TypePattern::Named {
                path: "Vec",
                arguments: &[TypePattern::String],
            },
            error: &TypePattern::Named {
                path: "std::io::Error",
                arguments: &[],
            },
        }
    );
}

#[test]
fn declarations_have_unique_stable_identity_and_complete_metadata() {
    let intrinsics = INTEGER_INTRINSICS
        .iter()
        .chain(FLOAT_INTRINSICS)
        .collect::<Vec<_>>();
    for (index, left) in intrinsics.iter().enumerate() {
        assert!(
            intrinsics[index + 1..]
                .iter()
                .all(|right| left.id != right.id)
        );
    }
    for declarations in [INTEGER_INTRINSICS, FLOAT_INTRINSICS] {
        for (index, left) in declarations.iter().enumerate() {
            assert!(
                declarations[index + 1..]
                    .iter()
                    .all(|right| left.kind != right.kind || left.name != right.name)
            );
        }
    }
    for (index, declaration) in BUILTINS.iter().enumerate() {
        assert!(
            BUILTINS[index + 1..]
                .iter()
                .all(|other| declaration.path != other.path)
        );
        assert!(
            !declaration.documentation.is_empty(),
            "{} requires documentation",
            declaration.path
        );
        for (member_index, member) in declaration.members.iter().enumerate() {
            assert!(!member.documentation.is_empty());
            assert!(
                declaration.members[member_index + 1..]
                    .iter()
                    .all(|other| member.name != other.name)
            );
            if member.kind == BuiltinMemberKind::Method {
                assert!(member.signature.is_some());
                assert!(member.receiver.is_some());
            }
        }
    }
}

#[test]
fn overloaded_runtime_methods_use_owner_qualified_imports() {
    for declaration in BUILTINS {
        for member in declaration.members {
            let Some(import) = member.builtin_id.and_then(BuiltinId::bytecode_import) else {
                continue;
            };
            for other in BUILTINS
                .iter()
                .flat_map(|declaration| declaration.members)
                .filter(|other| other.name == member.name)
            {
                if let Some(other_import) = other.builtin_id.and_then(BuiltinId::bytecode_import)
                    && import != other_import
                {
                    assert!(import.starts_with("core::") && other_import.starts_with("core::"));
                }
            }
        }
    }
}

#[test]
fn builtin_catalog_is_bidirectional_at_its_boundaries() {
    for declaration in BUILTINS {
        for member in declaration.members {
            if let Some(id) = member.builtin_id {
                let (_, found) = runtime_member(id).expect("runtime member declaration");
                assert_eq!(found.builtin_id, Some(id));
            }
        }
    }
    for &id in BuiltinId::ALL {
        if let Some((_, member)) = runtime_member(id) {
            assert_eq!(id.member_name(), Some(member.name));
        } else {
            let intrinsic = intrinsic(id).unwrap_or_else(|| {
                panic!(
                    "missing declaration for configured built-in {}",
                    id.canonical_path().unwrap_or("<unknown>")
                )
            });
            assert_eq!(id.member_name(), Some(intrinsic.name));
        }
    }
}

#[test]
fn numeric_intrinsics_use_their_reserved_builtin_id_blocks() {
    assert_eq!(
        rils_builtins::builtin_id!("core::integer::try_from").as_raw(),
        0x0B00
    );
    assert_eq!(
        rils_builtins::builtin_id!("core::integer::reverse_bits").as_raw(),
        0x0B5B
    );
    assert_eq!(
        rils_builtins::builtin_id!("core::float::is_nan").as_raw(),
        0x0C00
    );
    assert_eq!(
        rils_builtins::builtin_id!("core::float::mul_add").as_raw(),
        0x0C13
    );

    for declaration in INTEGER_INTRINSICS {
        assert_eq!(declaration.id.as_raw() & 0xFF00, 0x0B00);
    }
    for declaration in FLOAT_INTRINSICS {
        assert_eq!(declaration.id.as_raw() & 0xFF00, 0x0C00);
    }
}

#[test]
fn default_trait_has_a_catalog_defined_associated_function() {
    let declaration = builtin("Default").expect("Default trait declaration");
    assert_eq!(declaration.kind, BuiltinKind::Trait);
    let member = builtin_member("Default", "default").expect("Default::default declaration");
    assert_eq!(member.kind, BuiltinMemberKind::AssociatedFunction);
    let signature = member.signature.expect("Default::default signature");
    assert!(signature.parameters.is_empty());
    assert_eq!(signature.result, TypePattern::SelfType);
}

#[test]
fn rils_standard_library_files_supply_type_member_and_variant_metadata() {
    let option = builtin("Option").expect("Option declaration");
    assert_eq!(option.documentation, "An optional value.");
    assert_eq!(option.type_parameters, &["T"]);
    assert_eq!(
        option.member("None").expect("None variant").documentation,
        "An absent optional value."
    );
    assert_eq!(
        option.member("map").expect("Option::map").documentation,
        "Maps a present value with the supplied function."
    );

    let result = builtin("Result").expect("Result declaration");
    assert_eq!(result.type_parameters, &["T", "E"]);
    assert_eq!(
        result.member("Err").expect("Err variant").value_type,
        Some(TypePattern::Generic("E"))
    );

    let string = builtin("string").expect("string declaration");
    assert_eq!(string.kind, BuiltinKind::Primitive);
    assert_eq!(string.documentation, "An owned UTF-8 string.");
    assert_eq!(
        string.member("split").expect("string::split").builtin_id,
        Some(BuiltinId::StringSplit)
    );
    assert_eq!(
        string
            .member("split")
            .expect("string::split")
            .signature
            .expect("split signature")
            .result,
        TypePattern::Named {
            path: "SequenceIterator",
            arguments: &[TypePattern::String],
        }
    );

    let vec = builtin("Vec").expect("Vec declaration");
    assert_eq!(vec.kind, BuiltinKind::Struct);
    assert_eq!(vec.type_parameters, &["T"]);
    assert_eq!(
        vec.member("new").expect("Vec::new").kind,
        BuiltinMemberKind::AssociatedFunction
    );
    assert_eq!(vec.member("new").expect("Vec::new").builtin_id, None);
    assert_eq!(
        vec.member("len").expect("Vec::len").builtin_id,
        Some(BuiltinId::SequenceLen)
    );

    let map = builtin("HashMap").expect("HashMap declaration");
    assert_eq!(map.type_parameters, &["K", "V"]);
    assert_eq!(
        map.member("insert").expect("HashMap::insert").builtin_id,
        Some(BuiltinId::HashMapInsert)
    );

    let set = builtin("HashSet").expect("HashSet declaration");
    assert_eq!(set.type_parameters, &["T"]);
    assert_eq!(
        set.member("union").expect("HashSet::union").builtin_id,
        Some(BuiltinId::HashSetUnion)
    );
}

#[test]
fn rils_standard_library_files_supply_traits_modules_and_free_functions() {
    let iterator = builtin("Iterator").expect("Iterator declaration");
    assert_eq!(iterator.kind, BuiltinKind::Trait);
    assert_eq!(iterator.documentation, "A stateful sequence producer.");
    assert_eq!(
        iterator.member("Item").expect("Iterator::Item").kind,
        BuiltinMemberKind::AssociatedType
    );
    let map = iterator.member("map").expect("Iterator::map");
    assert_eq!(map.type_parameters, &["U"]);
    assert_eq!(map.builtin_id, Some(BuiltinId::IteratorMap));

    let array = builtin("Array").expect("Array declaration");
    assert_eq!(array.kind, BuiltinKind::Primitive);
    assert_eq!(
        array.member("len").expect("Array::len").builtin_id,
        Some(BuiltinId::SequenceLen)
    );

    assert_eq!(
        builtin("core").expect("core module").documentation,
        "Host-independent core APIs."
    );
    let println = builtin("std::io::println").expect("std::io::println");
    assert!(println.signature.expect("println signature").variadic);
    assert_eq!(
        println.backend,
        rils_builtins::BuiltinBackend::Host("std::io")
    );
    let some = builtin("Some").expect("Some function");
    assert_eq!(some.type_parameters, &["T"]);
    assert_eq!(
        some.signature.expect("Some signature").result,
        TypePattern::Option(&TypePattern::Generic("T"))
    );

    let formatter = builtin("Formatter").expect("Formatter declaration");
    let write_derived_debug = formatter
        .member("write_derived_debug")
        .expect("Formatter::write_derived_debug");
    assert_eq!(
        write_derived_debug.receiver,
        Some(rils_builtins::ReceiverMode::Mutable)
    );
    assert_eq!(
        write_derived_debug
            .signature
            .expect("write_derived_debug signature")
            .parameters,
        &[TypePattern::Reference {
            mutable: false,
            inner: &TypePattern::Unknown,
        }]
    );
}

#[test]
fn rils_numeric_files_supply_intrinsics_constants_and_docs() {
    let try_from = INTEGER_INTRINSICS
        .iter()
        .find(|declaration| declaration.name == "try_from")
        .expect("integer try_from declaration");
    assert_eq!(try_from.kind, IntrinsicKind::AssociatedFunction);
    assert_eq!(try_from.signature.parameters, &[TypePattern::AnyInteger]);
    assert_eq!(
        try_from.documentation,
        "Converts an integer when its value is representable by the target type."
    );

    let overflowing_add = INTEGER_INTRINSICS
        .iter()
        .find(|declaration| declaration.name == "overflowing_add")
        .expect("integer overflowing_add declaration");
    assert_eq!(
        overflowing_add.signature.result,
        TypePattern::Tuple(&[TypePattern::SelfType, TypePattern::Bool])
    );
    assert_eq!(
        INTEGER_CONSTANTS
            .iter()
            .find(|constant| constant.name == "BITS")
            .expect("integer BITS")
            .value_type,
        TypePattern::U32
    );
    assert_eq!(
        FLOAT_INTRINSICS
            .iter()
            .find(|declaration| declaration.name == "mul_add")
            .expect("float mul_add")
            .signature
            .parameters,
        &[TypePattern::SelfType, TypePattern::SelfType]
    );
    assert_eq!(
        FLOAT_CONSTANTS
            .iter()
            .find(|constant| constant.name == "NEG_INFINITY")
            .expect("float NEG_INFINITY")
            .documentation,
        "Negative infinity."
    );
}

#[test]
fn rils_numeric_files_cover_every_concrete_primitive() {
    fn primitive_impls(source: &str) -> Vec<String> {
        parse(lex(source).expect("numeric source lexes"))
            .expect("numeric source parses")
            .statements
            .into_iter()
            .filter_map(|statement| match statement {
                Stmt::Impl {
                    target: Type::Integer(kind),
                    ..
                } => Some(kind.name().to_owned()),
                Stmt::Impl {
                    target: Type::Float(kind),
                    ..
                } => Some(kind.name().to_owned()),
                _ => None,
            })
            .collect()
    }

    assert_eq!(
        primitive_impls(include_str!("../stdlib/core/integer.rils")),
        IntegerType::ALL
            .iter()
            .map(|kind| kind.name().to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        primitive_impls(include_str!("../stdlib/core/float.rils")),
        [FloatType::F32, FloatType::F64]
            .iter()
            .map(|kind| kind.name().to_owned())
            .collect::<Vec<_>>()
    );
}

#[test]
fn declarations_report_member_and_runtime_coverage() {
    let iterator = builtin("Iterator").expect("Iterator declaration");

    assert!(iterator.contains_member("next"));
    assert!(!iterator.contains_member("missing"));
    assert!(iterator.contains_builtin(BuiltinId::IteratorNext));
    assert!(!iterator.contains_builtin(BuiltinId::VecPush));
}
