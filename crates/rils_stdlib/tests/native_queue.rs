use rils_stdlib::native_collections::queue;
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue};

fn item(number: i32) -> DynamicValue {
    DynamicValue::from_rust(DynamicLayout::copy_of::<i32>(Type::I32), number).unwrap()
}
fn empty() -> DynamicValue {
    DynamicValue::sequence(
        DynamicLayout::sequence(
            Type::named("Queue"),
            DynamicLayout::copy_of::<i32>(Type::I32),
        ),
        vec![],
    )
    .unwrap()
}
fn number(item: DynamicValue) -> i32 {
    item.into_rust::<i32>()
        .unwrap_or_else(|(_, error)| panic!("{error}"))
}

#[test]
fn native_heap_matches_rust_heap_and_preserves_storage_on_borrow_failure() {
    for input in [
        vec![],
        vec![1],
        vec![2, 1, 2, -1, i32::MAX, i32::MIN],
        (0..32).collect(),
    ] {
        let mut native = empty();
        let mut expected = std::collections::BinaryHeap::new();
        for value in input {
            queue::heap_push(native.view_mut(), item(value)).unwrap();
            expected.push(value);
        }
        let lease = native
            .sequence_borrows()
            .unwrap()
            .begin_iteration()
            .unwrap();
        if !expected.is_empty() {
            assert!(queue::heap_pop(native.view_mut()).is_err());
        }
        assert!(queue::heap_push(native.view_mut(), item(100)).is_err());
        drop(lease);
        while let Some(value) = expected.pop() {
            assert_eq!(
                number(queue::heap_pop(native.view_mut()).unwrap().unwrap()),
                value
            );
        }
        assert!(queue::heap_pop(native.view_mut()).unwrap().is_none());
    }
}

#[test]
fn deque_pop_preserves_end_order_and_rejects_active_borrows() {
    for back in [false, true] {
        let mut native = empty();
        for value in [1, 2, 3] {
            native.push_sequence_item(item(value)).unwrap();
        }
        let lease = native
            .sequence_borrows()
            .unwrap()
            .begin_iteration()
            .unwrap();
        assert!(queue::pop(native.view_mut(), back).is_err());
        drop(lease);
        for expected in if back { [3, 2, 1] } else { [1, 2, 3] } {
            assert_eq!(
                number(queue::pop(native.view_mut(), back).unwrap().unwrap()),
                expected
            );
        }
        assert!(queue::pop(native.view_mut(), back).unwrap().is_none());
    }
}
