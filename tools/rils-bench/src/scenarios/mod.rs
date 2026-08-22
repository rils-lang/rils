mod vm;

use crate::args::Scenario;

pub(crate) struct Benchmark {
    pub(crate) name: &'static str,
    run: Box<dyn Fn() -> Result<(), String>>,
}

impl Benchmark {
    pub(crate) fn run(&self) -> Result<(), String> {
        (self.run)()
    }
}

pub(crate) fn build(scenario: Scenario, work: usize) -> Result<Benchmark, String> {
    match scenario {
        Scenario::VmIntegerLoop => vm::integer_loop(work),
    }
}
