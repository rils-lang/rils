use std::{cell::Cell, rc::Rc};

use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue};

struct Probe(Rc<Cell<usize>>);

impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn root(index: Option<usize>, drops: &Rc<Cell<usize>>) -> DynamicValue {
    let leaf = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let item = DynamicValue::from_rust(leaf.clone(), Probe(drops.clone())).unwrap();
    match index {
        None => DynamicValue::some(DynamicLayout::option(leaf).unwrap(), item).unwrap(),
        Some(index) => DynamicValue::variant(
            DynamicLayout::variant(
                Type::Result(
                    Box::new(Type::named("Probe")),
                    Box::new(Type::named("Probe")),
                ),
                vec![leaf.clone(), leaf],
            )
            .unwrap(),
            index,
            item,
        )
        .unwrap(),
    }
}

#[test]
fn owned_sum_transfer_rejects_leases_and_moved_reads_and_drops_once() {
    for index in [None, Some(0), Some(1)] {
        let drops = Rc::new(Cell::new(0));
        let mut value = root(index, &drops);
        let step = index.map_or(DynamicPathStep::Some, DynamicPathStep::Variant);
        let lease = value.reference_path(&[step]).unwrap();
        assert!(value.take_active_payload().is_err());
        assert_eq!(drops.get(), 0);
        assert!(value.view_path(&[step]).is_ok());
        drop(lease);
        let (active, payload) = value.take_active_payload().unwrap().unwrap();
        assert_eq!(active, index.unwrap_or(0));
        assert!(
            payload
                .with::<Probe, _>(|probe| Rc::ptr_eq(&probe.0, &drops))
                .unwrap()
        );
        assert!(value.take_active_payload().is_err());
        assert!(value.view().layout().is_err());
        assert!(value.view_path(&[step]).is_err());
        assert!(value.copy_owned().is_err());
        assert!(value.is_some().is_err());
        assert!(value.variant_index().is_err());
        drop(value);
        assert_eq!(drops.get(), 0);
        drop(payload);
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn moved_sum_cannot_be_reintroduced_as_an_initialized_child() {
    for kind in 0..5 {
        let drops = Rc::new(Cell::new(0));
        let mut value = root(Some(0), &drops);
        let layout = value.layout_handle();
        let payload = value.take_active_payload().unwrap();
        let result = match kind {
            0 => DynamicValue::some(DynamicLayout::option(layout).unwrap(), value),
            1 => DynamicValue::variant(
                DynamicLayout::variant(Type::named("Outer"), vec![layout]).unwrap(),
                0,
                value,
            ),
            2 => DynamicValue::record(
                DynamicLayout::record(Type::named("Outer"), vec![("item".into(), layout)]).unwrap(),
                vec![value],
            ),
            3 => DynamicValue::sequence(
                DynamicLayout::sequence(Type::named("Items"), layout),
                vec![value],
            ),
            _ => {
                let mut outer = DynamicValue::sequence(
                    DynamicLayout::sequence(Type::named("Items"), layout),
                    vec![],
                )
                .unwrap();
                assert!(outer.push_sequence_item(value).is_err());
                assert_eq!(outer.sequence_len().unwrap(), 0);
                drop(payload);
                assert_eq!(drops.get(), 1);
                continue;
            }
        };
        assert!(result.is_err());
        assert_eq!(drops.get(), 0);
        drop(payload);
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn empty_option_transfer_has_no_child_and_cannot_be_repeated() {
    let leaf = DynamicLayout::of::<Probe>(Type::named("Probe"));
    let mut value = DynamicValue::none(DynamicLayout::option(leaf).unwrap()).unwrap();
    assert!(value.take_active_payload().unwrap().is_none());
    assert!(value.take_active_payload().is_err());
    assert!(value.is_some().is_err());
}
