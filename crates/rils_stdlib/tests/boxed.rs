use rils_stdlib::stdlib::basic::Box as RilsBox;

#[test]
fn boxed_wrapper_exposes_the_std_box() {
    let mut value = RilsBox::from(std::boxed::Box::new(1));
    **value = 2;
    assert_eq!(**value, 2);
}
