use rils_stdlib::stdlib::collections::{HashMap, HashSet};
use rils_stdlib::stdlib::option::Option;

fn value<T>(option: Option<T>) -> std::option::Option<T> {
    match option {
        Option::Some(value) => Some(value),
        Option::None => None,
    }
}

#[test]
fn hash_set_supports_owned_and_borrowed_operations() {
    let mut values = HashSet::new();
    assert!(values.insert(1));
    assert!(values.insert(3));
    assert!(!values.insert(1));
    assert!(values.contains(&3));
    assert_eq!(values.iter().0.len(), 2);

    let mut other = HashSet::new();
    other.insert(3);
    other.insert(4);
    assert_eq!(values.union(&other).len(), 3);
    assert_eq!(values.intersection(&other).len(), 1);
    assert_eq!(values.difference(&other).len(), 1);
    assert_eq!(values.symmetric_difference(&other).len(), 2);
    assert!(values.remove(&1));
    assert_eq!(values.into_iter().0, [3]);
}

#[test]
fn hash_map_preserves_replaced_values_and_entries() {
    let mut values = HashMap::new();
    assert_eq!(value(values.insert(1, "first")), None);
    assert_eq!(value(values.insert(2, "second")), None);
    assert_eq!(value(values.insert(1, "replacement")), Some("first"));
    assert_eq!(value(values.get_cloned(&1)), Some("replacement"));
    assert_eq!(values.keys_cloned().0.len(), 2);
    assert_eq!(values.values_cloned().0.len(), 2);
    assert_eq!(values.iter().0.len(), 2);
    assert_eq!(value(values.remove(&2)), Some("second"));
    assert_eq!(values.into_iter().0, [(1, "replacement")]);
}
