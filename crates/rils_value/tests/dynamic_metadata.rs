use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicType};
use std::{cell::Cell, rc::Rc};

#[test]
fn lazy_metadata_is_installed_once_and_initializer_can_read_other_metadata() {
    let descriptor = DynamicType::<()>::new(DynamicLayout::copy_of::<i32>(Type::I32))
        .register_metadata(Rc::new(7_u32));
    let calls = Cell::new(0);
    let first = descriptor.metadata_or_insert_with(|| {
        calls.set(calls.get() + 1);
        Rc::new(format!("number {}", descriptor.metadata::<u32>().unwrap()))
    });
    let second = descriptor.metadata_or_insert_with(|| {
        calls.set(calls.get() + 1);
        Rc::new("unexpected".to_owned())
    });
    assert!(Rc::ptr_eq(&first, &second));
    assert!(Rc::ptr_eq(
        &first,
        &descriptor.metadata::<String>().unwrap()
    ));
    assert_eq!(first.as_str(), "number 7");
    assert_eq!(calls.get(), 1);
}

#[test]
fn reentrant_initializer_preserves_the_first_installed_context() {
    let descriptor = DynamicType::<()>::new(DynamicLayout::copy_of::<i32>(Type::I32));
    let value = descriptor.metadata_or_insert_with(|| {
        descriptor.metadata_or_insert_with(|| Rc::new(3_u32));
        Rc::new(5_u32)
    });
    assert_eq!(*value, 3);
}
