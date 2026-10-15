use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicPathStep, DynamicValue, OwnedElements};
use std::{cell::Cell, rc::Rc};

#[repr(align(64))]
struct Probe(Rc<Cell<usize>>, usize);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn cursors_keep_payloads_in_place_until_yield_and_drop_only_remaining_items() {
    for array in [false, true] {
        for length in [0, 1, 4] {
            let drops = Rc::new(Cell::new(0));
            let item = DynamicLayout::of::<Probe>(Type::named("Probe"));
            let values = (0..length)
                .map(|i| DynamicValue::from_rust(item.clone(), Probe(drops.clone(), i)).unwrap())
                .collect();
            let mut value = if array {
                DynamicValue::record(DynamicLayout::array(item.clone(), length).unwrap(), values)
                    .unwrap()
            } else {
                DynamicValue::sequence(
                    DynamicLayout::sequence(Type::named("Items"), item.clone()),
                    values,
                )
                .unwrap()
            };
            let addresses = (0..length)
                .map(|i| {
                    value
                        .view()
                        .indexed()
                        .unwrap()
                        .item(i)
                        .unwrap()
                        .with_rust::<Probe, _>(|p| p as *const Probe as usize)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let mut cursor = if array {
                OwnedElements::from_array(value, item.clone()).unwrap()
            } else {
                let cursor = value.drain_sequence().unwrap();
                assert_eq!(value.sequence_len().unwrap(), 0);
                cursor
            };
            assert!(cursor.item_layout().compatible_with(&item));
            assert_eq!(cursor.len(), length);
            let mut i = 0;
            cursor
                .any_remaining(|view| {
                    view.with_rust::<Probe, _>(|p| {
                        assert_eq!(p.1, i);
                        assert_eq!(p as *const Probe as usize, addresses[i]);
                    })?;
                    i += 1;
                    Ok(false)
                })
                .unwrap();
            assert_eq!(i, length);
            let first = cursor.next().transpose().unwrap();
            assert_eq!(drops.get(), 0);
            assert_eq!(cursor.len(), length.saturating_sub(1));
            let mut remaining = 0;
            cursor
                .any_remaining(|view| {
                    assert!(!view.is_partially_moved()?);
                    view.with_rust::<Probe, _>(|p| assert_eq!(p.1, remaining + 1))?;
                    remaining += 1;
                    Ok(false)
                })
                .unwrap();
            assert_eq!(remaining, length.saturating_sub(1));
            drop(cursor);
            assert_eq!(drops.get(), length.saturating_sub(1));
            drop(first);
            assert_eq!(drops.get(), length);
        }
    }
}

#[test]
fn rejected_drain_preserves_sequence_and_element_leases() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let mut value = DynamicValue::sequence(
        DynamicLayout::sequence(Type::named("Items"), item.clone()),
        vec![DynamicValue::from_rust(item, 42).unwrap()],
    )
    .unwrap();
    let ledger = value.sequence_borrows().unwrap();
    let lease = ledger.reference(0).unwrap();
    assert!(value.drain_sequence().is_err());
    assert_eq!(value.sequence_len().unwrap(), 1);
    drop(lease);
    let iterator_lease = ledger.begin_iteration().unwrap();
    assert!(value.drain_sequence().is_err());
    drop(iterator_lease);
    let mut cursor = value.drain_sequence().unwrap();
    assert_eq!(
        cursor
            .next()
            .unwrap()
            .unwrap()
            .with::<i32, _>(|v| *v)
            .unwrap(),
        42
    );
    assert!(cursor.next().is_none());
    assert!(cursor.next().is_none());
}

#[test]
fn drain_rejects_partially_moved_items_before_transferring_storage() {
    let item = DynamicLayout::aggregate(
        Type::Tuple(vec![Type::I32]),
        vec![("0".into(), DynamicLayout::copy_of::<i32>(Type::I32))],
    )
    .unwrap();
    let payload = DynamicValue::record(
        item.clone(),
        vec![DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), 7).unwrap()],
    )
    .unwrap();
    let mut sequence = DynamicValue::sequence(
        DynamicLayout::sequence(Type::named("Items"), item),
        vec![payload],
    )
    .unwrap();
    drop(
        sequence
            .take_path_field(&[DynamicPathStep::Index(0), DynamicPathStep::Field(0)])
            .unwrap(),
    );
    assert!(sequence.drain_sequence().is_err());
    assert_eq!(sequence.sequence_len().unwrap(), 1);
}

#[test]
fn zero_sized_elements_preserve_count_and_item_layout() {
    let item = DynamicLayout::copy_of::<()>(Type::Unit);
    let values = (0..3).map(|_| DynamicValue::from_rust(item.clone(), ()).unwrap());
    let mut cursor = OwnedElements::from_values(item.clone(), values).unwrap();
    assert_eq!(cursor.len(), 3);
    assert!(
        cursor
            .by_ref()
            .all(|item| item.unwrap().with::<(), _>(|_| ()).is_ok())
    );
    assert_eq!(cursor.len(), 0);
    assert!(cursor.next().is_none());
}
