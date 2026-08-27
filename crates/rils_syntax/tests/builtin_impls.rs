use rils_syntax::{Type, ast::Stmt, lex, parse};

#[test]
fn parses_impls_for_builtin_generic_types() {
    let program =
        parse(lex("impl<T> Option<T> { fn take(&mut self) -> Self {} }").expect("source lexes"))
            .expect("source parses");

    let [
        Stmt::Impl {
            target, methods, ..
        },
    ] = program.statements.as_slice()
    else {
        panic!("expected one impl declaration");
    };
    assert!(matches!(target, Type::Option(_)));
    assert_eq!(methods[0].name, "take");
}

#[test]
fn parses_primitive_impls_and_builtin_member_attributes() {
    let source = r#"
        impl string {
            #[runtime(core::sequence::len)]
            fn len(&self) -> usize {}

            #[metadata]
            fn new() -> Self {}
        }
    "#;
    let program = parse(lex(source).expect("source lexes")).expect("source parses");
    let [
        Stmt::Impl {
            target, methods, ..
        },
    ] = program.statements.as_slice()
    else {
        panic!("expected one primitive impl declaration");
    };

    assert_eq!(target, &Type::String);
    assert_eq!(methods[0].attributes[0].path, ["runtime"]);
    assert_eq!(
        methods[0].attributes[0].arguments,
        [vec!["core", "sequence", "len"]]
    );
    assert_eq!(methods[1].attributes[0].path, ["metadata"]);
}

#[test]
fn parses_inferred_builtin_parameter_types_as_unknown() {
    let program =
        parse(lex("impl Formatter { fn write(&mut self, value: &_) {} }").expect("source lexes"))
            .expect("source parses");
    let [Stmt::Impl { methods, .. }] = program.statements.as_slice() else {
        panic!("expected one impl declaration");
    };

    assert_eq!(
        methods[0].parameters[1].type_annotation,
        Some(Type::Reference {
            mutable: false,
            inner: Box::new(Type::Unknown),
        })
    );
}
