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

#[test]
fn cell_get_rejects_non_copy_values_in_both_backends() {
    let source = r#"
        let cell: Cell<string> = Cell::new("first");
        cell.get()
    "#;
    let interpreted = eval(source).expect_err("Cell<string>::get must reject non-Copy values");
    assert!(interpreted.to_string().contains("Copy"));
    let module = compile(source).expect("the transitional runtime still checks method bounds");
    let compiled = module
        .execute()
        .expect_err("VM must reject non-Copy values");
    assert!(compiled.to_string().contains("Copy"));
}
