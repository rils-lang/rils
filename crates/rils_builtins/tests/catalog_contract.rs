use rils_builtins::{
    BUILTIN_MODULES, BUILTIN_SOURCES, BUILTINS, BuiltinBackend, BuiltinKind, BuiltinMemberKind,
    BuiltinSourceKind, FLOAT_CONSTANTS, FLOAT_INTRINSICS, INTEGER_CONSTANTS, INTEGER_INTRINSICS,
    IntrinsicKind, TypePattern, builtin, builtin_member, builtin_module_members, native_member,
};
use rils_stdlib_macros::decl_rils_source;

#[test]
fn owned_native_bridge_exports_a_symbol_without_legacy_id() {
    let constructor = builtin_member("Rc", "new").expect("Rc::new is declared");
    let symbol = constructor.native_symbol.expect("native symbol");
    assert!(std::ptr::eq(native_member(symbol).unwrap(), constructor));
}

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
        "core/debug.rils",
        "core/display.rils",
        "core/formatter.rils",
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
    let extend = builtin_member("Vec", "extend").expect("Vec::extend is exported");
    assert_eq!(extend.native_symbol, Some("core::collections::vec::extend"));
    let queue_push =
        builtin_member("VecDeque", "push_back").expect("VecDeque::push_back is exported");
    assert_eq!(
        queue_push.native_symbol,
        Some("core::collections::vec_deque::push_back")
    );
    let heap_push = builtin_member("BinaryHeap", "push").expect("BinaryHeap::push is exported");
    assert_eq!(
        heap_push.native_symbol,
        Some("core::collections::binary_heap::push")
    );
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
                .all(|right| left.symbol != right.symbol)
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
fn exported_option_result_callbacks_use_native_symbols() {
    for (type_name, methods) in [
        ("Option", &["map", "and_then", "or_else", "filter"][..]),
        ("Result", &["map", "map_err", "and_then", "or_else"][..]),
    ] {
        let declaration = builtin(type_name).expect("declared standard type");
        for method in methods {
            let member = declaration
                .member(method)
                .expect("exported callback method");
            assert!(member.native_symbol.is_some(), "{type_name}::{method}");
        }
    }
}

#[test]
fn collection_constructors_export_native_symbols_without_ids() {
    for owner in ["VecDeque", "BinaryHeap", "BTreeMap", "BTreeSet"] {
        let constructor = builtin_member(owner, "new").expect("constructor");
        let symbol = constructor.native_symbol.expect("native symbol");
        assert!(std::ptr::eq(native_member(symbol).unwrap(), constructor));
    }
}

#[test]
fn queue_and_heap_members_use_native_symbols_without_ids() {
    for owner in ["VecDeque", "BinaryHeap"] {
        let declaration = builtin(owner).expect("collection declaration");
        for member in declaration.members {
            let symbol = member.native_symbol.expect("native symbol");
            assert!(std::ptr::eq(native_member(symbol).unwrap(), member));
        }
    }
}

#[test]
fn map_and_set_size_methods_use_native_symbols_without_ids() {
    for owner in ["HashMap", "BTreeMap", "HashSet", "BTreeSet"] {
        let declaration = builtin(owner).expect("collection declaration");
        for name in ["len", "is_empty"] {
            let member = declaration.member(name).expect("size method");
            assert!(member.native_bridge, "{owner}::{name}");
            let symbol = member.native_symbol.expect("native symbol");
            assert!(std::ptr::eq(native_member(symbol).unwrap(), member));
        }
    }
}

#[test]
fn map_and_set_methods_use_native_symbols_without_ids() {
    for owner in ["HashMap", "BTreeMap", "HashSet", "BTreeSet"] {
        for member in builtin(owner).expect("collection declaration").members {
            if let Some(symbol) = member.native_symbol {
                assert!(std::ptr::eq(native_member(symbol).unwrap(), member));
            }
        }
    }
}

#[test]
fn migrated_hash_constructors_keep_imports_and_native_iterators() {
    for (name, constructor) in [
        ("HashMap", "core::hash_map::new"),
        ("HashSet", "core::hash_set::new"),
    ] {
        let declaration = builtin(name).expect("migrated hash collection");
        let new = declaration.member("new").expect("constructor");
        assert_eq!(new.runtime_import, Some(constructor));
        let iter = declaration.member("iter").unwrap();
        assert!(std::ptr::eq(
            native_member(iter.native_symbol.unwrap()).unwrap(),
            iter
        ));
    }
}

#[test]
fn migrated_vec_exports_indexed_methods_without_legacy_ids() {
    let vector = builtin("Vec").expect("native Vec declaration");
    let from = vector.member("from").expect("array constructor");
    assert_eq!(from.runtime_import, Some("core::vec::from"));
    assert_eq!(
        from.signature.unwrap().parameters,
        &[TypePattern::ArrayParameter {
            element: &TypePattern::Generic("T"),
            length: "N"
        }]
    );
    assert!(vector.member("len").unwrap().indexed_view);
    assert!(vector.member("iter").unwrap().indexed_view);
    for name in [
        "len",
        "is_empty",
        "push",
        "pop",
        "clear",
        "truncate",
        "insert",
        "remove",
        "swap_remove",
        "extend",
        "into_iter",
    ] {
        let member = vector.member(name).unwrap();
        assert!(member.native_symbol.is_some(), "Vec::{name}");
    }
    assert!(vector.member("from").is_some());
}

#[test]
fn exported_trait_impl_methods_keep_public_names_and_range_next_is_native() {
    let range = builtin("Range").unwrap();
    let next = range.member("next").unwrap();
    assert!(next.native_symbol.is_some());
    assert!(rils_builtins::BLANKET_TRAIT_IMPLS.contains(&("Iterator", "IntoIterator")));

    let borrowed = builtin("Iter").unwrap();
    assert!(borrowed.member("next").is_some());
}

#[test]
fn migrated_ref_cell_exposes_lexical_reference_signatures() {
    let cell = builtin("RefCell").expect("native RefCell declaration");
    for (name, mutable) in [("borrow", false), ("borrow_mut", true)] {
        let method = cell.member(name).expect("borrow method");
        let symbol = method.native_symbol.expect("native method symbol");
        let (owner, resolved) = rils_builtins::native_member_owner(symbol).unwrap();
        assert_eq!(owner.path, cell.path);
        assert!(std::ptr::eq(resolved, method));
        assert_eq!(
            method.signature.unwrap().result,
            TypePattern::Reference {
                mutable,
                inner: &TypePattern::Generic("T")
            }
        );
    }
}

#[test]
fn filesystem_functions_come_from_native_declarations() {
    for name in [
        "read_to_string",
        "write",
        "append",
        "try_exists",
        "create_dir_all",
        "remove_file",
        "remove_dir",
        "read_dir",
    ] {
        let path = format!("std::fs::{name}");
        let function = builtin(&path).expect("native filesystem declaration");
        assert_eq!(function.kind, BuiltinKind::Function);
        assert_eq!(function.backend, BuiltinBackend::Host("std::fs"));
        assert!(builtin_module_members("std::fs").contains(&name));
    }
}

#[test]
fn string_methods_export_native_symbols() {
    for member in builtin("string").unwrap().members {
        assert!(member.native_symbol.is_some(), "string::{}", member.name);
    }
}

#[test]
fn direct_option_result_methods_export_native_symbols() {
    for (type_name, methods) in [
        ("Option", &["is_some", "is_none"][..]),
        ("Result", &["is_ok", "is_err", "ok", "err"][..]),
    ] {
        let declaration = builtin(type_name).unwrap();
        for &name in methods {
            let member = declaration.member(name).unwrap();
            assert!(member.native_symbol.is_some(), "{type_name}::{name}");
        }
    }
}

#[test]
fn native_function_aliases_resolve_to_exported_methods() {
    let aliases = BUILTINS
        .iter()
        .filter_map(|function| {
            function
                .native_symbol
                .filter(|symbol| native_member(symbol).is_some())
                .map(|symbol| (function, symbol))
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases.len(), 5);
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
fn runtime_members_have_a_symbol_or_import_binding() {
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
                        || member.runtime_import.is_some()
                        || (declaration.kind == BuiltinKind::Trait
                            && !member.required
                            && declaration.source.is_some()),
                    "{}::{} has no runtime binding",
                    declaration.path,
                    member.name
                );
            }
        }
    }
}

#[test]
fn numeric_intrinsics_use_their_declared_symbols() {
    for declaration in INTEGER_INTRINSICS {
        assert_eq!(
            declaration.symbol,
            format!("core::integer::{}", declaration.name)
        );
    }
    for declaration in FLOAT_INTRINSICS {
        assert_eq!(
            declaration.symbol,
            format!("core::float::{}", declaration.name)
        );
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

    let map = builtin("HashMap").expect("HashMap declaration");
    assert_eq!(map.type_parameters, &["K", "V"]);
    let insert = map.member("insert").expect("HashMap::insert");
    assert!(insert.native_symbol.is_some());

    let set = builtin("HashSet").expect("HashSet declaration");
    assert_eq!(set.type_parameters, &["T"]);
    let union = set.member("union").expect("HashSet::union");
    assert!(union.native_symbol.is_some());
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
    assert!(!map.required);
    let source = iterator.source.expect("Iterator exports its source");
    let program =
        rils_syntax::parser::parse_builtin_declarations(rils_syntax::lex(source).unwrap())
            .expect("Iterator source parses");
    let methods = program
        .statements
        .into_iter()
        .find_map(|statement| match statement {
            rils_syntax::ast::Stmt::Trait { methods, .. } => Some(methods),
            _ => None,
        })
        .expect("Iterator trait is present");
    assert!(
        methods
            .iter()
            .find(|method| method.name == "map")
            .unwrap()
            .body
            .is_some()
    );
    assert_eq!(
        iterator.member("next").unwrap().receiver,
        Some(rils_builtins::ReceiverMode::Mutable)
    );

    let into_iterator = builtin("IntoIterator").expect("conversion trait");
    assert_eq!(
        into_iterator.member("Item").unwrap().kind,
        BuiltinMemberKind::AssociatedType
    );
    assert_eq!(
        into_iterator.member("IntoIter").unwrap().kind,
        BuiltinMemberKind::AssociatedType
    );

    let borrowed = builtin("Iter").expect("borrowed indexed iterator declaration");
    assert_eq!(borrowed.type_parameters, &["T"]);
    assert!(borrowed.member("next").unwrap().native_symbol.is_some());
    assert_eq!(
        borrowed.member("next").unwrap().signature.unwrap().result,
        TypePattern::Option(&TypePattern::Generic("T"))
    );

    for owner in ["HashMap", "BTreeMap", "HashSet", "BTreeSet"] {
        let declaration = builtin(owner).expect("map or set declaration");
        let iter = declaration
            .member("iter")
            .expect("borrowed iteration method");
        assert!(std::ptr::eq(
            native_member(iter.native_symbol.unwrap()).unwrap(),
            iter
        ));
        let into_iter = declaration
            .member("into_iter")
            .expect("owned iteration method");
        assert!(std::ptr::eq(
            native_member(into_iter.native_symbol.unwrap()).unwrap(),
            into_iter
        ));
    }

    assert!(builtin("Array").is_none());

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
    assert!(builtin_module_members("std::io").contains(&"write_line"));
    assert_eq!(
        builtin("std::io::write")
            .expect("native std::io::write")
            .signature
            .expect("write signature")
            .parameters,
        &[TypePattern::BoundGeneric {
            name: "T",
            bounds: &[TypePattern::Named {
                path: "core::fmt::Display",
                arguments: &[]
            }]
        }]
    );
    let some = builtin("Some").expect("Some function");
    assert_eq!(some.type_parameters, &["T"]);
    assert_eq!(
        some.signature.expect("Some signature").result,
        TypePattern::Option(&TypePattern::Generic("T"))
    );

    let formatter = builtin("Formatter").expect("Formatter declaration");
    for name in ["write_str", "write_derived_debug"] {
        let member = formatter.member(name).expect("Formatter member");
        let symbol = member.native_symbol.expect("Formatter native symbol");
        assert!(std::ptr::eq(native_member(symbol).unwrap(), member));
    }
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
            inner: &TypePattern::BoundGeneric {
                name: "T",
                bounds: &[TypePattern::Named {
                    path: "core::fmt::Debug",
                    arguments: &[]
                }]
            },
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
    assert!(iterator.member("next").unwrap().native_symbol.is_some());
}

#[test]
fn derived_vec_is_empty_has_no_numeric_id() {
    let member = builtin("Vec")
        .expect("Vec declaration")
        .member("is_empty")
        .expect("Vec::is_empty declaration");
    assert!(member.native_symbol.is_some());
    assert!(member.indexed_view);
}

#[test]
fn trait_requirements_and_provided_methods_come_from_stdlib() {
    let iterator = builtin("Iterator").expect("Iterator declaration");
    assert!(iterator.member("next").expect("Iterator::next").required);
    assert_eq!(
        iterator
            .member("next")
            .expect("Iterator::next")
            .native_symbol,
        Some("Iterator::next")
    );
    assert!(
        iterator
            .members
            .iter()
            .filter(|member| member.kind == BuiltinMemberKind::Method && member.name != "next")
            .all(|member| !member.required)
    );
    let last = iterator.member("last").expect("Iterator::last");
    assert!(matches!(
        last.signature.expect("Iterator::last signature").result,
        TypePattern::Option(inner)
            if matches!(
                *inner,
                TypePattern::Associated {
                    trait_name: Some("Iterator"),
                    name: "Item",
                    ..
                }
            )
    ));
    let map = iterator.member("map").expect("Iterator::map");
    assert!(matches!(
        map.signature.expect("Iterator::map signature").parameters,
        [TypePattern::Function {
            parameters: [TypePattern::Associated {
                trait_name: Some("Iterator"),
                name: "Item",
                ..
            }],
            ..
        }]
    ));

    let into_iterator = builtin("IntoIterator").expect("IntoIterator declaration");
    assert!(
        builtin("Iterator")
            .expect("Iterator declaration")
            .member("into_iter")
            .is_none()
    );
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
#[test]
fn exported_fn_trait_keeps_its_generic_contract() {
    for (name, supertrait) in [
        ("FnOnce", None),
        ("FnMut", Some("FnOnce")),
        ("Fn", Some("FnMut")),
    ] {
        let declaration =
            rils_builtins::builtin(name).expect("callable trait is exported by rils_stdlib");
        assert_eq!(declaration.kind, rils_builtins::BuiltinKind::Trait);
        assert_eq!(declaration.type_parameters, &["Args", "Output"]);
        assert!(declaration.members.is_empty());
        assert_eq!(
            declaration.supertraits,
            supertrait.map(|name| vec![name]).unwrap_or_default()
        );
        assert!(rils_builtins::builtin_module_members("core::ops").contains(&name));
    }
}
#[test]
fn native_private_storage_is_opaque() {
    assert!(rils_builtins::builtin("Rc").unwrap().opaque_native);
    assert!(rils_builtins::builtin("Vec").unwrap().opaque_native);
    assert!(rils_builtins::builtin("Box").unwrap().opaque_native);
    assert!(!rils_builtins::builtin("FormatError").unwrap().opaque_native);
}

#[test]
fn boxed_methods_are_exported_from_the_wrapper_definition() {
    let declaration = rils_builtins::builtin("Box").unwrap();
    let method = declaration
        .members
        .iter()
        .find(|member| member.name == "new")
        .expect("Box::new is exported");
    let signature = method.signature.as_ref().unwrap();
    assert_eq!(signature.parameters.len(), 1);
    assert_eq!(signature.result, rils_builtins::TypePattern::SelfType);
}
