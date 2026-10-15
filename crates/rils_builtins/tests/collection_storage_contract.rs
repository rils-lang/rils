//! Language contracts projected from the collections' replaceable Rust storage.
use rils_builtins::{BuiltinDeclaration, ReceiverMode, TypePattern, builtin};
use rils_syntax::{ast::Stmt, lex, parser::parse_builtin_declarations};

const KEY: TypePattern = TypePattern::Generic("K");
const VALUE: TypePattern = TypePattern::Generic("V");
const ITEM: TypePattern = TypePattern::Generic("T");
const KEY_REF: TypePattern = TypePattern::Reference {
    mutable: false,
    inner: &KEY,
};
const ITEM_REF: TypePattern = TypePattern::Reference {
    mutable: false,
    inner: &ITEM,
};
const SELF_REF: TypePattern = TypePattern::Reference {
    mutable: false,
    inner: &TypePattern::SelfType,
};

fn signature(
    owner: &BuiltinDeclaration,
    name: &str,
    receiver: Option<ReceiverMode>,
    parameters: &[TypePattern],
    result: TypePattern,
) {
    let member = owner
        .member(name)
        .unwrap_or_else(|| panic!("{}::{name} is missing", owner.path));
    let signature = member.signature.expect("method signature");
    assert_eq!(member.receiver, receiver, "{}::{name}", owner.path);
    assert_eq!(signature.parameters, parameters, "{}::{name}", owner.path);
    assert_eq!(signature.result, result, "{}::{name}", owner.path);
    assert!(member.type_parameters.is_empty(), "{}::{name}", owner.path);
    assert!(member.native_symbol.is_some(), "{}::{name}", owner.path);
}

#[test]
fn collection_sources_and_iteration_metadata_hide_backend_generics() {
    for (name, parameters, borrowed, item, into_iter) in [
        (
            "HashMap",
            &["K", "V"][..],
            TypePattern::Tuple(&[
                KEY_REF,
                TypePattern::Reference {
                    mutable: false,
                    inner: &VALUE,
                },
            ]),
            "(K,V)",
            "HashMapIntoIter",
        ),
        (
            "BTreeMap",
            &["K", "V"][..],
            TypePattern::Tuple(&[
                KEY_REF,
                TypePattern::Reference {
                    mutable: false,
                    inner: &VALUE,
                },
            ]),
            "(K,V)",
            "BTreeMapIntoIter",
        ),
        ("HashSet", &["T"][..], ITEM_REF, "T", "HashSetIntoIter"),
        ("BTreeSet", &["T"][..], ITEM_REF, "T", "BTreeSetIntoIter"),
    ] {
        let declaration = builtin(name).unwrap();
        assert_eq!(declaration.type_parameters, parameters, "{name}");
        let source = declaration.source.expect("generated language declaration");
        let compact = source.replace(' ', "");
        for hidden in [
            "S::",
            "rils_storage",
            "rils_associated",
            "Sequence",
            "MapInsert",
            "SetInsert",
            "InsertOutput",
            "SearchOutput",
            "RelationOutput",
            "AlgebraOutput",
            "CloneOutput",
            "RemoveOutput",
            "KeysOutput",
            "ValuesOutput",
        ] {
            assert!(
                !compact.contains(hidden),
                "{name} leaked {hidden}: {source}"
            );
        }
        let parsed = parse_builtin_declarations(lex(source).unwrap()).unwrap();
        let mut associated_checked = false;
        for statement in parsed.statements {
            match statement {
                Stmt::Struct {
                    name: structure,
                    generic_parameters,
                    ..
                } if structure == name => {
                    assert_eq!(
                        generic_parameters
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>(),
                        parameters,
                        "{name}"
                    );
                }
                Stmt::Impl {
                    generic_parameters,
                    trait_name,
                    associated_types,
                    ..
                } => {
                    assert_eq!(
                        generic_parameters
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>(),
                        parameters,
                        "{name}"
                    );
                    if trait_name.as_deref() == Some("IntoIterator") {
                        assert_eq!(associated_types.len(), 2, "{name}");
                        for (associated, expected) in [
                            ("Item", item.to_owned()),
                            ("IntoIter", format!("{into_iter}<{}>", parameters.join(","))),
                        ] {
                            let actual = associated_types
                                .iter()
                                .find(|ty| ty.name == associated)
                                .unwrap()
                                .value
                                .as_ref()
                                .unwrap()
                                .to_string()
                                .replace(' ', "");
                            assert_eq!(actual, expected, "{name}::{associated}");
                        }
                        associated_checked = true;
                    }
                }
                _ => {}
            }
        }
        assert!(associated_checked, "{name} source omitted IntoIterator");
        let method = declaration.member("iter").unwrap();
        assert_eq!(method.receiver, Some(ReceiverMode::Shared));
        let TypePattern::Named {
            path,
            arguments: [actual],
        } = method.signature.unwrap().result
        else {
            panic!("{name} borrowed iterator shape");
        };
        assert_eq!(path, "Iter");
        assert_eq!(*actual, borrowed, "{name}");
        let owned = declaration.member("into_iter").unwrap();
        assert_eq!(owned.trait_name, Some("IntoIterator"));
        assert_eq!(owned.receiver, Some(ReceiverMode::Owned));
        let TypePattern::Named { path, arguments } = owned.signature.unwrap().result else {
            panic!("{name} owning iterator shape");
        };
        assert_eq!(path, into_iter);
        assert_eq!(
            arguments,
            parameters
                .iter()
                .map(|name| TypePattern::Generic(name))
                .collect::<Vec<_>>(),
            "{name}"
        );
        signature(declaration, "new", None, &[], TypePattern::SelfType);
        for (method, receiver, result) in [
            ("len", ReceiverMode::Shared, TypePattern::Usize),
            ("is_empty", ReceiverMode::Shared, TypePattern::Bool),
            ("clear", ReceiverMode::Mutable, TypePattern::Unit),
        ] {
            signature(declaration, method, Some(receiver), &[], result);
        }
    }
}

#[test]
fn map_signatures_preserve_distinct_key_and_value_parameters() {
    for name in ["HashMap", "BTreeMap"] {
        let declaration = builtin(name).unwrap();
        for (method, receiver, parameters, result) in [
            (
                "insert",
                ReceiverMode::Mutable,
                &[KEY, VALUE][..],
                TypePattern::Option(&VALUE),
            ),
            (
                "remove",
                ReceiverMode::Mutable,
                &[KEY_REF][..],
                TypePattern::Option(&VALUE),
            ),
            (
                "contains_key",
                ReceiverMode::Shared,
                &[KEY_REF][..],
                TypePattern::Bool,
            ),
            (
                "get_cloned",
                ReceiverMode::Shared,
                &[KEY_REF][..],
                TypePattern::Option(&VALUE),
            ),
        ] {
            signature(declaration, method, Some(receiver), parameters, result);
        }
    }
    for (method, item) in [("keys_cloned", KEY), ("values_cloned", VALUE)] {
        let method = builtin("HashMap").unwrap().member(method).unwrap();
        assert_eq!(method.receiver, Some(ReceiverMode::Shared));
        assert!(method.signature.unwrap().parameters.is_empty());
        let TypePattern::Named {
            path,
            arguments: [actual],
        } = method.signature.unwrap().result
        else {
            panic!("owned projection iterator");
        };
        assert_eq!(path, "Iter");
        assert_eq!(*actual, item);
    }
    for method in ["first_key_cloned", "last_key_cloned"] {
        signature(
            builtin("BTreeMap").unwrap(),
            method,
            Some(ReceiverMode::Shared),
            &[],
            TypePattern::Option(&KEY),
        );
    }
}

#[test]
fn set_signatures_keep_element_identity_and_self_results() {
    for name in ["HashSet", "BTreeSet"] {
        let declaration = builtin(name).unwrap();
        for (method, receiver, parameters, result) in [
            (
                "insert",
                ReceiverMode::Mutable,
                &[ITEM][..],
                TypePattern::Bool,
            ),
            (
                "remove",
                ReceiverMode::Mutable,
                &[ITEM_REF][..],
                TypePattern::Bool,
            ),
            (
                "contains",
                ReceiverMode::Shared,
                &[ITEM_REF][..],
                TypePattern::Bool,
            ),
            (
                "is_subset",
                ReceiverMode::Shared,
                &[SELF_REF][..],
                TypePattern::Bool,
            ),
            (
                "is_superset",
                ReceiverMode::Shared,
                &[SELF_REF][..],
                TypePattern::Bool,
            ),
            (
                "is_disjoint",
                ReceiverMode::Shared,
                &[SELF_REF][..],
                TypePattern::Bool,
            ),
            (
                "union",
                ReceiverMode::Shared,
                &[SELF_REF][..],
                TypePattern::SelfType,
            ),
            (
                "intersection",
                ReceiverMode::Shared,
                &[SELF_REF][..],
                TypePattern::SelfType,
            ),
            (
                "difference",
                ReceiverMode::Shared,
                &[SELF_REF][..],
                TypePattern::SelfType,
            ),
            (
                "symmetric_difference",
                ReceiverMode::Shared,
                &[SELF_REF][..],
                TypePattern::SelfType,
            ),
        ] {
            signature(declaration, method, Some(receiver), parameters, result);
        }
    }
    for method in ["first_cloned", "last_cloned"] {
        signature(
            builtin("BTreeSet").unwrap(),
            method,
            Some(ReceiverMode::Shared),
            &[],
            TypePattern::Option(&ITEM),
        );
    }
}
