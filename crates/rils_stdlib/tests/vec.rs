use rils_stdlib::stdlib::{collections::vector::Vec as RilsVec, option::Option};

#[test]
fn native_queries_borrow_the_original_allocation_without_cloning_elements() {
    struct NonClone(String);
    for length in [0, 1, 3] {
        let storage = (0..length)
            .map(|i| NonClone(i.to_string()))
            .collect::<Vec<_>>();
        let receiver = RilsVec::from_storage(&storage);
        assert_eq!(receiver.len(), length);
        assert_eq!(receiver.is_empty(), length == 0);
        assert_eq!(receiver.as_ptr(), storage.as_ptr());
        for (index, value) in receiver.iter().enumerate() {
            assert_eq!(value.0, index.to_string());
        }
    }
}

#[test]
fn native_vec_supports_fixed_arrays_and_borrowed_iteration() {
    let mut values = RilsVec::from([1, 2, 3]);
    assert_eq!(values.len(), 3);
    assert_eq!(
        values.iter().copied().collect::<std::vec::Vec<_>>(),
        [1, 2, 3]
    );
    values.push(4);
    assert!(values.contains(&2));
    assert!(matches!(values.pop(), Option::Some(4)));
    values.insert(1, 5);
    assert_eq!(values.remove(1), 5);
    assert_eq!(values.into_iter().collect::<std::vec::Vec<_>>(), [1, 2, 3]);
}
