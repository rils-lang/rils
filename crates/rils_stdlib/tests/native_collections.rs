use std::{cell::Cell, rc::Rc};

use rils_native::NativeKey;
use rils_stdlib::native_collections::{map, set};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue};

fn integer(value: i32) -> DynamicValue {
    DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), value).unwrap()
}

fn collection(items: &[i32]) -> DynamicValue {
    DynamicValue::sequence(
        DynamicLayout::sequence(
            Type::named("TestSet"),
            DynamicLayout::copy_of::<i32>(Type::I32),
        ),
        items.iter().copied().map(integer).collect(),
    )
    .unwrap()
}

fn values(value: &DynamicValue) -> Vec<NativeKey> {
    (0..value.sequence_len().unwrap())
        .map(|index| {
            value
                .with_sequence_item(index, |item| {
                    rils_stdlib::native::registry().key(item.view()).unwrap()
                })
                .unwrap()
        })
        .collect()
}

#[test]
fn set_insert_remove_order_and_active_borrows() {
    for ordered in [false, true] {
        let mut value = collection(&[]);
        for number in [3, 1, 2] {
            assert!(
                set::insert(
                    value.view_mut(),
                    integer(number),
                    &NativeKey::Signed(number.into()),
                    ordered
                )
                .unwrap()
            );
        }
        let expected = if ordered { [1, 2, 3] } else { [3, 1, 2] };
        assert_eq!(values(&value), expected.map(NativeKey::Signed));
        let lease = value.sequence_borrows().unwrap().begin_iteration().unwrap();
        assert!(
            !set::insert(value.view_mut(), integer(1), &NativeKey::Signed(1), ordered).unwrap()
        );
        assert!(set::insert(value.view_mut(), integer(4), &NativeKey::Signed(4), ordered).is_err());
        assert!(set::remove(value.view_mut(), &NativeKey::Signed(2)).is_err());
        assert!(!set::remove(value.view_mut(), &NativeKey::Signed(9)).unwrap());
        drop(lease);
        assert!(set::remove(value.view_mut(), &NativeKey::Signed(2)).unwrap());
        assert!(!set::remove(value.view_mut(), &NativeKey::Signed(2)).unwrap());
    }
}

#[test]
fn set_algebra_clones_only_selected_values_and_preserves_inputs_on_failure() {
    let left = collection(&[1, 2]);
    let right = collection(&[2, 3]);
    for (method, expected) in [
        ("union", vec![1, 2, 3]),
        ("intersection", vec![2]),
        ("difference", vec![1]),
        ("symmetric_difference", vec![1, 3]),
    ] {
        let mut copies = 0;
        let result = set::algebra(&left.view(), &right.view(), method, &mut |view| {
            copies += 1;
            let NativeKey::Signed(value) = rils_stdlib::native::registry().key(view)? else {
                unreachable!()
            };
            Ok(integer(value as i32))
        })
        .unwrap();
        assert_eq!(copies, expected.len());
        assert_eq!(
            result
                .iter()
                .map(|item| rils_stdlib::native::registry().key(item.view()).unwrap())
                .collect::<Vec<_>>(),
            expected
                .into_iter()
                .map(NativeKey::Signed)
                .collect::<Vec<_>>()
        );
    }
    assert!(set::relation(&left.view(), &left.view(), "is_subset").unwrap());
    assert!(!set::relation(&left.view(), &right.view(), "is_superset").unwrap());
    assert!(set::relation(&left.view(), &collection(&[]).view(), "is_disjoint").unwrap());
    assert!(
        set::algebra(&left.view(), &right.view(), "union", &mut |_| Err(
            "clone rejected".into()
        ))
        .is_err()
    );
    assert_eq!(
        values(&left),
        vec![NativeKey::Signed(1), NativeKey::Signed(2)]
    );
    assert_eq!(
        values(&right),
        vec![NativeKey::Signed(2), NativeKey::Signed(3)]
    );
}

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn maps_move_non_clone_values_and_return_replaced_payloads() {
    for ordered in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let payload = DynamicLayout::of::<Probe>(Type::named("Probe"));
        let pair = DynamicLayout::aggregate(
            Type::Tuple(vec![Type::I32, Type::named("Probe")]),
            vec![
                ("0".into(), DynamicLayout::copy_of::<i32>(Type::I32)),
                ("1".into(), payload.clone()),
            ],
        )
        .unwrap();
        let entry = |number| {
            DynamicValue::record(
                pair.clone(),
                vec![
                    integer(number),
                    DynamicValue::from_rust(payload.clone(), Probe(drops.clone())).unwrap(),
                ],
            )
            .unwrap()
        };
        let mut value = DynamicValue::sequence(
            DynamicLayout::sequence(Type::named("TestMap"), pair.clone()),
            vec![],
        )
        .unwrap();
        for number in [3, 1, 2] {
            assert!(
                map::insert(
                    value.view_mut(),
                    entry(number),
                    &NativeKey::Signed(number.into()),
                    ordered
                )
                .unwrap()
                .is_none()
            );
        }
        assert_eq!(drops.get(), 0);
        let previous = map::insert(value.view_mut(), entry(1), &NativeKey::Signed(1), ordered)
            .unwrap()
            .unwrap();
        assert_eq!(drops.get(), 0);
        drop(previous);
        assert_eq!(drops.get(), 1);
        let lease = value.sequence_borrows().unwrap().begin_iteration().unwrap();
        assert!(map::insert(value.view_mut(), entry(1), &NativeKey::Signed(1), ordered).is_err());
        assert_eq!(drops.get(), 2);
        assert!(map::remove(value.view_mut(), &NativeKey::Signed(1)).is_err());
        drop(lease);
        let removed = map::remove(value.view_mut(), &NativeKey::Signed(1))
            .unwrap()
            .unwrap();
        assert_eq!(drops.get(), 2);
        drop(removed);
        assert_eq!(drops.get(), 3);
        drop(value);
        assert_eq!(drops.get(), 5);
    }
}
