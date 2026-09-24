use rils_builtins::{
    BUILTIN_MODULES, BUILTIN_SOURCES, BUILTINS, BuiltinId, BuiltinKind, BuiltinMemberKind,
    BuiltinSourceKind, FLOAT_CONSTANTS, FLOAT_INTRINSICS, INTEGER_CONSTANTS, INTEGER_INTRINSICS,
    IntrinsicKind, TypePattern, builtin, builtin_member, builtin_module_members, intrinsic,
    native_member, runtime_member,
};
use rils_builtins_macros::decl_rils_source;

#[test]
fn stdlib_directory_generates_source_and_module_metadata() {
    let mut discovered = Vec::new();
    collect_rils_files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib"),
        &mut discovered,
    );
    discovered.sort();
    let mut generated = BUILTIN_SOURCES
        .iter()
        .map(|source| {
            source
                .path
                .strip_prefix("stdlib/")
                .unwrap_or(source.path)
                .to_owned()
        })
        .collect::<Vec<_>>();
    generated.sort();
    assert_eq!(generated, discovered);
    assert!(BUILTIN_SOURCES.iter().any(|source| {
        source.path == "stdlib/modules.rils" && source.kind == BuiltinSourceKind::ModuleTree
    }));
    for migrated in [
        "core/bit_flags/bit_flags.rils",
        "core/box.rils",
        "core/clone/clone.rils",
        "core/clone/copy.rils",
        "core/cmp/eq.rils",
        "core/collections/binary_heap.rils",
        "core/collections/vec_deque.rils",
        "core/default/default.rils",
        "core/float.rils",
        "core/format_error.rils",
        "core/hash/hash.rils",
        "core/integer.rils",
        "core/iter/range.rils",
        "core/option/option.rils",
        "core/result/result.rils",
        "core/rc.rils",
        "core/weak.rils",
        "core/cell.rils",
        "core/string/string.rils",
        "std/io/error.rils",
        "std/io/error_kind.rils",
    ] {
        let legacy_path = format!("stdlib/{migrated}");
        assert!(
            BUILTIN_SOURCES
                .iter()
                .all(|source| source.path != legacy_path),
            "migrated source should not remain in the legacy catalog: {migrated}"
        );
    }
    assert!(BUILTIN_MODULES.iter().any(|module| {
        module.path == "std::collections"
            && module.members
                == [
                    "BTreeMap",
                    "BTreeSet",
                    "BinaryHeap",
                    "HashMap",
                    "HashSet",
                    "Vec",
                    "VecDeque",
                ]
    }));
}

#[test]
fn runtime_import_bindings_come_from_stdlib_members() {
    let expected = [
        ("Vec", "new", "core::vec::new"),
        ("Vec", "from", "core::vec::from"),
        ("HashMap", "new", "core::hash_map::new"),
        ("HashSet", "new", "core::hash_set::new"),
    ];
    for (owner, member, import) in expected {
        let declaration = builtin_member(owner, member).expect("associated built-in declaration");
        assert_eq!(declaration.runtime_import, Some(import));
        assert_eq!(declaration.builtin_id, None);
    }
    for declaration in BUILTINS {
        for member in declaration.members {
            if member.runtime_import.is_some() {
                assert_eq!(member.kind, BuiltinMemberKind::AssociatedFunction);
                assert!(member.signature.is_some());
            }
        }
    }
}

#[test]
fn native_symbols_are_unique_and_resolve_to_their_declarations() {
    let mut symbols = std::collections::HashSet::new();
    for declaration in BUILTINS {
        for member in declaration.members {
            if let Some(symbol) = member.native_symbol {
                assert!(symbols.insert(symbol), "duplicate native symbol `{symbol}`");
                assert!(symbol.ends_with(&format!("::{}", member.name)));
                assert!(member.signature.is_some());
                assert!(member.runtime_import.is_none());
                assert_eq!(
                    rils_builtins::native_member(symbol).unwrap().name,
                    member.name
                );
            }
        }
    }
    assert!(!symbols.is_empty());
}

fn collect_rils_files(directory: &std::path::Path, files: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rils_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "rils")
        {
            files.push(
                path.strip_prefix(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib"))
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}
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
            assert!(
                !member.documentation.is_empty(),
                "{}::{} requires documentation",
                declaration.path,
                member.name
            );
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
fn direct_runtime_members_resolve_without_import_names() {
    for declaration in BUILTINS {
        for member in declaration.members {
            let Some(id) = member.builtin_id.filter(|id| id.has_direct_runtime_call()) else {
                continue;
            };
            assert!(id.canonical_path().is_some());
            assert!(runtime_member(id).is_some());
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
fn regrouped_native_paths_keep_their_numeric_ids() {
    for (id, path, raw) in [
        (BuiltinId::RangeNext, "core::iter::range::next", 0x0400),
        (
            BuiltinId::VecDequeNew,
            "core::collections::vec_deque::new",
            0x1000,
        ),
        (
            BuiltinId::BinaryHeapNew,
            "core::collections::binary_heap::new",
            0x1100,
        ),
    ] {
        assert_eq!(id.canonical_path(), Some(path));
        assert_eq!(id.as_raw(), raw);
    }
}

#[test]
fn string_methods_no_longer_reserve_builtin_ids() {
    for member in builtin("string").unwrap().members {
        assert!(member.builtin_id.is_none(), "string::{}", member.name);
        assert!(member.native_symbol.is_some(), "string::{}", member.name);
    }
    for raw in 0x0A00..=0x0A13 {
        assert!(BuiltinId::from_raw(raw).canonical_path().is_none());
    }
}

#[test]
fn direct_option_result_methods_no_longer_reserve_builtin_ids() {
    for (type_name, methods) in [
        ("Option", &["is_some", "is_none"][..]),
        ("Result", &["is_ok", "is_err", "ok", "err"][..]),
    ] {
        let declaration = builtin(type_name).unwrap();
        for &name in methods {
            let member = declaration.member(name).unwrap();
            assert!(member.builtin_id.is_none(), "{type_name}::{name}");
            assert!(member.native_symbol.is_some(), "{type_name}::{name}");
        }
    }
    for raw in [0x0800, 0x0801, 0x0805, 0x0806, 0x0900, 0x0901] {
        assert!(BuiltinId::from_raw(raw).canonical_path().is_none());
    }
}

#[test]
fn native_function_aliases_resolve_to_exported_methods() {
    let aliases = BUILTINS
        .iter()
        .filter_map(|function| function.native_symbol.map(|symbol| (function, symbol)))
        .collect::<Vec<_>>();
    assert_eq!(aliases.len(), 4);
    for (function, symbol) in aliases {
        assert_eq!(function.kind, BuiltinKind::Function);
        let method = native_member(symbol).expect("native alias targets an exported method");
        let function_arity = function.signature.unwrap().parameters.len();
        let method_arity = method.signature.unwrap().parameters.len();
        assert!(method.receiver.is_some());
        assert_eq!(function_arity, method_arity + 1, "{}", function.path);
    }
}

#[test]
fn runtime_members_have_a_native_or_legacy_binding() {
    for declaration in BUILTINS {
        if declaration.backend != rils_builtins::BuiltinBackend::Runtime {
            continue;
        }
        for member in declaration.members {
            if matches!(
                member.kind,
                BuiltinMemberKind::Method | BuiltinMemberKind::AssociatedFunction
            ) {
                assert!(
                    member.native_symbol.is_some()
                        || member.builtin_id.is_some()
                        || member.runtime_import.is_some(),
                    "{}::{} has no runtime binding",
                    declaration.path,
                    member.name
                );
            }
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
        string.member("split").expect("string::split").native_symbol,
        Some("core::string::string::split")
    );
    assert_eq!(
        string
            .member("split")
            .expect("string::split")
            .signature
            .expect("split signature")
            .result,
        TypePattern::Named {
            path: "OwnedIterator",
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

    let borrowed = builtin("Iter").expect("borrowed sequence iterator declaration");
    assert_eq!(borrowed.type_parameters, &["T"]);
    assert_eq!(
        borrowed.member("next").expect("Iter::next").builtin_id,
        Some(BuiltinId::SequenceIterNext)
    );

    for owner in ["Array", "Vec"] {
        assert_eq!(
            builtin(owner)
                .expect("sequence declaration")
                .member("iter")
                .expect("borrowed iteration method")
                .builtin_id,
            Some(BuiltinId::SequenceIter)
        );
    }
    for (owner, id) in [
        ("HashMap", BuiltinId::HashMapIter),
        ("BTreeMap", BuiltinId::BtreeMapIter),
        ("HashSet", BuiltinId::HashSetIter),
        ("BTreeSet", BuiltinId::BtreeSetIter),
    ] {
        assert_eq!(
            builtin(owner)
                .expect("map or set declaration")
                .member("iter")
                .expect("borrowed iteration method")
                .builtin_id,
            Some(id)
        );
    }

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
fn rust_numeric_definitions_cover_every_concrete_primitive() {
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
        primitive_impls(rils_stdlib::integer_definition!(decl_rils_source)),
        IntegerType::ALL
            .iter()
            .map(|kind| kind.name().to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        primitive_impls(rils_stdlib::float_definition!(decl_rils_source)),
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

#[test]
fn trait_requirements_and_provided_methods_come_from_stdlib() {
    let iterator = builtin("Iterator").expect("Iterator declaration");
    assert!(iterator.member("next").expect("Iterator::next").required);
    assert!(!iterator.member("count").expect("Iterator::count").required);

    let into_iterator = builtin("IntoIterator").expect("IntoIterator declaration");
    assert!(
        into_iterator
            .member("into_iter")
            .expect("IntoIterator::into_iter")
            .required
    );
}

#[test]
fn io_error_shapes_and_module_exports_come_from_stdlib() {
    let error = builtin("std::io::Error").expect("std::io::Error declaration");
    assert!(matches!(
        error.backend,
        rils_builtins::BuiltinBackend::Host("std::io")
    ));
    assert_eq!(
        error
            .members
            .iter()
            .filter(|member| member.kind == rils_builtins::BuiltinMemberKind::Field)
            .map(|member| member.name)
            .collect::<Vec<_>>(),
        ["kind", "message", "path"]
    );

    let error_kind = builtin("std::io::ErrorKind").expect("std::io::ErrorKind declaration");
    assert!(matches!(
        error_kind.backend,
        rils_builtins::BuiltinBackend::Host("std::io")
    ));
    assert!(error_kind.contains_member("NotFound"));
    assert!(error_kind.contains_member("Other"));
    assert!(builtin_module_members("std::io").contains(&"Error"));
    assert!(builtin_module_members("std::io").contains(&"ErrorKind"));
}
