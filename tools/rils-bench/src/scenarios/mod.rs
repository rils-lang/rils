mod vm;

use crate::args::{IntegerType, Scenario};

pub(crate) struct Benchmark {
    pub(crate) name: &'static str,
    pub(crate) case: &'static str,
    pub(crate) integer_type: Option<&'static str>,
    run: Box<dyn Fn() -> Result<(), String>>,
}

impl Benchmark {
    pub(crate) fn run(&self) -> Result<(), String> {
        (self.run)()
    }
}

pub(crate) fn build(
    scenario: Scenario,
    work: usize,
    integer_type: IntegerType,
) -> Result<Benchmark, String> {
    match scenario {
        Scenario::EmptyCall => vm::empty_call(work),
        Scenario::CounterLoop => vm::counter_loop(work, integer_type),
        Scenario::IntegerLoop => vm::integer_loop(work),
    }
}
