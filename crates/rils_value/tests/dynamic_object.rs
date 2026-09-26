use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

#[test]
fn copy_option_uses_inline_handles_and_registered_operations() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let layout = DynamicLayout::option(item.clone()).unwrap();
    let descriptor = Rc::new(
        DynamicType::<bool>::new(layout.clone())
            .register_method("is_some", |context| context.option_is_some()),
    );
    let object = DynamicObject::new(
        descriptor,
        DynamicValue::some(layout, DynamicValue::from_rust(item, 42).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(object.is_inline());
    assert_eq!(object.call("is_some", &[]), Some(Ok(true)));
    assert_eq!(object.clone().call("is_some", &[]), Some(Ok(true)));
    assert_eq!(
        object.copy_owned().unwrap().call("is_some", &[]),
        Some(Ok(true))
    );
}

#[test]
fn noncopy_option_shares_payload_and_drops_it_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let layout = DynamicLayout::option(item.clone()).unwrap();
    let descriptor = Rc::new(DynamicType::<()>::new(layout.clone()));
    let object = DynamicObject::new(
        descriptor,
        DynamicValue::some(
            layout,
            DynamicValue::from_rust(item, Probe(drops.clone())).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!object.is_inline());
    assert!(object.copy_owned().is_err());
    let alias = object.clone();
    drop(object);
    assert_eq!(drops.get(), 0);
    drop(alias);
    assert_eq!(drops.get(), 1);
}

#[test]
fn dynamic_object_rejects_a_different_descriptor() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let first = DynamicLayout::option(item.clone()).unwrap();
    let second = DynamicLayout::option(item).unwrap();
    let value = DynamicValue::none(first).unwrap();
    assert!(DynamicObject::<()>::new(Rc::new(DynamicType::new(second)), value).is_err());
}
