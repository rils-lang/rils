use super::*;

pub(super) const HASH_MAP_MEMBERS: &[BuiltinMember] = &[
    member!("new", associated [] -> TypePattern::SelfType, "Creates an empty HashMap."),
    member!("len", method Shared [] -> TypePattern::Usize, HashMapLen, "Returns the entry count."),
    member!("is_empty", method Shared [] -> TypePattern::Bool, HashMapIsEmpty, "Returns true when the map has no entries."),
    member!("clear", method Mutable [] -> TypePattern::Unit, HashMapClear, "Removes all entries."),
    member!("contains_key", method Shared [TypePattern::Reference { mutable: false, inner: &K }] -> TypePattern::Bool, HashMapContainsKey, "Returns true when the key is present."),
    member!("insert", method Mutable [K, V] -> TypePattern::Option(&V), HashMapInsert, "Inserts a key-value pair and returns the previous value."),
    member!("get_cloned", method Shared [TypePattern::Reference { mutable: false, inner: &K }] -> TypePattern::Option(&V), HashMapGetCloned, "Clones the value stored for a key."),
    member!("remove", method Mutable [TypePattern::Reference { mutable: false, inner: &K }] -> TypePattern::Option(&V), HashMapRemove, "Removes a key and returns its value."),
    member!("keys_cloned", method Shared [] -> TypePattern::Named { path: "SequenceIterator", arguments: &[K] }, HashMapKeysCloned, "Clones all keys into an owned iterator."),
    member!("values_cloned", method Shared [] -> TypePattern::Named { path: "SequenceIterator", arguments: &[V] }, HashMapValuesCloned, "Clones all values into an owned iterator."),
    member!("into_iter", method Owned [] -> TypePattern::Named { path: "SequenceIterator", arguments: &[TypePattern::Tuple(&[K, V])] }, HashMapIntoIter, "Consumes the map and iterates over owned key-value pairs."),
];

const HASH_SET_T: TypePattern = TypePattern::Named {
    path: "HashSet",
    arguments: &[T],
};
const REF_HASH_SET_T: TypePattern = TypePattern::Reference {
    mutable: false,
    inner: &HASH_SET_T,
};

pub(super) const HASH_SET_MEMBERS: &[BuiltinMember] = &[
    member!("new", associated [] -> TypePattern::SelfType, "Creates an empty HashSet."),
    member!("len", method Shared [] -> TypePattern::Usize, HashSetLen, "Returns the element count."),
    member!("is_empty", method Shared [] -> TypePattern::Bool, HashSetIsEmpty, "Returns true when the set has no elements."),
    member!("clear", method Mutable [] -> TypePattern::Unit, HashSetClear, "Removes all elements."),
    member!("contains", method Shared [REF_T] -> TypePattern::Bool, HashSetContains, "Returns true when the value is present."),
    member!("insert", method Mutable [T] -> TypePattern::Bool, HashSetInsert, "Inserts a value and reports whether it was new."),
    member!("remove", method Mutable [REF_T] -> TypePattern::Bool, HashSetRemove, "Removes a value and reports whether it was present."),
    member!("is_subset", method Shared [REF_HASH_SET_T] -> TypePattern::Bool, HashSetIsSubset, "Returns true when every element is in the other set."),
    member!("is_superset", method Shared [REF_HASH_SET_T] -> TypePattern::Bool, HashSetIsSuperset, "Returns true when the set contains every element of the other set."),
    member!("is_disjoint", method Shared [REF_HASH_SET_T] -> TypePattern::Bool, HashSetIsDisjoint, "Returns true when the sets share no elements."),
    member!("union", method Shared [REF_HASH_SET_T] -> HASH_SET_T, HashSetUnion, "Clones the union of two sets."),
    member!("intersection", method Shared [REF_HASH_SET_T] -> HASH_SET_T, HashSetIntersection, "Clones the intersection of two sets."),
    member!("difference", method Shared [REF_HASH_SET_T] -> HASH_SET_T, HashSetDifference, "Clones values that are not in the other set."),
    member!("symmetric_difference", method Shared [REF_HASH_SET_T] -> HASH_SET_T, HashSetSymmetricDifference, "Clones values present in exactly one set."),
    member!("into_iter", method Owned [] -> TypePattern::Named { path: "SequenceIterator", arguments: &[T] }, HashSetIntoIter, "Consumes the set and iterates over its values."),
];
