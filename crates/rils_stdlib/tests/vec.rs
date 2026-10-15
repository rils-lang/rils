use rils_stdlib::stdlib::{collections::vector::Vec as RilsVec, option::Option};

#[test]
fn native_queries_borrow_the_original_allocation_without_cloning_elements() {
    struct NonClone(String);
    for length in [0, 1, 3] {
        let storage = (0..length)
            .map(|i| NonClone(i.to_string()))
            .collect::<Vec<_>>();
        let receiver = RilsVec::<NonClone, _>::with_storage(storage.as_slice());
        assert_eq!(receiver.len(), length);
        assert_eq!(receiver.is_empty(), length == 0);
        for (index, value) in storage.iter().enumerate() {
            assert_eq!(value.0, index.to_string());
        }
    }
}

#[test]
fn native_vec_supports_fixed_arrays_and_borrowed_iteration() {
    let mut values = RilsVec::<i32>::from([1, 2, 3]);
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

#[test]
fn queries_use_native_views_for_both_arrays_and_sequences() {
    use rils_stdlib::Type;
    use rils_value::{DynamicLayout, DynamicValue};
    for array in [false, true] {
        for length in [0, 1, 4] {
            let item = DynamicLayout::of::<String>(Type::String);
            let items = (0..length)
                .map(|i| DynamicValue::from_rust(item.clone(), i.to_string()).unwrap())
                .collect();
            let value = if array {
                DynamicValue::record(DynamicLayout::array(item, length).unwrap(), items).unwrap()
            } else {
                DynamicValue::sequence(DynamicLayout::sequence(Type::named("Items"), item), items)
                    .unwrap()
            };
            let view = value.view();
            let receiver = RilsVec::<(), _>::with_storage(view.indexed().unwrap());
            assert_eq!(receiver.len(), length);
            assert_eq!(receiver.is_empty(), length == 0);
            for i in 0..length {
                assert!(
                    view.indexed()
                        .unwrap()
                        .item(i)
                        .unwrap()
                        .with_rust::<String, _>(|v| v == &i.to_string())
                        .unwrap()
                );
            }
        }
    }
}
