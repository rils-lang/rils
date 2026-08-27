use super::append_path_entry;

#[test]
fn appends_a_missing_path_without_duplicate_separators() {
    assert_eq!(
        append_path_entry(r"C:\Tools;", r"C:\Users\me\.rils\bin"),
        Some(r"C:\Tools;C:\Users\me\.rils\bin".to_owned())
    );
}

#[test]
fn recognizes_equivalent_windows_path_entries() {
    assert_eq!(
        append_path_entry(
            r#"C:\Tools;"C:\Users\ME\.rils\bin\""#,
            r"C:/Users/me/.rils/bin"
        ),
        None
    );
}
