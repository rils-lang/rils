use rils_stdlib::stdlib::{binary_heap::BinaryHeap, prelude::Option, string::String};

fn standard<T>(value: Option<T>) -> std::option::Option<T> {
    match value {
        Option::Some(value) => Some(value),
        Option::None => None,
    }
}

#[test]
fn native_heap_orders_supported_values_and_clears() {
    let mut numbers = BinaryHeap::<i32>::new();
    assert!(numbers.is_empty());
    assert_eq!(standard(numbers.pop()), None);
    for value in [2, 5, 1, 5] {
        numbers.push(value);
    }
    assert_eq!(numbers.len(), 4);
    assert_eq!(standard(numbers.peek_cloned()), Some(5));
    assert_eq!(standard(numbers.pop()), Some(5));
    assert_eq!(standard(numbers.pop()), Some(5));
    assert_eq!(standard(numbers.pop()), Some(2));
    assert_eq!(standard(numbers.pop()), Some(1));

    let mut letters = BinaryHeap::<char>::new();
    letters.push('a');
    letters.push('z');
    assert_eq!(standard(letters.pop()), Some('z'));

    let mut words = BinaryHeap::<String>::new();
    words.push("a".to_owned().into());
    words.push("z".to_owned().into());
    assert_eq!(
        std::string::String::from(standard(words.pop()).unwrap()),
        "z"
    );
    words.clear();
    assert!(words.is_empty());
}
