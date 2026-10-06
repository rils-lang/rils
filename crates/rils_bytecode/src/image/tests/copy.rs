use super::*;

#[test]
fn verifier_rejects_forged_copy_declarations_including_inactive_variants() {
    let source = include_str!("../../../tests/fixtures/copy/declarations.rils");
    for target in ["Text", "Mixed", "Missing"] {
        let mut module = compile(source).unwrap();
        let mut implementation = module
            .trait_implementations
            .iter()
            .find(|implementation| implementation.trait_name == "Copy")
            .unwrap()
            .clone();
        implementation.target = target.into();
        module.trait_implementations.push(implementation);
        let error = module.verify().unwrap_err();
        assert!(error.message.contains("Copy"), "{target}: {error}");
        assert!(module.to_bytes().is_err(), "{target}: export must verify");
        assert!(
            module.execute_value().is_err(),
            "{target}: execution must verify"
        );
    }
}
