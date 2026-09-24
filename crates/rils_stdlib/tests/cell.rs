use rils_stdlib::stdlib::cell::Cell;

#[test]
fn native_cell_clones_and_replaces_values() {
    let cell = Cell::new(String::from("first"));
    assert_eq!(cell.get(), "first");
    cell.set(String::from("second"));
    assert_eq!(cell.replace(String::from("third")), "second");
    assert_eq!(cell.get(), "third");
    assert_eq!(cell.borrow().as_str(), "third");
}
