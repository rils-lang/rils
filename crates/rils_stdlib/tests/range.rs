use rils_stdlib::stdlib::range::Range;

#[test]
fn every_integer_range_advances_to_its_exclusive_end() {
    macro_rules! check {
        ($($integer:ty),* $(,)?) => {
            $(
                let mut range = Range::from_bounds(1 as $integer, 3 as $integer);
                assert_eq!(range.next(), Some(1 as $integer));
                assert_eq!(range.next(), Some(2 as $integer));
                assert_eq!(range.next(), None);
                assert_eq!(range.current(), 3 as $integer);

                let mut empty = Range::from_bounds(2 as $integer, 1 as $integer);
                assert_eq!(empty.next(), None);
                assert_eq!(empty.current(), 2 as $integer);

                let mut edge = Range::from_bounds(<$integer>::MAX - 1, <$integer>::MAX);
                assert_eq!(edge.next(), Some(<$integer>::MAX - 1));
                assert_eq!(edge.next(), None);
            )*
        };
    }
    check!(
        i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
    );
}

#[test]
fn range_also_implements_the_bound_rust_iterator() {
    fn through_blanket_impl<T: IntoIterator<IntoIter = T>>(value: T) -> T {
        value.into_iter()
    }
    let values: Vec<_> = Range::from_bounds(1i32, 4i32).collect();
    assert_eq!(values, [1, 2, 3]);
    let via_blanket_impl: Vec<_> = through_blanket_impl(Range::from_bounds(1i32, 4i32)).collect();
    assert_eq!(via_blanket_impl, values);
}
