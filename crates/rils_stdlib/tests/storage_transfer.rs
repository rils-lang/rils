use rils_stdlib::{Type, stdlib::collections::vector::Vec as RilsVec};
use rils_value::{DynamicLayout, DynamicValue};
use std::{cell::Cell, rc::Rc};

#[repr(align(64))]
struct Probe {
    id: usize,
    drops: Rc<Cell<usize>>,
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn item(layout: &Rc<DynamicLayout>, drops: &Rc<Cell<usize>>, id: usize) -> DynamicValue {
    DynamicValue::from_rust(
        layout.clone(),
        Probe {
            id,
            drops: drops.clone(),
        },
    )
    .unwrap()
}
fn ids(value: &DynamicValue) -> Vec<usize> {
    let view = value.view();
    let indexed = view.indexed().unwrap();
    (0..indexed.len())
        .map(|i| {
            indexed
                .item(i)
                .unwrap()
                .with_rust::<Probe, _>(|p| p.id)
                .unwrap()
        })
        .collect()
}
fn take_id(value: DynamicValue) -> usize {
    value.with::<Probe, _>(|p| p.id).unwrap()
}

#[test]
fn owned_transfers_preserve_order_and_destroy_noncopy_payloads_once() {
    for length in [0usize, 1, 4] {
        let drops = Rc::new(Cell::new(0));
        let layout = DynamicLayout::of::<Probe>(Type::named("Probe"));
        let mut value = DynamicValue::sequence(
            DynamicLayout::sequence(Type::named("Items"), layout.clone()),
            (0..length).map(|id| item(&layout, &drops, id)).collect(),
        )
        .unwrap();
        let mut model: RilsVec<usize> = (0..length).collect::<Vec<_>>().into();
        RilsVec::<DynamicValue, _>::with_storage(value.view_mut())
            .push(item(&layout, &drops, length))
            .unwrap();
        model.push(length);
        RilsVec::<DynamicValue, _>::with_storage(value.view_mut())
            .insert(0, item(&layout, &drops, length + 1))
            .unwrap();
        model.insert(0, length + 1);
        assert_eq!(ids(&value), model.as_slice());
        assert_eq!(drops.get(), 0);
        assert_eq!(
            take_id(
                RilsVec::<DynamicValue, _>::with_storage(value.view_mut())
                    .remove(0)
                    .unwrap()
            ),
            model.remove(0)
        );
        assert_eq!(ids(&value), model.as_slice());
        assert_eq!(
            take_id(
                RilsVec::<DynamicValue, _>::with_storage(value.view_mut())
                    .swap_remove(0)
                    .unwrap()
            ),
            model.swap_remove(0)
        );
        assert_eq!(ids(&value), model.as_slice());
        while let rils_stdlib::stdlib::option::Option::Some(expected) = model.pop() {
            assert_eq!(
                take_id(
                    RilsVec::<DynamicValue, _>::with_storage(value.view_mut())
                        .pop()
                        .unwrap()
                        .unwrap()
                ),
                expected
            );
        }
        for _ in 0..2 {
            assert!(
                RilsVec::<DynamicValue, _>::with_storage(value.view_mut())
                    .pop()
                    .unwrap()
                    .is_none()
            );
        }
        drop(value);
        assert_eq!(drops.get(), length + 2);
    }
}

#[test]
fn rejected_transfers_preserve_storage_and_drop_consumed_inputs_once() {
    for iterator in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let layout = DynamicLayout::of::<Probe>(Type::named("Probe"));
        let mut value = DynamicValue::sequence(
            DynamicLayout::sequence(Type::named("Items"), layout.clone()),
            vec![item(&layout, &drops, 0)],
        )
        .unwrap();
        let ledger = value.sequence_borrows().unwrap();
        let reference = (!iterator).then(|| ledger.reference(0).unwrap());
        let iteration = iterator.then(|| ledger.begin_iteration().unwrap());
        let mut receiver = RilsVec::<DynamicValue, _>::with_storage(value.view_mut());
        assert!(receiver.push(item(&layout, &drops, 1)).is_err());
        assert!(receiver.insert(0, item(&layout, &drops, 2)).is_err());
        assert!(receiver.pop().is_err());
        assert!(receiver.remove(0).is_err());
        assert!(receiver.swap_remove(0).is_err());
        drop(receiver);
        assert_eq!(ids(&value), [0]);
        assert_eq!(drops.get(), 2);
        drop(reference);
        drop(iteration);
        let mut receiver = RilsVec::<DynamicValue, _>::with_storage(value.view_mut());
        assert!(receiver.insert(2, item(&layout, &drops, 3)).is_err());
        assert!(receiver.remove(1).is_err());
        assert!(receiver.swap_remove(1).is_err());
        let wrong = DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), 7).unwrap();
        assert!(receiver.push(wrong).is_err());
        drop(receiver);
        assert_eq!(ids(&value), [0]);
        assert_eq!(drops.get(), 3);
        drop(value);
        assert_eq!(drops.get(), 4);
    }
}

#[test]
fn zero_sized_elements_transfer_without_losing_count() {
    let layout = DynamicLayout::copy_of::<()>(Type::Unit);
    let mut value = DynamicValue::sequence(
        DynamicLayout::sequence(Type::named("Items"), layout.clone()),
        vec![],
    )
    .unwrap();
    let mut receiver = RilsVec::<DynamicValue, _>::with_storage(value.view_mut());
    receiver
        .push(DynamicValue::from_rust(layout.clone(), ()).unwrap())
        .unwrap();
    receiver
        .insert(0, DynamicValue::from_rust(layout, ()).unwrap())
        .unwrap();
    receiver.remove(0).unwrap().with::<(), _>(|_| ()).unwrap();
    receiver
        .pop()
        .unwrap()
        .unwrap()
        .with::<(), _>(|_| ())
        .unwrap();
    assert!(receiver.pop().unwrap().is_none());
}
