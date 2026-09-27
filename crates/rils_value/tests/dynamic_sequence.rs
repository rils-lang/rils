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

#[test]
fn nested_field_moves_through_sequence_variant_and_option() {
    let number = DynamicLayout::copy_of::<i32>(Type::I32);
    let inner_type = Type::named("Inner");
    let inner = DynamicLayout::record(
        inner_type.clone(),
        vec![
            ("value".into(), number.clone()),
            ("sibling".into(), number.clone()),
        ],
    )
    .unwrap();
    let optional = DynamicLayout::option(inner.clone()).unwrap();
    let result_type = Type::Result(
        Box::new(Type::Option(Box::new(inner_type))),
        Box::new(Type::Unit),
    );
    let unit = DynamicLayout::copy_of::<()>(Type::Unit);
    let result = DynamicLayout::variant(result_type.clone(), vec![optional.clone(), unit]).unwrap();
    let vector = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![result_type],
        },
        result.clone(),
    );
    let outer = DynamicLayout::record(Type::named("Outer"), vec![("items".into(), vector.clone())])
        .unwrap();
    let child = DynamicValue::record(
        inner,
        vec![
            DynamicValue::from_rust(number.clone(), 7).unwrap(),
            DynamicValue::from_rust(number.clone(), 9).unwrap(),
        ],
    )
    .unwrap();
    let some = DynamicValue::some(optional, child).unwrap();
    let selected = DynamicValue::variant(result, 0, some).unwrap();
    let items = DynamicValue::sequence(vector, vec![selected]).unwrap();
    let mut root = DynamicValue::record(outer, vec![items]).unwrap();
    let mut path = vec![
        DynamicPathStep::Field(0),
        DynamicPathStep::Index(0),
        DynamicPathStep::Variant(0),
        DynamicPathStep::Some,
        DynamicPathStep::Field(0),
    ];
    let moved = root.take_path_field(&path).unwrap();
    assert_eq!(
        moved
            .into_rust::<i32>()
            .unwrap_or_else(|_| panic!("integer moves")),
        7
    );
    assert!(root.with_path::<i32, _>(&path, |n| *n).is_err());
    *path.last_mut().unwrap() = DynamicPathStep::Field(1);
    assert_eq!(root.with_path::<i32, _>(&path, |n| *n), Ok(9));
    *path.last_mut().unwrap() = DynamicPathStep::Field(0);
    root.put_path_field(&path, DynamicValue::from_rust(number, 8).unwrap())
        .unwrap();
    assert_eq!(root.with_path::<i32, _>(&path, |n| *n), Ok(8));
}

#[test]
fn independently_resolved_nested_layouts_accept_the_same_value() {
    let build = || {
        let number = DynamicLayout::copy_of::<i32>(Type::I32);
        let optional = DynamicLayout::option(number.clone()).unwrap();
        let record = DynamicLayout::record(
            Type::named("Inner"),
            vec![("value".into(), optional.clone())],
        )
        .unwrap();
        let sequence = DynamicLayout::sequence(
            Type::Named {
                name: "Vec".into(),
                arguments: vec![Type::named("Inner")],
            },
            record.clone(),
        );
        (number, optional, record, sequence)
    };
    let (number, optional, record, sequence) = build();
    let (_, _, _, equivalent) = build();
    assert!(sequence.compatible_with(&equivalent));
    let item = DynamicValue::record(
        record,
        vec![DynamicValue::some(optional, DynamicValue::from_rust(number, 11).unwrap()).unwrap()],
    )
    .unwrap();
    let value = DynamicValue::sequence(sequence, vec![item]).unwrap();
    let object = rils_value::DynamicObject::<()>::new(
        Rc::new(rils_value::DynamicType::new(equivalent)),
        value,
    )
    .unwrap();
    let path = [
        DynamicPathStep::Index(0),
        DynamicPathStep::Field(0),
        DynamicPathStep::Some,
    ];
    assert_eq!(
        object.with(|value| value.with_path::<i32, _>(&path, |n| *n)),
        Ok(Ok(11))
    );
}
