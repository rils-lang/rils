use rils_builtins::builtin_function;

#[test]
fn qualified_function_lookup_follows_declared_module_exports() {
    for (qualified, canonical) in [
        ("core::option::Some", "Some"),
        ("core::result::Ok", "Ok"),
        ("core::result::Err", "Err"),
        ("core::result::unwrap", "unwrap"),
    ] {
        assert!(
            std::ptr::eq(
                builtin_function(qualified).unwrap(),
                builtin_function(canonical).unwrap(),
            ),
            "{qualified}"
        );
    }
    for path in [
        "std::fs::Ok",
        "core::result::Some",
        "missing::Err",
        "core::result::Result",
    ] {
        assert!(builtin_function(path).is_none(), "{path}");
    }
}
