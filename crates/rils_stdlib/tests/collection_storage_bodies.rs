use rils_stdlib::stdlib::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use rils_stdlib::storage::{KeyPolicy, MapInsert, SetAlgebra, SetInsert};

struct RecordingStorage;

fn policy_name(policy: KeyPolicy) -> &'static str {
    match policy {
        KeyPolicy::Hash => "hash",
        KeyPolicy::Ordered => "ordered",
    }
}

impl SetInsert<i32> for RecordingStorage {
    type InsertOutput = (i32, &'static str);
    fn insert(&mut self, value: i32, policy: KeyPolicy) -> Self::InsertOutput {
        (value, policy_name(policy))
    }
}

impl MapInsert<i32, String> for RecordingStorage {
    type InsertOutput = (i32, String, &'static str);
    fn insert(&mut self, key: i32, value: String, policy: KeyPolicy) -> Self::InsertOutput {
        (key, value, policy_name(policy))
    }
}

#[test]
fn collection_bodies_select_key_policy_and_preserve_backend_outputs() {
    for (actual, expected) in [
        (
            HashSet::<i32, _>::with_storage(RecordingStorage).insert(7),
            (7, "hash"),
        ),
        (
            BTreeSet::<i32, _>::with_storage(RecordingStorage).insert(11),
            (11, "ordered"),
        ),
    ] {
        assert_eq!(actual, expected);
    }
    for (actual, expected) in [
        (
            HashMap::<i32, String, _>::with_storage(RecordingStorage).insert(7, "h".into()),
            (7, "h".to_owned(), "hash"),
        ),
        (
            BTreeMap::<i32, String, _>::with_storage(RecordingStorage).insert(11, "b".into()),
            (11, "b".to_owned(), "ordered"),
        ),
    ] {
        assert_eq!(actual, expected);
    }
}

struct AlgebraStorage(i32);

impl SetAlgebra for AlgebraStorage {
    type Storage = (i32, i32, &'static str, &'static str);
    type AlgebraOutput<R> = Result<R, &'static str>;

    fn union<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R> {
        Ok(wrap((self.0, other.0, policy_name(policy), "union")))
    }

    fn intersection<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R> {
        Ok(wrap((self.0, other.0, policy_name(policy), "intersection")))
    }

    fn difference<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R> {
        Ok(wrap((self.0, other.0, policy_name(policy), "difference")))
    }

    fn symmetric_difference<R>(
        &self,
        _other: &Self,
        _policy: KeyPolicy,
        _wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R> {
        Err("backend rejected clone")
    }
}

#[test]
fn set_algebra_wraps_distinct_result_storage_and_keeps_backend_failure() {
    let left = HashSet::<i32, _>::with_storage(AlgebraStorage(3));
    let right = HashSet::<i32, _>::with_storage(AlgebraStorage(5));
    for (actual, operation) in [
        (left.union(&right), "union"),
        (left.intersection(&right), "intersection"),
        (left.difference(&right), "difference"),
    ] {
        assert_eq!(actual.unwrap().into_storage(), (3, 5, "hash", operation));
    }
    assert!(matches!(
        left.symmetric_difference(&right),
        Err("backend rejected clone")
    ));

    let left = BTreeSet::<i32, _>::with_storage(AlgebraStorage(3));
    let right = BTreeSet::<i32, _>::with_storage(AlgebraStorage(5));
    for (actual, operation) in [
        (left.union(&right), "union"),
        (left.intersection(&right), "intersection"),
        (left.difference(&right), "difference"),
    ] {
        assert_eq!(actual.unwrap().into_storage(), (3, 5, "ordered", operation));
    }
    assert!(matches!(
        left.symmetric_difference(&right),
        Err("backend rejected clone")
    ));
}
