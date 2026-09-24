use rils_stdlib::stdlib::cell::Cell;

#[test]
fn native_cell_clones_and_replaces_values() {
    let cell = Cell::new(String::from("first"));
    assert_eq!(cell.get(), "first");
    cell.set(String::from("second"));
    assert_eq!(cell.replace(String::from("third")), "second");
    assert_eq!(cell.get(), "third");
    assert_eq!(cell.get(), "third");
}

#[test]
fn native_cell_restores_its_value_after_clone_panics() {
    struct PanicClone;

    impl Clone for PanicClone {
        fn clone(&self) -> Self {
            panic!("clone failed")
        }
    }

    let cell = Cell::new(PanicClone);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cell.get()));
    assert!(result.is_err());
    cell.replace(PanicClone);
}
