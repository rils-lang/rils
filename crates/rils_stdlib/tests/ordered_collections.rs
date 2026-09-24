use rils_stdlib::stdlib::collections::{BTreeMap, BTreeSet};
use rils_stdlib::stdlib::option::Option;

fn value<T>(option: Option<T>) -> std::option::Option<T> {
    match option {
        Option::Some(value) => Some(value),
        Option::None => None,
    }
}

#[test]
fn ordered_set_preserves_order_and_set_operations() {
    let mut values = BTreeSet::new();
    assert!(values.insert(3));
    assert!(values.insert(1));
    assert!(!values.insert(1));
    assert_eq!(values.len(), 2);
    assert_eq!(value(values.first_cloned()), Some(1));
    assert_eq!(value(values.last_cloned()), Some(3));
    assert_eq!(
        values.iter().0.into_iter().copied().collect::<Vec<_>>(),
        [1, 3]
    );

    let mut other = BTreeSet::new();
    other.insert(3);
    other.insert(4);
    assert_eq!(values.union(&other).into_iter().0, [1, 3, 4]);
    assert_eq!(values.intersection(&other).into_iter().0, [3]);
    assert_eq!(values.difference(&other).into_iter().0, [1]);
    assert_eq!(values.symmetric_difference(&other).into_iter().0, [1, 4]);
    assert!(values.remove(&1));
    assert!(!values.remove(&1));
}

#[test]
fn ordered_map_preserves_keys_and_replaces_values() {
    let mut values = BTreeMap::new();
    assert_eq!(value(values.insert(2, "two")), None);
    assert_eq!(value(values.insert(1, "one")), None);
    assert_eq!(value(values.insert(2, "second")), Some("two"));
    assert_eq!(value(values.get_cloned(&2)), Some("second"));
    assert_eq!(value(values.first_key_cloned()), Some(1));
    assert_eq!(value(values.last_key_cloned()), Some(2));
    assert_eq!(values.iter().0, [(&1, &"one"), (&2, &"second")]);
    assert_eq!(value(values.remove(&1)), Some("one"));
    assert_eq!(values.into_iter().0, [(2, "second")]);
}
