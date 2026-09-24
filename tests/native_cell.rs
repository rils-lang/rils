use rils::{Value, compile, eval};

#[test]
fn native_cell_definition_runs_in_interpreter_and_vm() {
    let source = r#"
        let cell: core::cell::Cell<i32> = core::cell::Cell::new(1);
        cell.set(2);
        cell.replace(3) + cell.get()
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(5));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(5));
}
