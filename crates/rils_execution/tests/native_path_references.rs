use std::{cell::Cell, rc::Rc};

use rils_execution::{
    Type, Value,
    value::{ReferenceValue, record_codec, record_layout::RecordLayoutResolver},
};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

#[test]
fn reborrow_of_an_inline_copy_record_retains_the_same_reference_owner() {
    let number = RecordLayoutResolver::new(&[]).resolve(&Type::I32).unwrap();
    let layout = DynamicLayout::record(
        Type::named("Inline"),
        vec![("count".into(), number.clone())],
    )
    .unwrap();
    let count = record_codec::into_native(Value::from_i32(1), number).unwrap();
    let payload = DynamicValue::record(layout.clone(), vec![count]).unwrap();
    let object = DynamicObject::new(Rc::new(DynamicType::new(layout)), payload).unwrap();
    assert!(object.is_inline());
    let first =
        Rc::new(ReferenceValue::new_dynamic_field(object, 0, true, None, vec![], vec![]).unwrap());
    let second = first.reborrow(true).unwrap();
    second.write(Value::from_i32(42)).unwrap();
    assert_eq!(first.copy_native().unwrap(), Some(Value::from_i32(42)));
    assert_eq!(second.copy_native().unwrap(), Some(Value::from_i32(42)));
}

#[test]
fn native_projection_and_type_queries_do_not_clone_unregistered_payloads() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let drops = Rc::new(Cell::new(0));
    let probe = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let number = RecordLayoutResolver::new(&[]).resolve(&Type::I32).unwrap();
    let inner = DynamicLayout::record(
        Type::named("Inner"),
        vec![
            ("probe".into(), probe.clone()),
            ("count".into(), number.clone()),
        ],
    )
    .unwrap();
    let outer =
        DynamicLayout::record(Type::named("Outer"), vec![("inner".into(), inner.clone())]).unwrap();
    let count = record_codec::into_native(Value::from_i32(1), number).unwrap();
    let inner = DynamicValue::record(
        inner,
        vec![
            DynamicValue::from_rust(probe, Probe(drops.clone())).unwrap(),
            count,
        ],
    )
    .unwrap();
    let payload = DynamicValue::record(outer.clone(), vec![inner]).unwrap();
    let object = DynamicObject::new_shared(Rc::new(DynamicType::new(outer)), payload).unwrap();
    let inner =
        Rc::new(ReferenceValue::new_dynamic_field(object, 0, true, None, vec![], vec![]).unwrap());
    assert!(
        inner.read().is_err(),
        "Probe deliberately has no borrowed Clone registration"
    );
    assert_eq!(Value::Reference(inner.clone()).type_name(), "&mut Inner");
    assert_eq!(
        Type::of_value(&Value::Reference(inner.clone())),
        Some(Type::Reference {
            mutable: true,
            inner: Box::new(Type::named("Inner"))
        })
    );
    let first = Rc::new(inner.project_native_field("count").unwrap().unwrap());
    let second = first.reborrow(true).unwrap();
    assert_eq!(first.copy_native().unwrap(), Some(Value::from_i32(1)));
    second.write(Value::from_i32(42)).unwrap();
    assert_eq!(first.copy_native().unwrap(), Some(Value::from_i32(42)));
    assert_eq!(drops.get(), 0);
    drop(inner);
    drop(first);
    assert_eq!(
        drops.get(),
        0,
        "the projected reference retains the root owner"
    );
    drop(second);
    assert_eq!(drops.get(), 1);
}
