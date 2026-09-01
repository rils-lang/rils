use std::path::PathBuf;

use rils_runtime::{Engine, Value};

#[test]
fn eval_file_loads_external_modules() {
    let entry =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/module_tree/main.rils");
    let value = Engine::new()
        .eval_file(entry)
        .expect("fixture module tree should execute");

    assert_eq!(value, Value::I32(42));
}
