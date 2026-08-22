use std::hint::black_box;

use rils::{BytecodeModule, ExecutionLimits, Value};

use crate::args::IntegerType;

use super::Benchmark;

pub(super) fn counter_loop(work: usize, integer_type: IntegerType) -> Result<Benchmark, String> {
    let limit = typed_limit(work, integer_type)?;
    let source = format!(
        "fn count_to(limit: {type_name}) -> {type_name} {{\n    let mut index: {type_name} = 0;\n    while index < limit {{\n        index = index + 1;\n    }}\n    index\n}}\ncount_to({limit})\n",
        type_name = integer_type.name(),
    );
    let module = rils::compile(&source).map_err(|error| error.to_string())?;
    let limits = execution_limits(work);
    let expected = expected_value(work, integer_type)?;
    verify_result(&module, limits, expected.clone())?;

    Ok(Benchmark {
        name: "vm-counter-loop",
        integer_type: Some(integer_type.name()),
        run: Box::new(move || verify_result(&module, limits, expected.clone())),
    })
}

pub(super) fn integer_loop(work: usize) -> Result<Benchmark, String> {
    let work = i64::try_from(work).map_err(|_| "work exceeds i64::MAX".to_owned())?;
    let expected = work
        .checked_mul(work.saturating_sub(1))
        .and_then(|value| value.checked_div(2))
        .ok_or_else(|| "work produces an i64 overflow".to_owned())?;
    let source = format!(
        "fn sum_to(limit: i64) -> i64 {{\n    let mut index = 0;\n    let mut total = 0;\n    while index < limit {{\n        total = total + index;\n        index = index + 1;\n    }}\n    total\n}}\nsum_to({work})\n"
    );
    let module = rils::compile(&source).map_err(|error| error.to_string())?;
    let limits = execution_limits(usize::try_from(work).unwrap_or(usize::MAX));
    verify_result(&module, limits, Value::I64(expected))?;

    Ok(Benchmark {
        name: "vm-integer-loop",
        integer_type: Some("i64"),
        run: Box::new(move || verify_result(&module, limits, Value::I64(expected))),
    })
}

fn verify_result(
    module: &BytecodeModule,
    limits: ExecutionLimits,
    expected: Value,
) -> Result<(), String> {
    let result = module
        .execute_with_limits(limits)
        .map_err(|error| error.to_string())?;
    if result != expected {
        return Err(format!("expected {expected}, received {result}"));
    }
    black_box(result);
    Ok(())
}

fn execution_limits(work: usize) -> ExecutionLimits {
    ExecutionLimits::new(
        work.saturating_mul(32),
        ExecutionLimits::default().max_call_depth,
    )
}

fn typed_limit(work: usize, integer_type: IntegerType) -> Result<String, String> {
    match integer_type {
        IntegerType::I32 => i32::try_from(work).map(|value| value.to_string()),
        IntegerType::U32 => u32::try_from(work).map(|value| value.to_string()),
        IntegerType::I64 => i64::try_from(work).map(|value| value.to_string()),
        IntegerType::U64 => u64::try_from(work).map(|value| value.to_string()),
        IntegerType::Usize => Ok(work.to_string()),
    }
    .map_err(|_| format!("work {work} does not fit in {}", integer_type.name()))
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
    use super::{IntegerType, integer_loop};

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
