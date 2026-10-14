use rils_stdlib::native_collections::vector;
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
    for borrowed in [false, true] {
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
        let mut destination = DynamicValue::sequence(layout.clone(), vec![make()]).unwrap();
        let source = DynamicValue::sequence(layout.clone(), vec![make(), make()]).unwrap();
        let lease = borrowed.then(|| {
            destination
                .sequence_borrows()
                .unwrap()
                .begin_iteration()
                .unwrap()
        });
        let result = vector::extend(destination.view_mut(), source);
        assert_eq!(result.is_err(), borrowed);
        assert_eq!(
            destination.sequence_len().unwrap(),
            if borrowed { 1 } else { 3 }
        );
        assert_eq!(drops.get(), if borrowed { 2 } else { 0 });
        drop(lease);
        drop(destination);
        assert_eq!(drops.get(), 3);
    }
}

#[test]
fn contains_borrows_elements_and_propagates_comparison_errors() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let sequence = DynamicValue::sequence(
        DynamicLayout::sequence(Type::named("Vec"), item.clone()),
        vec![
            DynamicValue::from_rust(item.clone(), 1i32).unwrap(),
            DynamicValue::from_rust(item, 2i32).unwrap(),
        ],
    )
    .unwrap();
    let mut count = 0;
    assert!(
        vector::contains(sequence.view(), |view| {
            count += 1;
            view.with_rust::<i32, _>(|value| *value == 2)
        })
        .unwrap()
    );
    assert_eq!(count, 2);
    assert!(vector::contains(sequence.view(), |_| Err("comparison rejected".into())).is_err());
    assert_eq!(sequence.sequence_len().unwrap(), 2);
}
