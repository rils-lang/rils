use std::{cell::Cell, panic::AssertUnwindSafe, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicType, DynamicValue};

#[test]
fn partial_move_checks_only_active_fields_and_restoration_clears_the_state() {
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let record = DynamicLayout::record(
        Type::named("Record"),
        vec![("number".into(), number.clone())],
    )
    .unwrap();
    let optional = DynamicLayout::option(record.clone()).unwrap();
    assert!(
        !DynamicValue::none(optional.clone())
            .unwrap()
            .view()
            .is_partially_moved()
            .unwrap()
    );
    let sum_type = Type::Result(Box::new(optional.rils_type().clone()), Box::new(Type::Unit));
    let sum = DynamicLayout::variant(
        sum_type,
        vec![optional.clone(), DynamicLayout::copy_of::<()>(Type::Unit)],
    )
    .unwrap();
    let sequence = DynamicLayout::sequence(Type::named("Sequence"), sum.clone());
    let mut value = DynamicValue::sequence(
        sequence,
        vec![
            DynamicValue::variant(
                sum,
                0,
                DynamicValue::some(
                    optional,
                    DynamicValue::record(
                        record,
                        vec![DynamicValue::from_rust(number, 42_i32).unwrap()],
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let path = [
        DynamicPathStep::Index(0),
        DynamicPathStep::Variant(0),
        DynamicPathStep::Some,
        DynamicPathStep::Field(0),
    ];
    assert!(!value.view().is_partially_moved().unwrap());
    let moved = value.take_path_field(&path).unwrap();
    assert!(value.view().is_partially_moved().unwrap());
    assert!(value.view_path(&path).is_err());
    value.put_path_field(&path, moved).unwrap();
    assert!(!value.view().is_partially_moved().unwrap());
}

#[test]
fn declaration_metadata_is_typed_shared_and_owned_by_the_descriptor() {
    struct Context(Rc<Cell<usize>>);
    impl Drop for Context {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let metadata = Rc::new(Context(drops.clone()));
    let descriptor: DynamicType<()> = DynamicType::new(DynamicLayout::copy_of::<i32>(Type::I32))
        .register_metadata(metadata.clone());
    assert!(Rc::ptr_eq(
        &descriptor.metadata::<Context>().unwrap(),
        &metadata
    ));
    assert!(descriptor.metadata::<String>().is_none());
    drop(metadata);
    assert_eq!(drops.get(), 0);
    drop(descriptor);
    assert_eq!(drops.get(), 1);
}

#[test]
fn duplicate_metadata_registration_is_rejected() {
    let descriptor: DynamicType<()> = DynamicType::new(DynamicLayout::copy_of::<i32>(Type::I32))
        .register_metadata(Rc::new(String::from("first")));
    assert!(
        std::panic::catch_unwind(AssertUnwindSafe(
            || descriptor.register_metadata(Rc::new(String::from("second")))
        ))
        .is_err()
    );
}
