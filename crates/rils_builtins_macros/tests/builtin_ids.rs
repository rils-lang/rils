rils_builtins_macros::builtin_id_declarations!("tests/fixtures/builtin_ids.toml");

#[test]
fn declarations_and_builtin_id_share_the_configured_value() {
    const VEC_PUSH: BuiltinId = builtin_id!("core::vec::push");
    const LEGACY_VEC_PUSH: BuiltinId = legacy_builtin_id!("core::vec::push");

    assert_eq!(VEC_PUSH, BuiltinId::VecPush);
    assert_eq!(LEGACY_VEC_PUSH, VEC_PUSH);
    assert_eq!(VEC_PUSH.as_raw(), 0x0200);
}
