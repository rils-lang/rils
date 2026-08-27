use std::ffi::OsString;

use super::take_explicit_toolchain;

#[test]
fn consumes_plus_version_before_forwarding_arguments() {
    let mut arguments = vec![OsString::from("+0.4.0"), OsString::from("repl")];
    assert_eq!(
        take_explicit_toolchain(&mut arguments).unwrap().as_deref(),
        Some("0.4.0")
    );
    assert_eq!(arguments, [OsString::from("repl")]);
}
