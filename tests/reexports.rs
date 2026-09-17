#[test]
fn reexport_chains_agree_in_interpreter_and_bytecode() {
    let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/reexports/src/main.rils");
    let interpreted = rils::Engine::new().eval_file(&entry).unwrap();
    let compiled = rils::compile_file(&entry).unwrap().execute().unwrap();
    assert_eq!(interpreted, rils::Value::I32(42));
    assert_eq!(compiled, interpreted);
}
