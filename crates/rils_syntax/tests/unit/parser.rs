use super::*;
use crate::lexer::lex;

#[test]
fn parses_function_and_if_expression() {
    let source = "fn max(a, b) { if a > b { a } else { b } }";
    let program = parse(lex(source).unwrap()).unwrap();
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
fn parses_and_expands_default_derive() {
    let program =
        parse(lex("#[derive(Default)] pub struct Settings { enabled: bool, count: i32 }").unwrap())
            .unwrap();
    assert!(matches!(program.statements[0], Stmt::Public { .. }));
    assert!(matches!(
        &program.statements[1],
        Stmt::Impl { trait_name: Some(name), methods, .. }
            if name == "Default" && methods.len() == 1 && methods[0].name == "default"
    ));
}

#[test]
fn default_derive_rejects_non_default_fields() {
    let error =
        parse(lex("#[derive(Default)] struct Bad { callback: fn() -> () }").unwrap()).unwrap_err();
    assert!(error.message.contains("field `callback`"), "{error:?}");
    assert!(
        error.message.contains("does not implement Default"),
        "{error:?}"
    );
}

#[test]
fn default_derive_adds_required_generic_bounds() {
    let program = parse(lex("#[derive(Default)] struct Wrapper<T> { value: T }").unwrap()).unwrap();
    let Stmt::Impl {
        generic_parameters, ..
    } = &program.statements[1]
    else {
        panic!("expected generated impl");
    };
    assert_eq!(generic_parameters[0].bounds, ["Default"]);
}

#[test]
fn default_derive_rejects_an_explicit_impl_for_the_same_type() {
    let error = parse(
        lex(
            "#[derive(Default)] struct Value; impl Default for Value { fn default() -> Self { loop {} } }",
        )
        .unwrap(),
    )
    .unwrap_err();
    assert!(error.message.contains("both derive Default"), "{error:?}");
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
    let mut tokens = crate::lexer::lex("type_of(getter) == \"fn() -> i32\"").unwrap();
    tokens.pop();
    assert!(super::is_expression_fragment(&tokens));
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
