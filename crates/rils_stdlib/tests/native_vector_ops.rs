use rils_stdlib::stdlib::collections::vector::Vec as RilsVec;
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
fn extend_moves_non_clone_items_and_checks_destination_before_mutation() {
    for (destination_length, source_length, borrowed, source_borrowed) in (0..3).flat_map(|a| {
        (0..3).flat_map(move |b| {
            [false, true]
                .into_iter()
                .flat_map(move |c| [false, true].into_iter().map(move |d| (a, b, c, d)))
        })
    }) {
        let drops = Rc::new(Cell::new(0));
        let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
        let layout = DynamicLayout::sequence(
            Type::Named {
                name: "Vec".into(),
                arguments: vec![Type::named("Probe")],
            },
            item.clone(),
        );
        let make = || DynamicValue::from_rust(item.clone(), Probe(drops.clone())).unwrap();
        let mut destination = DynamicValue::sequence(
            layout.clone(),
            (0..destination_length).map(|_| make()).collect(),
        )
        .unwrap();
        let mut source =
            DynamicValue::sequence(layout.clone(), (0..source_length).map(|_| make()).collect())
                .unwrap();
        let lease = borrowed.then(|| {
            destination
                .sequence_borrows()
                .unwrap()
                .begin_iteration()
                .unwrap()
        });
        let source_lease = source_borrowed.then(|| {
            source
                .sequence_borrows()
                .unwrap()
                .begin_iteration()
                .unwrap()
        });
        let rejected = borrowed || source_borrowed;
        let result = RilsVec::<Probe, _>::with_storage(destination.view_mut())
            .extend(RilsVec::with_storage(source.view_mut()));
        assert_eq!(
            source.sequence_len().unwrap(),
            if rejected { source_length } else { 0 }
        );
        drop(source_lease);
        drop(source);
        assert_eq!(result.is_err(), rejected);
        assert_eq!(
            destination.sequence_len().unwrap(),
            if rejected {
                destination_length
            } else {
                destination_length + source_length
            }
        );
        assert_eq!(drops.get(), if rejected { source_length } else { 0 });
        drop(lease);
        drop(destination);
        assert_eq!(drops.get(), destination_length + source_length);
    }
}

#[test]
fn different_layouts_are_rejected_before_either_sequence_changes() {
    let left = DynamicLayout::copy_of::<i32>(Type::I32);
    let right = DynamicLayout::copy_of::<u32>(Type::named("u32"));
    let mut destination = DynamicValue::sequence(
        DynamicLayout::sequence(Type::named("Left"), left.clone()),
        vec![DynamicValue::from_rust(left, 1i32).unwrap()],
    )
    .unwrap();
    let mut source = DynamicValue::sequence(
        DynamicLayout::sequence(Type::named("Right"), right.clone()),
        vec![DynamicValue::from_rust(right, 2u32).unwrap()],
    )
    .unwrap();
    assert!(
        RilsVec::<(), _>::with_storage(destination.view_mut())
            .extend(RilsVec::with_storage(source.view_mut()))
            .is_err()
    );
    assert_eq!(destination.sequence_len().unwrap(), 1);
    assert_eq!(source.sequence_len().unwrap(), 1);
}
