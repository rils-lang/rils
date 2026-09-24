use rils_stdlib::stdlib::cell::Cell;

#[test]
fn native_cell_copies_and_replaces_values() {
    let cell = Cell::new(1);
    assert_eq!(cell.get(), 1);
    cell.set(2);
    assert_eq!(cell.replace(3), 2);
    assert_eq!(cell.get(), 3);
    assert_eq!(std::cell::Cell::get(&*cell), 3);
}

#[test]
fn native_cell_can_replace_non_copy_values() {
    let cell = Cell::new(String::from("first"));
    assert_eq!(cell.replace(String::from("second")), "first");
}
