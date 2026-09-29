use rils_stdlib::stdlib::{iterator::Iter, string::String as RilsString};

#[test]
fn owned_and_borrowed_rust_iterator_adapters_advance_in_order() {
    let mut owned = Iter::from(vec![1, 2]);
    assert_eq!(owned.next(), Some(1));
    assert_eq!(owned.next(), Some(2));
    assert_eq!(owned.next(), None);

    let mut borrowed = Iter::from(vec![3, 4]);
    assert_eq!(borrowed.next(), Some(3));
    assert_eq!(borrowed.next(), Some(4));
    assert_eq!(borrowed.next(), None);
}

#[test]
fn string_iterators_yield_on_demand_with_rust_matching_edges() {
    let text = RilsString::from("é\r\nthird\n".to_owned());
    assert_eq!(
        text.chars().collect::<Vec<_>>(),
        "é\r\nthird\n".chars().collect::<Vec<_>>()
    );
    assert_eq!(
        text.bytes().collect::<Vec<_>>(),
        "é\r\nthird\n".bytes().collect::<Vec<_>>()
    );
    assert_eq!(
        text.lines().map(String::from).collect::<Vec<_>>(),
        "é\r\nthird\n"
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
    for pattern in ["", "é", "missing", "\n"] {
        let actual = text
            .split(RilsString::from(pattern.to_owned()))
            .map(String::from)
            .collect::<Vec<_>>();
        let expected = "é\r\nthird\n"
            .split(pattern)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "pattern {pattern:?}");
    }
}
