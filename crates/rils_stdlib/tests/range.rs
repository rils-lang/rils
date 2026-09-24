use rils_stdlib::stdlib::{prelude::Option, range::Range};

fn standard<T>(value: Option<T>) -> std::option::Option<T> {
    match value {
        Option::Some(value) => Some(value),
        Option::None => None,
    }
}

#[test]
fn every_integer_range_advances_to_its_exclusive_end() {
    macro_rules! check {
        ($($integer:ty),* $(,)?) => {
            $(
                let mut range = Range::from_bounds(1 as $integer, 3 as $integer);
                assert_eq!(standard(range.next()), Some(1 as $integer));
                assert_eq!(standard(range.next()), Some(2 as $integer));
                assert_eq!(standard(range.next()), None);
                assert_eq!(range.current(), 3 as $integer);

                let mut empty = Range::from_bounds(2 as $integer, 1 as $integer);
                assert_eq!(standard(empty.next()), None);
                assert_eq!(empty.current(), 2 as $integer);

                let mut edge = Range::from_bounds(<$integer>::MAX - 1, <$integer>::MAX);
                assert_eq!(standard(edge.next()), Some(<$integer>::MAX - 1));
                assert_eq!(standard(edge.next()), None);
            )*
        };
    }
    check!(
        i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
    );
}

#[test]
fn range_also_implements_the_bound_rust_iterator() {
    let values: Vec<_> = Range::from_bounds(1i32, 4i32).collect();
    assert_eq!(values, [1, 2, 3]);
}
