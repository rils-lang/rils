use rils_stdlib::stdlib::basic::Box as RilsBox;

#[test]
fn boxed_wrapper_exposes_the_std_box() {
    let mut value = RilsBox::new(1);
    **value = 2;
    assert_eq!(**value, 2);
    assert_eq!(value.into_inner(), 2);
}
