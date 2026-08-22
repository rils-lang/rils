use clap::{Parser, ValueEnum};

#[derive(Clone, Debug, ValueEnum)]
pub(crate) enum Scenario {
    VmIntegerLoop,
}

#[derive(Debug, Parser)]
#[command(
    name = "rils-bench",
    about = "Repeatable performance benchmarks for Rils"
)]
pub(crate) struct Args {
    /// Scenario to measure.
    #[arg(value_enum)]
    pub(crate) scenario: Scenario,

    /// Number of unmeasured warm-up executions.
    #[arg(long, default_value_t = 3)]
    pub(crate) warmups: usize,

    /// Number of measured executions.
    #[arg(long, default_value_t = 20)]
    pub(crate) iterations: usize,

    /// Loop iterations performed by the VM in each sample.
    #[arg(long, default_value_t = 100_000)]
    pub(crate) work: usize,
}
