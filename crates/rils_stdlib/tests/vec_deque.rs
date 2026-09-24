use rils_stdlib::stdlib::{prelude::Option, vec_deque::VecDeque};

fn standard<T>(value: Option<T>) -> std::option::Option<T> {
    match value {
        Option::Some(value) => Some(value),
        Option::None => None,
    }
}

#[test]
fn native_deque_preserves_both_ends_and_empty_cases() {
    let mut queue = VecDeque::new();
    assert!(queue.is_empty());
    assert_eq!(standard(queue.pop_front()), None);
    assert_eq!(standard(queue.pop_back()), None);

    queue.push_back(2);
    queue.push_front(1);
    queue.push_back(3);
    assert_eq!(queue.len(), 3);
    assert_eq!(standard(queue.front_cloned()), Some(1));
    assert_eq!(standard(queue.back_cloned()), Some(3));
    assert_eq!(standard(queue.pop_front()), Some(1));
    assert_eq!(standard(queue.pop_back()), Some(3));
    assert_eq!(standard(queue.pop_back()), Some(2));
    queue.push_back(4);
    queue.clear();
    assert!(queue.is_empty());
}

#[test]
fn deque_wrapper_exposes_the_std_queue() {
    let mut queue = VecDeque::from(std::collections::VecDeque::from([1]));
    queue.reserve(4);
    assert_eq!(queue.front(), Some(&1));
    let standard: std::collections::VecDeque<_> = queue.into();
    assert_eq!(standard.into_iter().collect::<Vec<_>>(), vec![1]);
}
