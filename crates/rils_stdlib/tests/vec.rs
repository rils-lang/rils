use rils_stdlib::stdlib::{collections::vector::Vec as RilsVec, option::Option};

#[test]
fn native_vec_supports_fixed_arrays_and_borrowed_iteration() {
    let mut values = RilsVec::from([1, 2, 3]);
    assert_eq!(values.len(), 3);
    assert_eq!(
        values
            .iter()
            .0
            .into_iter()
            .copied()
            .collect::<std::vec::Vec<_>>(),
        [1, 2, 3]
    );
    values.push(4);
    assert!(values.contains(&2));
    assert!(matches!(values.pop(), Option::Some(4)));
    values.insert(1, 5);
    assert_eq!(values.remove(1), 5);
    assert_eq!(values.into_iter().0, [1, 2, 3]);
}
