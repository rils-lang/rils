use rils_frontend::{Type, parse, semantic::DeclarationTypeResolver};

#[test]
fn declarations_keep_defining_modules_and_expand_imported_aliases() {
    let tokens =
        rils_frontend::lex(include_str!("fixtures/declaration_types/modules.rils")).unwrap();
    let program = parse(tokens).unwrap();
    let resolver = DeclarationTypeResolver::from_programs([(&[] as &[String], &program)]);
    for (module, name, expected) in [
        ("left", "Item", Type::named("left::Item")),
        ("left::nested", "Item", Type::named("left::Item")),
        ("right", "Item", Type::named("right::Item")),
        ("right", "Imported", Type::named("left::Item")),
        (
            "right",
            "Local",
            Type::Option(Box::new(Type::named("left::Item"))),
        ),
        (
            "consumer",
            "Imported",
            Type::Result(
                Box::new(Type::Option(Box::new(Type::named("left::Item")))),
                Box::new(Type::named("String")),
            ),
        ),
        ("ambiguous", "Local", Type::named("Local")),
        ("cyclic", "a::Missing", Type::named("a::Missing")),
        ("cyclic", "Missing", Type::named("Missing")),
        ("cyclic", "First", Type::named("cyclic::First")),
    ] {
        let module = module.split("::").map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            resolver.resolve(&Type::named(name), &module),
            expected,
            "{module:?}::{name}"
        );
    }
}

#[test]
fn source_type_access_checks_identity_before_expanding_aliases() {
    let source = include_str!("fixtures/declaration_types/visibility.rils");
    let program = parse(rils_frontend::lex(source).unwrap()).unwrap();
    let resolver = DeclarationTypeResolver::from_programs([(&[] as &[String], &program)]);
    for (module, name, blocked) in [
        ("", "model::Secret", true),
        ("model", "Secret", false),
        ("model::nested", "super::Secret", false),
        ("", "model::Public", false),
        ("", "public_module::hidden::Item", true),
        ("public_module", "hidden::Item", false),
    ] {
        let module = module
            .split("::")
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(
            resolver.inaccessible_type_path(name, &module).is_some(),
            blocked,
            "{module:?}::{name}"
        );
    }
}
