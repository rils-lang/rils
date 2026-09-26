use std::collections::VecDeque;

use rils_stdlib::stdlib::{iterator::Iter, string::Iterator};

#[test]
fn owned_and_borrowed_rust_iterator_adapters_advance_in_order() {
    let mut owned = Iterator(VecDeque::from([1, 2]));
    assert_eq!(owned.next(), Some(1));
    assert_eq!(owned.next(), Some(2));
    assert_eq!(owned.next(), None);

    let mut borrowed = Iter::from(vec![3, 4]);
    assert_eq!(borrowed.next(), Some(3));
    assert_eq!(borrowed.next(), Some(4));
    assert_eq!(borrowed.next(), None);
}
