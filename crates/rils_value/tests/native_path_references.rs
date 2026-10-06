use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep as Step, DynamicValue};

fn nested() -> (DynamicValue, Rc<DynamicLayout>) {
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let inner = DynamicLayout::record(Type::named("Inner"), vec![("count".into(), number.clone())])
        .unwrap();
    let sequence = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::named("Inner")],
        },
        inner.clone(),
    );
    let optional = DynamicLayout::option(sequence.clone()).unwrap();
    let outer = DynamicLayout::record(
        Type::named("Outer"),
        vec![("items".into(), optional.clone())],
    )
    .unwrap();
    let item = DynamicValue::record(
        inner,
        vec![DynamicValue::from_rust(number.clone(), 1i32).unwrap()],
    )
    .unwrap();
    let items = DynamicValue::sequence(sequence, vec![item]).unwrap();
    let items = DynamicValue::some(optional, items).unwrap();
    (DynamicValue::record(outer, vec![items]).unwrap(), number)
}

#[test]
fn several_mutable_paths_write_the_same_native_bytes_and_release_their_leases() {
    let (mut value, number) = nested();
    let path = [Step::Field(0), Step::Some, Step::Index(0), Step::Field(0)];
    let first = value.reference_path(&path).unwrap();
    let second = value.reference_path(&path).unwrap();
    assert!(value.has_path_references());
    assert!(value.take_path_field(&path).is_err());
    value
        .replace_path_reference(
            &path,
            DynamicValue::from_rust(number.clone(), 42i32).unwrap(),
        )
        .unwrap();
    assert_eq!(value.with_path::<i32, _>(&path, |value| *value), Ok(42));
    let sequence = value
        .view()
        .field(0)
        .unwrap()
        .option_item()
        .unwrap()
        .sequence_borrows()
        .unwrap();
    assert!(sequence.check_structural_mutation().is_err());
    assert!(
        value
            .replace_path_field(&path, DynamicValue::from_rust(number, 0i32).unwrap())
            .is_err()
    );
    let parent = [Step::Field(0), Step::Some, Step::Index(0)];
    let replacement = value.view_path(&parent).unwrap().copy_owned().unwrap();
    assert!(value.replace_path_reference(&parent, replacement).is_err());
    drop(first);
    assert!(value.has_path_references());
    drop(second);
    assert!(!value.has_path_references());
    assert!(sequence.check_structural_mutation().is_ok());
    let moved = value.take_path_field(&path).unwrap();
    assert_eq!(
        moved
            .into_rust::<i32>()
            .unwrap_or_else(|_| panic!("i32 layout")),
        42
    );
    assert!(value.view_path(&path).is_err());
}

#[test]
fn rejected_native_paths_leave_storage_and_borrow_counts_unchanged() {
    let (mut value, _) = nested();
    let invalid_paths = [
        vec![Step::Field(1)],
        vec![Step::Field(0), Step::Variant(0)],
        vec![Step::Field(0), Step::Some, Step::Index(1)],
        vec![Step::Field(0), Step::Some, Step::Index(0), Step::Field(1)],
    ];
    for path in invalid_paths {
        assert!(value.reference_path(&path).is_err());
        assert!(!value.has_path_references());
    }
    let path = [Step::Field(0), Step::Some, Step::Index(0), Step::Field(0)];
    let wrong = DynamicValue::from_rust(DynamicLayout::copy_of::<bool>(Type::Bool), true).unwrap();
    assert!(value.replace_path_reference(&path, wrong).is_err());
    assert_eq!(value.with_path::<i32, _>(&path, |value| *value), Ok(1));
}

#[test]
fn referenced_non_copy_field_replacement_drops_each_owner_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let layout =
        DynamicLayout::record(Type::named("Owner"), vec![("item".into(), item.clone())]).unwrap();
    let mut value = DynamicValue::record(
        layout,
        vec![DynamicValue::from_rust(item.clone(), Probe(drops.clone())).unwrap()],
    )
    .unwrap();
    let lease = value.reference_path(&[Step::Field(0)]).unwrap();
    let previous = value
        .replace_path_reference(
            &[Step::Field(0)],
            DynamicValue::from_rust(item, Probe(drops.clone())).unwrap(),
        )
        .unwrap();
    assert_eq!(drops.get(), 0);
    drop(previous);
    assert_eq!(drops.get(), 1);
    drop(lease);
    let moved = value.take_path_field(&[Step::Field(0)]).unwrap();
    drop(value);
    assert_eq!(drops.get(), 1);
    drop(moved);
    assert_eq!(drops.get(), 2);
}
