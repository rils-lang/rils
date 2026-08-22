use clap::{Parser, ValueEnum};

#[derive(Clone, Debug, ValueEnum)]
pub(crate) enum Scenario {
    VmCounterLoop,
    VmIntegerLoop,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum IntegerType {
    I32,
    U32,
    I64,
    U64,
    Usize,
}

impl IntegerType {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::Usize => "usize",
        }
    }
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

    /// Integer type used by the vm-counter-loop scenario.
    #[arg(long, value_enum, default_value_t = IntegerType::I64)]
    pub(crate) integer_type: IntegerType,
}
