use std::hint::black_box;

use rils::{BytecodeModule, ExecutionLimits, Value};

use super::Benchmark;

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
    let limits = ExecutionLimits::new(
        usize::try_from(work)
            .ok()
            .and_then(|work| work.checked_mul(32))
            .unwrap_or(usize::MAX),
        ExecutionLimits::default().max_call_depth,
    );
    verify_result(&module, limits, expected)?;

    Ok(Benchmark {
        name: "vm-integer-loop",
        run: Box::new(move || verify_result(&module, limits, expected)),
    })
}

fn verify_result(
    module: &BytecodeModule,
    limits: ExecutionLimits,
    expected: i64,
) -> Result<(), String> {
    let result = module
        .execute_with_limits(limits)
        .map_err(|error| error.to_string())?;
    if result != Value::I64(expected) {
        return Err(format!("expected {expected}, received {result}"));
    }
    black_box(result);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::integer_loop;

    #[test]
    fn integer_loop_returns_the_expected_sum() {
        integer_loop(1_000)
            .expect("build VM integer loop")
            .run()
            .expect("run VM integer loop");
    }
}
