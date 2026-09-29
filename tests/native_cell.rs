use rils::{Value, compile, eval_value};

#[test]
fn native_cell_definition_runs_in_interpreter_and_vm() {
    let source = r#"
        let cell: core::cell::Cell<i32> = core::cell::Cell::new(1);
        cell.set(2);
        cell.replace(3) + cell.get()
    "#;
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(5));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(5)
    );
}

#[test]
fn cell_get_rejects_non_copy_values_in_both_backends() {
    let source = r#"
        let cell: Cell<string> = Cell::new("first");
        cell.get()
    "#;
    let interpreted =
        eval_value(source).expect_err("Cell<string>::get must reject non-Copy values");
    assert!(interpreted.to_string().contains("Copy"));
    let module = compile(source).expect("the transitional runtime still checks method bounds");
    let compiled = module
        .execute_value()
        .expect_err("VM must reject non-Copy values");
    assert!(compiled.to_string().contains("Copy"));
}

#[test]
fn cells_with_concrete_items_use_native_storage_and_move_values() {
    for source in [
        "let cell: Cell<i32> = Cell::new(4); cell",
        "let cell: Cell<string> = Cell::new(\"first\"); cell",
        "struct Item { value: i32 } let cell: Cell<Item> = Cell::new(Item { value: 4 }); cell",
    ] {
        assert!(
            matches!(eval_value(source).unwrap(), Value::Dynamic(_)),
            "{source}"
        );
        assert!(
            matches!(
                compile(source).unwrap().execute_value().unwrap(),
                Value::Dynamic(_)
            ),
            "{source}"
        );
    }
    let source = "let cell: Cell<string> = Cell::new(\"first\"); let old = cell.replace(\"second\"); cell.set(\"third\"); old";
    assert_eq!(eval_value(source).unwrap().to_string(), "first");
    assert_eq!(
        compile(source)
            .unwrap()
            .execute_value()
            .unwrap()
            .to_string(),
        "first"
    );
}

#[test]
fn ref_cells_use_native_storage_and_keep_borrowed_items_live() {
    for source in [
        "let cell: RefCell<i32> = RefCell::new(4); cell",
        "let cell: RefCell<string> = RefCell::new(\"text\"); cell",
        "struct Item { value: i32 } let cell: RefCell<Item> = RefCell::new(Item { value: 4 }); cell",
    ] {
        assert!(
            matches!(eval_value(source).unwrap(), Value::Dynamic(_)),
            "{source}"
        );
        assert!(
            matches!(
                compile(source).unwrap().execute_value().unwrap(),
                Value::Dynamic(_)
            ),
            "{source}"
        );
    }
    let source = "let cell: RefCell<i32> = RefCell::new(4); *cell.borrow_mut() = 7; *cell.borrow() + cell.replace(8)";
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(14));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(14)
    );
    let source = "struct Item { value: i32 } { let cell: RefCell<Item> = RefCell::new(Item { value: 4 }); let item = cell.borrow(); item.value }";
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(4));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(4)
    );
    let source = "struct Item { value: i32 } { let cell: RefCell<Item> = RefCell::new(Item { value: 4 }); let mut item = cell.borrow_mut(); item.value = 9; let current = cell.borrow(); current.value }";
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(9));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(9)
    );
}

#[test]
fn native_ref_cell_rejects_replacement_while_referenced() {
    let source = "{ let cell: RefCell<i32> = RefCell::new(4); let item = cell.borrow(); cell.replace(5); *item }";
    assert!(
        eval_value(source)
            .unwrap_err()
            .to_string()
            .contains("borrowed")
    );
    assert!(
        compile(source)
            .unwrap()
            .execute_value()
            .unwrap_err()
            .to_string()
            .contains("borrowed")
    );
}
