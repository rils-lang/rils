use rils_syntax::{Span, ast::Stmt, rils_quote, rils_quote_tokens};

fn invalid_derive(
    _: &Stmt,
) -> Result<Option<rils_syntax::quote::QuotedStatement>, rils_syntax::ParseError> {
    Ok(Some(rils_quote! { impl Broken for {} }))
}

#[test]
fn interpolates_names_and_repeated_fields_into_one_statement() {
    let origin = Span::new(12, 20);
    let name = "Pair";
    let fields = ["left: i32", "right: i32"];
    let quoted = rils_quote! { struct #name { #(#fields),* } }
        .parse(origin)
        .unwrap();
    let Stmt::Struct {
        name, fields, span, ..
    } = quoted
    else {
        panic!("expected a struct");
    };
    assert_eq!(name, "Pair");
    assert_eq!(fields.len(), 2);
    assert_eq!(span, origin);
}

#[test]
fn generated_parse_errors_point_to_the_source_declaration() {
    let origin = Span::new(30, 35);
    let field = "value:";
    let error = rils_quote! { struct Broken { #field } }
        .parse(origin)
        .unwrap_err();
    assert_eq!(error.span, origin);
}

#[test]
fn token_fragments_can_be_repeated_in_a_statement() {
    let name = "Pair";
    let left = "left";
    let right = "right";
    let fields = [
        rils_quote_tokens!(#left: i32),
        rils_quote_tokens!(#right: i32),
    ];
    let quoted = rils_quote! { struct #name { #(#fields),* } }
        .parse(Span::new(1, 5))
        .unwrap();
    assert!(matches!(quoted, Stmt::Struct { fields, .. } if fields.len() == 2));
}

#[test]
fn derive_expansion_supplies_the_source_span_without_template_syntax() {
    let tokens = rils_syntax::lex("#[derive(Probe)] struct Sample;").unwrap();
    let error = rils_syntax::parser::parse_with_native_macros_and_derives(
        tokens,
        rils_syntax::macros::STANDARD_NATIVE_MACROS,
        &[rils_syntax::derive::NativeDeriveDefinition {
            name: "Probe",
            expand: invalid_derive,
        }],
        rils_syntax::ParseCapabilities::USER,
    )
    .unwrap_err();
    assert_eq!(error.span, Span::new(17, 31));
}
