use rils_stdlib::{
    stdlib::collections::{BinaryHeap, VecDeque},
    storage::CloningView,
};
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
            BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
                .push(item(value))
                .unwrap();
            expected.push(value);
        }
        let heap = native.view();
        let heap = BinaryHeap::<DynamicValue, _>::with_storage(CloningView::new(
            heap.indexed().unwrap(),
            |view| view.with_rust::<i32, _>(|value| item(*value)),
        ));
        assert_eq!(
            heap.peek_cloned().unwrap().map(number),
            expected.peek().copied()
        );
        let lease = native
            .sequence_borrows()
            .unwrap()
            .begin_iteration()
            .unwrap();
        if !expected.is_empty() {
            assert!(
                BinaryHeap::<(), _>::with_storage(native.view_mut())
                    .clear()
                    .is_err()
            );
            assert!(
                BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
                    .pop()
                    .is_err()
            );
        }
        assert!(
            BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
                .push(item(100))
                .is_err()
        );
        drop(lease);
        while let Some(value) = expected.pop() {
            assert_eq!(
                number(
                    BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
                        .pop()
                        .unwrap()
                        .unwrap()
                ),
                value
            );
        }
        assert!(
            BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
                .pop()
                .unwrap()
                .is_none()
        );
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
        assert!(pop(&mut native, back).is_err());
        drop(lease);
        for expected in if back { [3, 2, 1] } else { [1, 2, 3] } {
            assert_eq!(number(pop(&mut native, back).unwrap().unwrap()), expected);
        }
        assert!(pop(&mut native, back).unwrap().is_none());
    }
}

fn pop(value: &mut DynamicValue, back: bool) -> Result<Option<DynamicValue>, String> {
    let mut deque = VecDeque::<DynamicValue, _>::with_storage(value.view_mut());
    if back {
        deque.pop_back()
    } else {
        deque.pop_front()
    }
}

#[test]
fn heap_rejects_unsupported_elements_before_mutating_and_clear_keeps_layout() {
    let mut unsupported = DynamicValue::sequence(
        DynamicLayout::sequence(
            Type::named("Heap"),
            DynamicLayout::copy_of::<bool>(Type::Bool),
        ),
        vec![],
    )
    .unwrap();
    let rejected =
        DynamicValue::from_rust(DynamicLayout::copy_of::<bool>(Type::Bool), true).unwrap();
    let error = BinaryHeap::<DynamicValue, _>::with_storage(unsupported.view_mut())
        .push(rejected)
        .unwrap_err();
    assert!(error.contains("does not support ordering bool"), "{error}");
    assert_eq!(unsupported.view().sequence_len().unwrap(), 0);

    let mut native = empty();
    for value in [3, 1, 2] {
        BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
            .push(item(value))
            .unwrap();
    }
    let view = native.view();
    let failing = BinaryHeap::<DynamicValue, _>::with_storage(CloningView::new(
        view.indexed().unwrap(),
        |_| Err("clone failed".into()),
    ));
    assert_eq!(failing.peek_cloned().err().unwrap(), "clone failed");
    BinaryHeap::<(), _>::with_storage(native.view_mut())
        .clear()
        .unwrap();
    assert!(
        BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
            .pop()
            .unwrap()
            .is_none()
    );
    BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
        .push(item(7))
        .unwrap();
    assert_eq!(
        number(
            BinaryHeap::<DynamicValue, _>::with_storage(native.view_mut())
                .pop()
                .unwrap()
                .unwrap()
        ),
        7
    );
}
