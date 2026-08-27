use super::validate_toolchain;

#[test]
fn normalizes_semantic_versions() {
    assert_eq!(validate_toolchain("v0.4.0").unwrap(), "0.4.0");
    assert_eq!(validate_toolchain("0.4.0-rc.1").unwrap(), "0.4.0-rc.1");
}

#[test]
fn rejects_channels_and_paths_as_installed_toolchains() {
    assert!(validate_toolchain("stable").is_err());
    assert!(validate_toolchain("../0.4.0").is_err());
}
