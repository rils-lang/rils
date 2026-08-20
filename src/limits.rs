/// Resource limits shared by the AST interpreter and bytecode VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionLimits {
    /// Maximum number of interpreter steps or bytecode instructions per invocation.
    pub max_steps: usize,
    /// Maximum number of simultaneously active script function calls.
    pub max_call_depth: usize,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_steps: 1_000_000,
            max_call_depth: 1_024,
        }
    }
}

impl ExecutionLimits {
    pub const fn new(max_steps: usize, max_call_depth: usize) -> Self {
        Self {
            max_steps,
            max_call_depth,
        }
    }
}
