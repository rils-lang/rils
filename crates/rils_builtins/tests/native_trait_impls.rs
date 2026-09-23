use rils_builtins::{BuiltinKind, builtin, native_implements, native_implements_with};

#[test]
fn native_trait_markers_cover_only_supported_types() {
    for trait_name in ["Clone", "Copy"] {
        assert_eq!(
            builtin(trait_name).map(|item| item.kind),
            Some(BuiltinKind::Trait)
        );
    }
    for number in [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64",
    ] {
        assert!(native_implements(number, "Clone"), "{number}");
        assert!(native_implements(number, "Copy"), "{number}");
    }
    assert!(native_implements("string", "Clone"));
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
