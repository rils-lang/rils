use super::{IntegerType, empty_call, integer_loop};

#[test]
fn empty_call_returns_its_argument() {
    empty_call(1_000)
        .expect("build empty VM call")
        .run()
        .expect("run empty VM call");
}

#[test]
fn integer_loop_returns_the_expected_sum() {
    integer_loop(1_000)
        .expect("build VM integer loop")
        .run()
        .expect("run VM integer loop");
}

#[test]
fn counter_loop_supports_each_benchmarked_integer_type() {
    for integer_type in [
        IntegerType::I32,
        IntegerType::U32,
        IntegerType::I64,
        IntegerType::U64,
        IntegerType::Usize,
    ] {
        super::counter_loop(1_000, integer_type)
            .expect("build VM counter loop")
            .run()
            .expect("run VM counter loop");
    }
}
