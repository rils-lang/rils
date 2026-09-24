use super::*;
use crate::lexer::lex;

#[test]
fn parses_function_and_if_expression() {
    let source = "fn max(a, b) { if a > b { a } else { b } }";
    let program = parse(lex(source).unwrap()).unwrap();
    assert!(matches!(program.statements[0], Stmt::Function { .. }));
}

#[test]
fn parses_explicit_generic_associated_paths() {
    let program = parse(lex("fn main() { Rc::<i32>::new(1) }").unwrap()).unwrap();
    let Stmt::Function { body, .. } = &program.statements[0] else {
        panic!("expected function");
    };
    let Stmt::Expr { expression, .. } = &body.statements[0] else {
        panic!("expected expression statement");
    };
    let Expr::Call { callee, .. } = expression else {
        panic!("expected call");
    };
    assert!(matches!(
        callee.as_ref(),
        Expr::GenericPath { segments, arguments, .. }
            if segments == &["Rc", "new"] && arguments.len() == 1
    ));
}

#[test]
fn signature_placeholders_require_trusted_parser_capabilities() {
    let tokens = lex("fn identity(value: _) -> _ {}").unwrap();
    let error = parse(tokens.clone()).unwrap_err();
    assert!(
        error
            .message
            .contains("reserved for trusted language packages")
    );

    let program = crate::parser::parse_with_capabilities(
        tokens,
        crate::macros::STANDARD_NATIVE_MACROS,
        crate::parser::ParseCapabilities::STANDARD_LIBRARY,
    )
    .unwrap();
    assert!(matches!(program.statements[0], Stmt::Function { .. }));
}

#[test]
fn parses_unit_and_empty_braced_structs() {
    let program = parse(lex("struct Unit; struct Empty {}").unwrap()).unwrap();
    assert!(matches!(
        &program.statements[0],
        Stmt::Struct { fields, .. } if fields.is_empty()
    ));
    assert!(matches!(
        &program.statements[1],
        Stmt::Struct { fields, .. } if fields.is_empty()
    ));
}

#[test]
fn unregistered_default_derive_is_rejected() {
    let error =
        parse(lex("#[derive(Default)] struct Settings { enabled: bool }").unwrap()).unwrap_err();
    assert_eq!(error.message, "unsupported derive `Default`");
}

#[test]
fn visibility_is_declaration_metadata() {
    let program = parse(lex("pub fn exported() {} fn private() {}").unwrap()).unwrap();
    assert!(matches!(
        program.statements.as_slice(),
        [
            Stmt::Function {
                visibility: Visibility::Public,
                span: public_span,
                ..
            },
            Stmt::Function {
                visibility: Visibility::Private,
                ..
            }
        ] if public_span.start == 0
    ));
    assert!(!Visibility::Restricted(crate::ast::VisibilityScope::Crate).is_public());
}

#[test]
fn rejects_visibility_on_non_declarations_and_duplicate_visibility() {
    let invalid_target = parse(lex("pub let answer = 42;").unwrap()).unwrap_err();
    assert!(
        invalid_target
            .message
            .contains("only allowed on declarations"),
        "{invalid_target:?}"
    );

    let duplicate = parse(lex("pub pub fn answer() {}").unwrap()).unwrap_err();
    assert!(
        duplicate.message.contains("already specified"),
        "{duplicate:?}"
    );
}

#[test]
fn rejects_invalid_assignment_target() {
    let error = parse(lex("(1 + 2) = 3;").unwrap()).unwrap_err();
    assert_eq!(error.message, "invalid assignment target");
}

#[test]
fn rejects_unclosed_delimiters_before_macro_fragment_matching() {
    for source in [
        "call(",
        "call([1, 2",
        "call({ let value = 1;",
        "call([1, 2})",
    ] {
        let error = parse(lex(source).unwrap()).expect_err("delimiter must be rejected");
        assert!(
            error.message.contains("expected") || error.message.contains("unexpected"),
            "unexpected error for `{source}`: {}",
            error.message
        );
    }
}

#[test]
fn only_functions_can_be_declared_inside_blocks() {
    let error = parse(lex("fn outer() { struct Local { value: i32 } }").unwrap())
        .expect_err("local type items are not part of the language");
    assert!(error.message.contains("module scope"));

    parse(lex("fn outer() { fn local() -> i32 { 1 } local() }").unwrap())
        .expect("nested functions remain valid");
}

#[test]
fn recognizes_function_call_comparisons_as_macro_expression_fragments() {
    let tokens = crate::lexer::lex("type_of(getter) == \"fn() -> i32\"").unwrap();
    let stream = crate::cursor::TokenStream::new(tokens).unwrap();
    assert!(super::is_expression_fragment_stream(&stream));
}

#[test]
fn accepts_reference_types_inside_generic_signatures() {
    parse(lex("fn identity(value: &i32) -> Option<&i32> { Some(value) }").unwrap())
        .expect("generic containers may carry inferred reference regions");
}

#[test]
fn reports_removed_numeric_type_names() {
    let integer = parse(lex("let value: int = 1;").unwrap()).unwrap_err();
    assert!(integer.message.contains("`int` was removed"));

    let float = parse(lex("let value: float = 1.0;").unwrap()).unwrap_err();
    assert!(float.message.contains("`float` was removed"));
}

#[test]
fn parses_crate_self_and_super_paths() {
    let source = r#"
        use crate::math::Value;
        fn read(value: self::Value) {
            crate::math::make();
            self::helper();
            super::super::shared::run();
        }
    "#;
    parse(lex(source).unwrap()).unwrap();
}

#[test]
fn flattens_grouped_nested_and_glob_use_trees() {
    let source = r#"
        use root::{self as root_alias, alpha, beta as b, nested::{delta, epsilon}, tools::*};
    "#;
    let program = parse(lex(source).unwrap()).unwrap();
    let Stmt::Use { imports, .. } = &program.statements[0] else {
        panic!("expected use statement");
    };
    assert_eq!(imports.len(), 6);
    assert_eq!(imports[0].path, ["root"]);
    assert_eq!(imports[0].binding_name(), Some("root_alias"));
    assert_eq!(imports[1].path, ["root", "alpha"]);
    assert_eq!(imports[2].binding_name(), Some("b"));
    assert_eq!(imports[3].path, ["root", "nested", "delta"]);
    assert_eq!(imports[4].path, ["root", "nested", "epsilon"]);
    assert_eq!(imports[5].path, ["root", "tools"]);
    assert_eq!(imports[5].kind, UseImportKind::Glob);
    assert!(imports.iter().all(|import| {
        import.path.len() == import.path_spans.len()
            && import.path_spans.iter().all(|span| span.start < span.end)
    }));
}
