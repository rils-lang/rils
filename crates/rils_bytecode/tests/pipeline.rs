use std::path::PathBuf;

use rils_bytecode::{BytecodeModule, compile, compile_file};
use rils_runtime::Value;

#[test]
fn compiled_image_round_trips_before_execution() {
    let module = compile("40 + 2").expect("source should compile");
    let bytes = module.to_bytes().expect("verified module should encode");
    let restored = BytecodeModule::from_bytes(&bytes).expect("encoded module should decode");

    assert_eq!(restored.execute().unwrap(), Value::I32(42));
}

#[test]
fn compile_file_uses_the_shared_module_loading_rules() {
    let entry =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/module_tree/main.rils");
    let module = compile_file(entry).expect("fixture module tree should compile");

    assert_eq!(module.execute().unwrap(), Value::I32(42));
}
