use rils_stdlib::stdlib::rc::native_storage::{strong, weak};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue};
use std::{cell::Cell, rc::Rc};

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn native_handles_share_non_clone_payload_and_expire_after_last_owner() {
    let drops = Rc::new(Cell::new(0));
    let value = DynamicValue::from_rust(
        DynamicLayout::of::<Probe>(Type::named("Probe")),
        Probe(drops.clone()),
    )
    .unwrap();
    let owner = strong::new(value).unwrap();
    let observer = strong::downgrade(owner.view()).unwrap();
    for _ in 0..3 {
        assert_eq!(strong::strong_count(owner.view()).unwrap(), 1);
        assert_eq!(weak::strong_count(observer.view()).unwrap(), 1);
        assert_eq!(weak::weak_count(observer.view()).unwrap(), 1);
    }
    let upgraded = weak::upgrade(observer.view()).unwrap();
    assert!(upgraded.view().option_is_some().unwrap());
    assert_eq!(strong::strong_count(owner.view()).unwrap(), 2);
    drop(owner);
    assert_eq!(drops.get(), 0);
    drop(upgraded);
    assert_eq!(drops.get(), 1);
    assert_eq!(weak::strong_count(observer.view()).unwrap(), 0);
    let expired = weak::upgrade(observer.view()).unwrap();
    assert!(!expired.view().option_is_some().unwrap());
    assert_eq!(
        expired.descriptor().rils_type(),
        &Type::Option(Box::new(Type::Named {
            name: "Rc".into(),
            arguments: vec![Type::named("Probe")]
        }))
    );
    drop(observer);
    assert_eq!(drops.get(), 1);
}

#[test]
fn shared_operations_reject_other_handle_types() {
    let scalar = DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), 1).unwrap();
    let owner = strong::new(scalar).unwrap();
    let observer = strong::downgrade(owner.view()).unwrap();
    assert!(strong::strong_count(observer.view()).is_err());
    assert!(strong::downgrade(observer.view()).is_err());
    assert!(weak::upgrade(owner.view()).is_err());
    assert!(weak::strong_count(owner.view()).is_err());
    assert!(weak::weak_count(owner.view()).is_err());
}
