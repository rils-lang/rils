use rils_builtins::{BuiltinKind, builtin, native_implements, native_implements_with};

#[test]
fn rc_is_clone_without_requiring_clone_for_its_value() {
    assert!(native_implements("Rc", "Clone"));
}

#[test]
fn native_trait_markers_cover_only_supported_types() {
    for trait_name in ["Clone", "Copy", "Default", "Eq", "Hash"] {
        assert_eq!(
            builtin(trait_name).map(|item| item.kind),
            Some(BuiltinKind::Trait)
        );
    }
    for number in [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
    ] {
        for trait_name in ["Clone", "Copy", "Default", "Eq", "Hash"] {
            assert!(
                native_implements(number, trait_name),
                "{number}: {trait_name}"
            );
        }
    }
    for number in ["f32", "f64"] {
        for trait_name in ["Clone", "Copy", "Default"] {
            assert!(
                native_implements(number, trait_name),
                "{number}: {trait_name}"
            );
        }
        for trait_name in ["Eq", "Hash"] {
            assert!(
                !native_implements(number, trait_name),
                "{number}: {trait_name}"
            );
        }
    }
    for trait_name in ["Clone", "Default", "Eq", "Hash"] {
        assert!(
            native_implements("string", trait_name),
            "string: {trait_name}"
        );
    }
    assert!(!native_implements("string", "Copy"));
    assert!(!native_implements("Option", "Copy"));
    assert!(!native_implements("Result", "Copy"));

    for (type_name, trait_name, arguments, expected) in [
        ("Option", "Clone", [true, false], true),
        ("Option", "Copy", [true, false], true),
        ("Option", "Copy", [false, false], false),
        ("Result", "Clone", [true, true], true),
        ("Result", "Clone", [true, false], false),
        ("Result", "Copy", [true, true], true),
        ("Result", "Copy", [false, true], false),
    ] {
        assert_eq!(
            native_implements_with(type_name, trait_name, |parameter, _| match parameter {
                "T" => arguments[0],
                "E" => arguments[1],
                _ => false,
            }),
            expected,
            "{type_name}: {trait_name}"
        );
    }
}
