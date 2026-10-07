use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue};

struct Probe(i32, Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.1.set(self.1.get() + 1);
    }
}

#[test]
fn active_sum_payload_replacement_transfers_non_clone_values_and_drops_once() {
    for step in [
        DynamicPathStep::Some,
        DynamicPathStep::Variant(0),
        DynamicPathStep::Variant(1),
    ] {
        let drops = Rc::new(Cell::new(0));
        let leaf = DynamicLayout::of::<Probe>(Type::named("Probe"));
        let make =
            |number| DynamicValue::from_rust(leaf.clone(), Probe(number, drops.clone())).unwrap();
        let mut root = match step {
            DynamicPathStep::Some => {
                DynamicValue::some(DynamicLayout::option(leaf.clone()).unwrap(), make(1)).unwrap()
            }
            DynamicPathStep::Variant(index) => DynamicValue::variant(
                DynamicLayout::variant(
                    Type::Result(
                        Box::new(Type::named("Probe")),
                        Box::new(Type::named("Probe")),
                    ),
                    vec![leaf.clone(), leaf.clone()],
                )
                .unwrap(),
                index,
                make(1),
            )
            .unwrap(),
            _ => unreachable!(),
        };
        let lease = root.reference_path(&[step]).unwrap();
        let second = root.reference_path(&[step]).unwrap();
        assert!(root.replace_path_owned(&[step], make(2)).is_err());
        assert_eq!(drops.get(), 1, "failed replacement drops only its input");
        let old = root
            .replace_path_reference(&[step], make(42))
            .unwrap()
            .unwrap();
        assert_eq!(drops.get(), 1);
        assert_eq!(old.with::<Probe, _>(|value| value.0).unwrap(), 1);
        assert_eq!(
            root.view_path(&[step])
                .unwrap()
                .with_rust::<Probe, _>(|value| value.0)
                .unwrap(),
            42
        );
        drop(old);
        assert_eq!(drops.get(), 2);
        let wrong = DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), 7).unwrap();
        assert!(root.replace_path_reference(&[step], wrong).is_err());
        let inactive = match step {
            DynamicPathStep::Some => DynamicPathStep::Variant(0),
            DynamicPathStep::Variant(index) => DynamicPathStep::Variant(1 - index),
            _ => unreachable!(),
        };
        assert!(root.replace_path_reference(&[inactive], make(99)).is_err());
        assert_eq!(drops.get(), 3);
        assert_eq!(
            root.view_path(&[step])
                .unwrap()
                .with_rust::<Probe, _>(|value| value.0)
                .unwrap(),
            42
        );
        drop(lease);
        drop(second);
        drop(root);
        assert_eq!(drops.get(), 4);
    }
}

#[test]
fn descendants_block_payload_replacement_until_their_lease_ends() {
    let leaf = DynamicLayout::copy_of::<i32>(Type::I32);
    let record =
        DynamicLayout::record(Type::named("Record"), vec![("number".into(), leaf.clone())])
            .unwrap();
    let make = |number| {
        DynamicValue::record(
            record.clone(),
            vec![DynamicValue::from_rust(leaf.clone(), number).unwrap()],
        )
        .unwrap()
    };
    let mut root =
        DynamicValue::some(DynamicLayout::option(record.clone()).unwrap(), make(1)).unwrap();
    let lease = root
        .reference_path(&[DynamicPathStep::Some, DynamicPathStep::Field(0)])
        .unwrap();
    assert!(
        root.replace_path_reference(&[DynamicPathStep::Some], make(2))
            .is_err()
    );
    assert_eq!(
        root.view()
            .option_item()
            .unwrap()
            .field(0)
            .unwrap()
            .with_rust::<i32, _>(|v| *v)
            .unwrap(),
        1
    );
    drop(lease);
    drop(
        root.replace_path_reference(&[DynamicPathStep::Some], make(42))
            .unwrap(),
    );
    assert_eq!(
        root.view()
            .option_item()
            .unwrap()
            .field(0)
            .unwrap()
            .with_rust::<i32, _>(|v| *v)
            .unwrap(),
        42
    );
}
