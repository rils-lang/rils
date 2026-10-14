use rils_execution::{
    Type, Value,
    runtime_builtins::{
        NativeOwnedContext, call_native_owned_symbol, call_native_symbol,
        requires_owned_native_call,
    },
    value::dynamic_sequence,
};

fn empty(owner: &str) -> Value {
    let count = if owner.ends_with("Map") { 2 } else { 1 };
    dynamic_sequence::empty_with_definitions(
        &Type::Named {
            name: owner.into(),
            arguments: vec![Type::Integer(rils_execution::IntegerType::I32); count],
        },
        &[],
        &[],
    )
    .unwrap()
}

#[test]
fn every_declared_keyed_method_has_a_bridge_and_checks_arity() {
    for owner in [
        "Vec",
        "HashMap",
        "BTreeMap",
        "HashSet",
        "BTreeSet",
        "VecDeque",
        "BinaryHeap",
    ] {
        for member in rils_builtins::builtin(owner).unwrap().members {
            let Some(symbol) = member.native_symbol else {
                continue;
            };
            if member.name == "new" {
                continue;
            }
            let result = if requires_owned_native_call(symbol) {
                call_native_owned_symbol(symbol, vec![], &NativeOwnedContext::default())
            } else {
                call_native_symbol(symbol, &[])
            };
            let error = result
                .unwrap_or_else(|| panic!("unregistered {symbol}"))
                .unwrap_err();
            assert!(error.contains("expects"), "{symbol}: {error}");
        }
    }
}

#[test]
fn iterators_reject_other_collection_receivers_before_borrowing_or_consuming() {
    for owner in [
        "HashMap",
        "BTreeMap",
        "HashSet",
        "BTreeSet",
        "VecDeque",
        "BinaryHeap",
    ] {
        for other in [
            "HashMap",
            "BTreeMap",
            "HashSet",
            "BTreeSet",
            "VecDeque",
            "BinaryHeap",
        ] {
            if owner == other {
                continue;
            }
            for method in ["iter", "into_iter"] {
                if method == "iter" && matches!(owner, "VecDeque" | "BinaryHeap") {
                    continue;
                }
                let symbol = rils_builtins::builtin_member(owner, method)
                    .unwrap()
                    .native_symbol
                    .unwrap();
                let error = call_native_owned_symbol(
                    symbol,
                    vec![empty(other)],
                    &NativeOwnedContext::default(),
                )
                .unwrap()
                .unwrap_err();
                assert!(
                    error.contains(owner) && error.contains("wrong"),
                    "{owner}::{method}({other}): {error}"
                );
            }
        }
    }
}

#[test]
fn generated_mutations_require_mutable_receivers() {
    for owner in ["HashMap", "BTreeMap", "HashSet", "BTreeSet"] {
        for method in ["insert", "remove", "clear"] {
            let symbol = rils_builtins::builtin_member(owner, method)
                .unwrap()
                .native_symbol
                .unwrap();
            let mut arguments = vec![empty(owner)];
            if method != "clear" {
                arguments.push(rils_execution::value::native_i32(1).into());
            }
            if method == "insert" && owner.ends_with("Map") {
                arguments.push(rils_execution::value::native_i32(2).into());
            }
            let result = if requires_owned_native_call(symbol) {
                call_native_owned_symbol(symbol, arguments, &NativeOwnedContext::default())
            } else {
                call_native_symbol(symbol, &arguments)
            };
            let error = result.unwrap().unwrap_err();
            assert!(
                error.contains("mutable reference"),
                "{owner}::{method}: {error}"
            );
        }
    }
}
