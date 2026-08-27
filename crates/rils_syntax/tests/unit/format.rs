use super::*;

#[test]
fn parses_rust_style_format_pieces() {
    let pieces = parse_format_string("value={:+08.2} debug={:#?} hex={:#x} {{ok}}")
        .expect("valid format string");
    assert_eq!(
        pieces
            .iter()
            .filter(|piece| matches!(piece, FormatPiece::Placeholder { .. }))
            .count(),
        3
    );
    assert!(matches!(
        &pieces[1],
        FormatPiece::Placeholder { argument: 0, spec } if spec.sign_plus && spec.zero_pad && spec.width == Some(8) && spec.precision == Some(2)
    ));
}

#[test]
fn rejects_malformed_format_strings() {
    for source in ["{", "}", "{name}", "{} {1}", "{:#}", "{:.}"] {
        assert!(parse_format_string(source).is_err(), "{source}");
    }
}
