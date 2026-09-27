use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue};

struct Probe(Rc<Cell<usize>>);

impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn sequence_field_owns_dynamic_children_and_moves_one_without_cloning() {
    let drops = Rc::new(Cell::new(0));
    let probe = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let sequence = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::named("Probe")],
        },
        probe.clone(),
    );
    let record = DynamicLayout::record(
        Type::named("Holder"),
        vec![("items".into(), sequence.clone())],
    )
    .unwrap();
    let items = DynamicValue::sequence(
        sequence,
        vec![
            DynamicValue::from_rust(probe.clone(), Probe(drops.clone())).unwrap(),
            DynamicValue::from_rust(probe, Probe(drops.clone())).unwrap(),
        ],
    )
    .unwrap();
    let mut holder = DynamicValue::record(record, vec![items]).unwrap();
    let mut items = holder.take_field(0).unwrap();
    assert_eq!(items.sequence_len(), Ok(2));
    let first = items.take_sequence_item(0).unwrap();
    assert_eq!(items.sequence_len(), Ok(1));
    holder.put_field(0, items).unwrap();
    drop(holder);
    assert_eq!(drops.get(), 1);
    drop(first);
    assert_eq!(drops.get(), 2);
}

#[test]
fn checked_path_reaches_leaf_across_all_composite_kinds() {
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let unit = DynamicLayout::copy_of::<()>(Type::Unit);
    let optional = DynamicLayout::option(number.clone()).unwrap();
    let result_type = Type::Result(
        Box::new(Type::Option(Box::new(Type::I32))),
        Box::new(Type::Unit),
    );
    let result = DynamicLayout::variant(result_type.clone(), vec![optional.clone(), unit]).unwrap();
    let vector = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![result_type],
        },
        result.clone(),
    );
    let holder = DynamicLayout::record(
        Type::named("Holder"),
        vec![("items".into(), vector.clone())],
    )
    .unwrap();
    let item = DynamicValue::from_rust(number, 5_i32).unwrap();
    let some = DynamicValue::some(optional, item).unwrap();
    let selected = DynamicValue::variant(result, 0, some).unwrap();
    let items = DynamicValue::sequence(vector, vec![selected]).unwrap();
    let mut root = DynamicValue::record(holder, vec![items]).unwrap();
    let path = [
        DynamicPathStep::Field(0),
        DynamicPathStep::Index(0),
        DynamicPathStep::Variant(0),
        DynamicPathStep::Some,
    ];
    assert_eq!(root.with_path::<i32, _>(&path, |n| *n), Ok(5));
    root.with_path_mut::<i32, _>(&path, |n| *n = 8).unwrap();
    assert_eq!(root.with_path::<i32, _>(&path, |n| *n), Ok(8));
    assert!(root.with_path::<i32, _>(&path[..3], |n| *n).is_err());
    assert!(
        root.with_path::<i32, _>(
            &[DynamicPathStep::Field(0), DynamicPathStep::Index(1)],
            |n| *n
        )
        .is_err()
    );
}
