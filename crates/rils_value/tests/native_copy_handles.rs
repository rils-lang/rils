use std::{cell::Cell, panic::AssertUnwindSafe, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue};

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn copy_retains_handles_through_records_options_variants_and_projections() {
    let drops = Rc::new(Cell::new(0));
    let owner = Rc::new(Probe(drops.clone()));
    let handle = DynamicLayout::copy_handle_of::<Rc<Probe>>(Type::named("Handle"));
    let optional = DynamicLayout::option(handle.clone()).unwrap();
    let sum = DynamicLayout::variant(
        Type::Result(Box::new(optional.rils_type().clone()), Box::new(Type::Unit)),
        vec![optional.clone(), DynamicLayout::copy_of::<()>(Type::Unit)],
    )
    .unwrap();
    let layout =
        DynamicLayout::record(Type::named("Record"), vec![("item".into(), sum.clone())]).unwrap();
    let item = DynamicValue::from_rust(handle, owner.clone()).unwrap();
    let item = DynamicValue::some(optional, item).unwrap();
    let item = DynamicValue::variant(sum, 0, item).unwrap();
    let mut value = DynamicValue::record(layout, vec![item]).unwrap();
    let path = [
        DynamicPathStep::Field(0),
        DynamicPathStep::Variant(0),
        DynamicPathStep::Some,
    ];
    let copy = value.copy_owned().unwrap();
    let projection = value.copy_path(&path).unwrap();
    assert_eq!(Rc::strong_count(&owner), 4);
    assert!(
        projection
            .with::<Rc<Probe>, _>(|value| Rc::ptr_eq(value, &owner))
            .unwrap()
    );
    let moved = value.take_field(0).unwrap();
    let partial_copy = value.copy_owned().unwrap();
    assert!(!partial_copy.field_is_live(0).unwrap());
    assert_eq!(Rc::strong_count(&owner), 4);
    drop(owner);
    drop(value);
    drop(partial_copy);
    drop(copy);
    drop(projection);
    assert_eq!(drops.get(), 0);
    drop(moved);
    assert_eq!(drops.get(), 1);
}

#[test]
fn absent_handles_are_neither_read_nor_retained() {
    let handle = DynamicLayout::copy_handle_of::<Rc<[u8; 128]>>(Type::named("Handle"));
    let optional = DynamicLayout::option(handle).unwrap();
    let none = DynamicValue::none(optional).unwrap();
    assert!(!none.copy_owned().unwrap().is_some().unwrap());
    assert!(!none.copy_path(&[]).unwrap().is_some().unwrap());
}

#[test]
fn panic_during_copy_releases_completed_children_and_preserves_source() {
    #[derive(Clone)]
    struct Handle {
        owner: Rc<Probe>,
        panic: bool,
    }
    impl Handle {
        fn new(owner: Rc<Probe>, panic: bool) -> Self {
            Self { owner, panic }
        }
    }
    struct FallibleHandle(Handle);
    impl Clone for FallibleHandle {
        fn clone(&self) -> Self {
            assert!(!self.0.panic, "deliberate copy failure");
            Self(self.0.clone())
        }
    }
    let drops = Rc::new(Cell::new(0));
    let owner = Rc::new(Probe(drops.clone()));
    let handle = DynamicLayout::copy_handle_of::<FallibleHandle>(Type::named("Handle"));
    let layout = DynamicLayout::record(
        Type::named("Record"),
        vec![
            ("first".into(), handle.clone()),
            ("second".into(), handle.clone()),
        ],
    )
    .unwrap();
    let value = DynamicValue::record(
        layout,
        vec![
            DynamicValue::from_rust(
                handle.clone(),
                FallibleHandle(Handle::new(owner.clone(), false)),
            )
            .unwrap(),
            DynamicValue::from_rust(handle, FallibleHandle(Handle::new(owner.clone(), true)))
                .unwrap(),
        ],
    )
    .unwrap();
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| value.copy_owned())).is_err());
    assert_eq!(
        Rc::strong_count(&owner),
        3,
        "completed copies must be cleaned up"
    );
    assert!(
        value
            .with_field::<FallibleHandle, _>(0, |item| Rc::ptr_eq(&item.0.owner, &owner))
            .unwrap()
    );
    drop(owner);
    drop(value);
    assert_eq!(drops.get(), 1);
}

#[test]
fn leaf_traversal_skips_inactive_and_moved_children_and_stops_early() {
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let optional = DynamicLayout::option(number.clone()).unwrap();
    let sequence = DynamicLayout::sequence(Type::named("Numbers"), number.clone());
    let record = DynamicLayout::record(
        Type::named("Record"),
        vec![
            ("absent".into(), optional.clone()),
            ("moved".into(), number.clone()),
            ("numbers".into(), sequence.clone()),
        ],
    )
    .unwrap();
    let mut value = DynamicValue::record(
        record,
        vec![
            DynamicValue::none(optional).unwrap(),
            DynamicValue::from_rust(number.clone(), 1_i32).unwrap(),
            DynamicValue::sequence(
                sequence,
                vec![
                    DynamicValue::from_rust(number.clone(), 41_i32).unwrap(),
                    DynamicValue::from_rust(number, 42_i32).unwrap(),
                ],
            )
            .unwrap(),
        ],
    )
    .unwrap();
    drop(value.take_field(1).unwrap());
    let mut visits = 0;
    assert!(
        value
            .view()
            .any_leaf(&mut |leaf| {
                visits += 1;
                leaf.with_rust::<i32, _>(|number| *number == 41)
            })
            .unwrap()
    );
    assert_eq!(visits, 1);
}

#[test]
fn managed_and_bitwise_copy_descriptors_are_not_interchangeable() {
    let plain = DynamicLayout::copy_of::<u32>(Type::named("Token"));
    let managed = DynamicLayout::copy_handle_of::<u32>(Type::named("Token"));
    assert!(!plain.compatible_with(&managed));
    assert!(managed.is_rust_type::<u32>());
    assert!(!managed.is_rust_type::<u64>());
}
