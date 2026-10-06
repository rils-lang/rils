use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep as Step, DynamicValue};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn nested(drops: &Rc<Cell<usize>>) -> (DynamicValue, Rc<DynamicLayout>, Vec<Step>) {
    let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let sequence = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::named("Probe")],
        },
        item.clone(),
    );
    let optional = DynamicLayout::option(sequence.clone()).unwrap();
    let sum = DynamicLayout::variant(
        Type::Result(Box::new(optional.rils_type().clone()), Box::new(Type::Unit)),
        vec![optional.clone(), DynamicLayout::copy_of::<()>(Type::Unit)],
    )
    .unwrap();
    let outer = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![sum.rils_type().clone()],
        },
        sum.clone(),
    );
    let record =
        DynamicLayout::record(Type::named("Holder"), vec![("items".into(), outer.clone())])
            .unwrap();
    let payload = DynamicValue::from_rust(item.clone(), Probe(drops.clone())).unwrap();
    let items = DynamicValue::sequence(sequence, vec![payload]).unwrap();
    let optional = DynamicValue::some(optional, items).unwrap();
    let sum = DynamicValue::variant(sum, 0, optional).unwrap();
    let outer = DynamicValue::sequence(outer, vec![sum]).unwrap();
    (
        DynamicValue::record(record, vec![outer]).unwrap(),
        item,
        vec![Step::Field(0), Step::Index(0), Step::Variant(0), Step::Some],
    )
}

#[test]
fn projected_mutation_keeps_ownership_in_original_bytes_on_error_and_unwind() {
    let drops = Rc::new(Cell::new(0));
    let (mut value, item, path) = nested(&drops);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut view = value.view_path_mut(&path).unwrap();
        view.push_sequence_item(DynamicValue::from_rust(item, Probe(drops.clone())).unwrap())
            .unwrap();
        assert!(view.take_sequence_item(9).is_err());
        panic!("native callback failed");
    }));
    assert!(result.is_err());
    assert_eq!(value.view_path(&path).unwrap().sequence_len(), Ok(2));
    assert_eq!(drops.get(), 0);
    let removed = value
        .view_path_mut(&path)
        .unwrap()
        .take_sequence_item(0)
        .unwrap();
    drop(value);
    assert_eq!(drops.get(), 1);
    drop(removed);
    assert_eq!(drops.get(), 2);
}

#[test]
fn projected_mutation_validates_layout_and_preserves_descendant_leases() {
    let drops = Rc::new(Cell::new(0));
    let (mut value, item, path) = nested(&drops);
    let mut element = path.clone();
    element.push(Step::Index(0));
    let first = value.reference_path(&element).unwrap();
    let second = value.reference_path(&element).unwrap();
    let mut view = value.view_path_mut(&path).unwrap();
    assert!(view.clear_sequence().is_err());
    assert!(
        view.push_sequence_item(
            DynamicValue::from_rust(item.clone(), Probe(drops.clone())).unwrap()
        )
        .is_err()
    );
    let wrong = DynamicValue::from_rust(DynamicLayout::copy_of::<bool>(Type::Bool), true).unwrap();
    assert!(view.replace_sequence_item(0, wrong).is_err());
    assert_eq!(view.sequence_len(), Ok(1));
    assert_eq!(drops.get(), 1);
    drop(first);
    drop(second);
    value
        .view_path_mut(&path)
        .unwrap()
        .clear_sequence()
        .unwrap();
    assert_eq!(drops.get(), 2);
    drop(value);
    assert_eq!(drops.get(), 2);
}

#[test]
fn mutable_views_reject_invalid_and_moved_projections() {
    let drops = Rc::new(Cell::new(0));
    let (mut value, _, _) = nested(&drops);
    for path in [
        vec![Step::Field(1)],
        vec![Step::Field(0), Step::Index(1)],
        vec![Step::Field(0), Step::Index(0), Step::Variant(1)],
    ] {
        assert!(value.view_path_mut(&path).is_err());
    }
    let items = value.take_field(0).unwrap();
    assert!(value.view_path_mut(&[Step::Field(0)]).is_err());
    assert_eq!(drops.get(), 0);
    drop(items);
    assert_eq!(drops.get(), 1);
    drop(value);
    assert_eq!(drops.get(), 1);
}
