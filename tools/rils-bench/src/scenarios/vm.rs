use std::{fs, hint::black_box, path::PathBuf};

use rils::{BytecodeHost, BytecodeModule, ExecutionLimits, Value};

use crate::args::IntegerType;

use super::Benchmark;

pub(super) fn empty_call(work: usize) -> Result<Benchmark, String> {
    let value = i32::try_from(work).map_err(|_| "work does not fit in i32".to_owned())?;
    benchmark_case(
        "vm_empty_call.rils",
        "echo",
        work,
        Value::I32(value),
        Value::I32(value),
        "vm-empty-call",
        Some("i32"),
    )
}

pub(super) fn counter_loop(work: usize, integer_type: IntegerType) -> Result<Benchmark, String> {
    let expected = expected_value(work, integer_type)?;
    benchmark_case(
        counter_case_name(integer_type),
        "count_to",
        work,
        expected.clone(),
        expected,
        "vm-counter-loop",
        Some(integer_type.name()),
    )
}

pub(super) fn integer_loop(work: usize) -> Result<Benchmark, String> {
    let work = i64::try_from(work).map_err(|_| "work exceeds i64::MAX".to_owned())?;
    let expected = work
        .checked_mul(work.saturating_sub(1))
        .and_then(|value| value.checked_div(2))
        .ok_or_else(|| "work produces an i64 overflow".to_owned())?;
    benchmark_case(
        "vm_integer_loop.rils",
        "sum_to",
        usize::try_from(work).unwrap_or(usize::MAX),
        Value::I64(work),
        Value::I64(expected),
        "vm-integer-loop",
        Some("i64"),
    )
}

fn benchmark_case(
    case_name: &'static str,
    function: &'static str,
    work: usize,
    argument: Value,
    expected: Value,
    name: &'static str,
    integer_type: Option<&'static str>,
) -> Result<Benchmark, String> {
    let source = read_case(case_name)?;
    let module = rils::compile(&source).map_err(|error| error.to_string())?;
    let host = BytecodeHost::standard();
    let limits = execution_limits(work);
    verify_result(
        &module,
        &host,
        function,
        limits,
        argument.clone(),
        expected.clone(),
    )?;

    Ok(Benchmark {
        name,
        case: case_name,
        integer_type,
        run: Box::new(move || {
            verify_result(
                &module,
                &host,
                function,
                limits,
                argument.clone(),
                expected.clone(),
            )
        }),
    })
}

fn verify_result(
    module: &BytecodeModule,
    host: &BytecodeHost,
    function: &str,
    limits: ExecutionLimits,
    argument: Value,
    expected: Value,
) -> Result<(), String> {
    let result = module
        .call_with_host_and_limits(function, vec![argument], host, limits)
        .map_err(|error| error.to_string())?;
    if result != expected {
        return Err(format!("expected {expected}, received {result}"));
    }
    black_box(result);
    Ok(())
}

fn read_case(name: &str) -> Result<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("cases")
        .join(name);
    fs::read_to_string(&path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))
}

fn execution_limits(work: usize) -> ExecutionLimits {
    ExecutionLimits::new(
        work.saturating_mul(32),
        ExecutionLimits::default().max_call_depth,
    )
}

fn counter_case_name(integer_type: IntegerType) -> &'static str {
    match integer_type {
        IntegerType::I32 => "vm_counter_i32.rils",
        IntegerType::U32 => "vm_counter_u32.rils",
        IntegerType::I64 => "vm_counter_i64.rils",
        IntegerType::U64 => "vm_counter_u64.rils",
        IntegerType::Usize => "vm_counter_usize.rils",
    }
}

fn expected_value(work: usize, integer_type: IntegerType) -> Result<Value, String> {
    match integer_type {
        IntegerType::I32 => i32::try_from(work).map(Value::I32),
        IntegerType::U32 => u32::try_from(work).map(Value::U32),
        IntegerType::I64 => i64::try_from(work).map(Value::I64),
        IntegerType::U64 => u64::try_from(work).map(Value::U64),
        IntegerType::Usize => Ok(Value::Usize(work)),
    }
    .map_err(|_| format!("work {work} does not fit in {}", integer_type.name()))
}

#[cfg(test)]
mod tests {
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
}
