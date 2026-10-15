use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use rils_native::NativeKey;
use rils_stdlib::storage::{
    EntrySchema, EntryShape, KeyPolicy, MapInsert, MapRemove, MutableAccess, NativeServices,
    OwnedSequence, SetAlgebra, SetInsert, SetRelations, SetRemove, SharedAccess,
};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue, DynamicValueRef};

const SET: EntrySchema = EntrySchema {
    owner: "TestSet",
    parameters: 1,
    entry: EntryShape::Parameter(0),
};
const MAP: EntrySchema = EntrySchema {
    owner: "TestMap",
    parameters: 2,
    entry: EntryShape::Tuple(&[EntryShape::Parameter(0), EntryShape::Parameter(1)]),
};
const REVERSED_MAP: EntrySchema = EntrySchema {
    owner: "TestMap",
    parameters: 2,
    entry: EntryShape::Tuple(&[EntryShape::Parameter(1), EntryShape::Parameter(0)]),
};

#[derive(Default)]
struct Services {
    copies: RefCell<Vec<NativeKey>>,
    reject: bool,
}
impl NativeServices for Services {
    fn implements(&self, _: &Type, _: &str) -> bool {
        false
    }
    fn clone_value(&self, value: DynamicValueRef<'_>) -> Result<DynamicValue, String> {
        if self.reject {
            return Err("clone rejected".into());
        }
        self.copies
            .borrow_mut()
            .push(rils_stdlib::native::registry().key(value.clone())?);
        value.copy_owned()
    }
}

fn integer(value: i32) -> DynamicValue {
    DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), value).unwrap()
}

fn collection(items: &[i32]) -> DynamicValue {
    DynamicValue::sequence(
        DynamicLayout::sequence(
            Type::Named {
                name: SET.owner.into(),
                arguments: vec![Type::I32],
            },
            DynamicLayout::copy_of::<i32>(Type::I32),
        ),
        items.iter().copied().map(integer).collect(),
    )
    .unwrap()
}

fn values(value: &DynamicValue) -> Vec<NativeKey> {
    (0..value.sequence_len().unwrap())
        .map(|index| {
            rils_stdlib::native::registry()
                .key(value.view().sequence_item(index).unwrap())
                .unwrap()
        })
        .collect()
}

#[test]
fn set_insert_remove_order_and_active_borrows() {
    let services = Services::default();
    for policy in [KeyPolicy::Hash, KeyPolicy::Ordered] {
        let mut value = collection(&[]);
        for number in [3, 1, 2] {
            let mut storage = MutableAccess::new(value.view_mut(), &SET, &services).unwrap();
            assert!(SetInsert::insert(&mut storage, integer(number), policy).unwrap());
        }
        let expected = if matches!(policy, KeyPolicy::Ordered) {
            [1, 2, 3]
        } else {
            [3, 1, 2]
        };
        assert_eq!(values(&value), expected.map(NativeKey::Signed));
        let lease = value.sequence_borrows().unwrap().begin_iteration().unwrap();
        {
            let mut storage = MutableAccess::new(value.view_mut(), &SET, &services).unwrap();
            assert!(!SetInsert::insert(&mut storage, integer(1), policy).unwrap());
            assert!(SetInsert::insert(&mut storage, integer(4), policy).is_err());
            assert!(SetRemove::remove(&mut storage, &integer(2), policy).is_err());
            assert!(!SetRemove::remove(&mut storage, &integer(9), policy).unwrap());
        }
        drop(lease);
        let mut storage = MutableAccess::new(value.view_mut(), &SET, &services).unwrap();
        assert!(SetRemove::remove(&mut storage, &integer(2), policy).unwrap());
        assert!(!SetRemove::remove(&mut storage, &integer(2), policy).unwrap());
    }
}

#[test]
fn set_algebra_clones_only_selected_values_using_each_sources_services() {
    let left = collection(&[1, 2]);
    let right = collection(&[2, 3]);
    for policy in [KeyPolicy::Hash, KeyPolicy::Ordered] {
        for (operation, expected, left_copies, right_copies) in [
            (0, vec![1, 2, 3], vec![1], vec![2, 3]),
            (1, vec![2], vec![], vec![2]),
            (2, vec![1], vec![1], vec![]),
            (3, vec![1, 3], vec![1], vec![3]),
        ] {
            let left_services = Services::default();
            let right_services = Services::default();
            let left = SharedAccess::new(left.view(), &SET, &left_services).unwrap();
            let right = SharedAccess::new(right.view(), &SET, &right_services).unwrap();
            let result = match operation {
                0 => left.union(&right, policy, OwnedSequence::into_elements),
                1 => left.intersection(&right, policy, OwnedSequence::into_elements),
                2 => left.difference(&right, policy, OwnedSequence::into_elements),
                _ => left.symmetric_difference(&right, policy, OwnedSequence::into_elements),
            }
            .unwrap();
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
            assert_eq!(
                *left_services.copies.borrow(),
                left_copies
                    .into_iter()
                    .map(NativeKey::Signed)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                *right_services.copies.borrow(),
                right_copies
                    .into_iter()
                    .map(NativeKey::Signed)
                    .collect::<Vec<_>>()
            );
        }
    }
    let services = Services::default();
    let rejecting = Services {
        reject: true,
        ..Services::default()
    };
    let left_view = SharedAccess::new(left.view(), &SET, &services).unwrap();
    let right_view = SharedAccess::new(right.view(), &SET, &rejecting).unwrap();
    let empty = collection(&[]);
    let empty = SharedAccess::new(empty.view(), &SET, &services).unwrap();
    assert!(left_view.is_subset(&left_view, KeyPolicy::Hash).unwrap());
    assert!(!left_view.is_superset(&right_view, KeyPolicy::Hash).unwrap());
    assert!(left_view.is_disjoint(&empty, KeyPolicy::Hash).unwrap());
    assert!(
        left_view
            .union(&right_view, KeyPolicy::Hash, OwnedSequence::into_elements)
            .is_err()
    );
    assert_eq!(values(&left), [1, 2].map(NativeKey::Signed));
    assert_eq!(values(&right), [2, 3].map(NativeKey::Signed));
}

struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn maps_move_non_clone_values_with_declaration_driven_entry_order() {
    let services = Services::default();
    for (schema, reversed) in [(&MAP, false), (&REVERSED_MAP, true)] {
        for policy in [KeyPolicy::Hash, KeyPolicy::Ordered] {
            let drops = Rc::new(Cell::new(0));
            let payload = DynamicLayout::of::<Probe>(Type::named("Probe"));
            let fields = if reversed {
                vec![payload.clone(), DynamicLayout::copy_of::<i32>(Type::I32)]
            } else {
                vec![DynamicLayout::copy_of::<i32>(Type::I32), payload.clone()]
            };
            let entry = DynamicLayout::aggregate(
                Type::Tuple(
                    fields
                        .iter()
                        .map(|field| field.rils_type().clone())
                        .collect(),
                ),
                fields
                    .into_iter()
                    .enumerate()
                    .map(|(index, layout)| (index.to_string(), layout))
                    .collect(),
            )
            .unwrap();
            let mut value = DynamicValue::sequence(
                DynamicLayout::sequence(
                    Type::Named {
                        name: MAP.owner.into(),
                        arguments: vec![Type::I32, Type::named("Probe")],
                    },
                    entry,
                ),
                vec![],
            )
            .unwrap();
            let new_payload =
                || DynamicValue::from_rust(payload.clone(), Probe(drops.clone())).unwrap();
            for number in [3, 1, 2] {
                let mut storage = MutableAccess::new(value.view_mut(), schema, &services).unwrap();
                assert!(
                    MapInsert::insert(&mut storage, integer(number), new_payload(), policy)
                        .unwrap()
                        .is_none()
                );
            }
            assert_eq!(drops.get(), 0);
            let previous = MapInsert::insert(
                &mut MutableAccess::new(value.view_mut(), schema, &services).unwrap(),
                integer(1),
                new_payload(),
                policy,
            )
            .unwrap()
            .unwrap();
            assert_eq!(drops.get(), 0);
            drop(previous);
            assert_eq!(drops.get(), 1);
            let lease = value.sequence_borrows().unwrap().begin_iteration().unwrap();
            assert!(
                MapInsert::insert(
                    &mut MutableAccess::new(value.view_mut(), schema, &services).unwrap(),
                    integer(1),
                    new_payload(),
                    policy
                )
                .is_err()
            );
            assert_eq!(drops.get(), 2);
            assert!(
                MapRemove::remove(
                    &mut MutableAccess::new(value.view_mut(), schema, &services).unwrap(),
                    &integer(1),
                    policy
                )
                .is_err()
            );
            drop(lease);
            let removed = MapRemove::remove(
                &mut MutableAccess::new(value.view_mut(), schema, &services).unwrap(),
                &integer(1),
                policy,
            )
            .unwrap()
            .unwrap();
            assert_eq!(drops.get(), 2);
            drop(removed);
            assert_eq!(drops.get(), 3);
            drop(value);
            assert_eq!(drops.get(), 5);
        }
    }
}
