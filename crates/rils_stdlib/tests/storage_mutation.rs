use rils_stdlib::{Type, stdlib::collections::vector::Vec as RilsVec};
use rils_value::{DynamicLayout, DynamicValue};
use std::{cell::Cell, rc::Rc};

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn truncation_and_clear_drop_exactly_the_removed_noncopy_elements() {
    for length in [0usize, 1, 4] {
        for keep in [0usize, 1, 8] {
            let drops = Rc::new(Cell::new(0));
            let mut values: RilsVec<Probe> = (0..length)
                .map(|_| Probe(drops.clone()))
                .collect::<Vec<_>>()
                .into();
            let _: () = values.truncate(keep);
            assert_eq!(values.len(), length.min(keep));
            assert_eq!(drops.get(), length.saturating_sub(keep));
            let _: () = values.clear();
            assert_eq!(drops.get(), length);
            drop(values);
            assert_eq!(drops.get(), length);

            let drops = Rc::new(Cell::new(0));
            let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
            let items = (0..length)
                .map(|_| DynamicValue::from_rust(item.clone(), Probe(drops.clone())).unwrap())
                .collect();
            let mut value =
                DynamicValue::sequence(DynamicLayout::sequence(Type::named("Items"), item), items)
                    .unwrap();
            RilsVec::<(), _>::with_storage(value.view_mut())
                .truncate(keep)
                .unwrap();
            assert_eq!(value.sequence_len().unwrap(), length.min(keep));
            assert_eq!(drops.get(), length.saturating_sub(keep));
            RilsVec::<(), _>::with_storage(value.view_mut())
                .clear()
                .unwrap();
            assert_eq!(drops.get(), length);
            drop(value);
            assert_eq!(drops.get(), length);
        }
    }
}

#[test]
fn rejected_mutations_preserve_values_and_borrow_leases() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let mut value = DynamicValue::sequence(
        DynamicLayout::sequence(Type::named("Items"), item.clone()),
        vec![DynamicValue::from_rust(item, 42).unwrap()],
    )
    .unwrap();
    let ledger = value.sequence_borrows().unwrap();
    let reference = ledger.reference(0).unwrap();
    assert!(
        RilsVec::<(), _>::with_storage(value.view_mut())
            .clear()
            .is_err()
    );
    assert!(
        RilsVec::<(), _>::with_storage(value.view_mut())
            .truncate(0)
            .is_err()
    );
    assert_eq!(value.sequence_len().unwrap(), 1);
    drop(reference);
    let iterator = ledger.begin_iteration().unwrap();
    assert!(
        RilsVec::<(), _>::with_storage(value.view_mut())
            .clear()
            .is_err()
    );
    assert!(
        RilsVec::<(), _>::with_storage(value.view_mut())
            .truncate(0)
            .is_err()
    );
    assert_eq!(value.sequence_len().unwrap(), 1);
    drop(iterator);
    RilsVec::<(), _>::with_storage(value.view_mut())
        .clear()
        .unwrap();
    assert_eq!(value.sequence_len().unwrap(), 0);
}
