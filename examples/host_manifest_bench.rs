use std::hint::black_box;
use std::time::{Duration, Instant};

use rils::{
    FunctionSignature, HOST_MANIFEST_JSON_MAX_BYTES, HOST_MANIFEST_MAX_BYTES,
    HOST_MANIFEST_MAX_FUNCTIONS, HostContract, IntegerType, Type,
};

const DEFAULT_COUNTS: &[usize] = &[10_000, 20_000, 50_000, HOST_MANIFEST_MAX_FUNCTIONS];
const PARSE_SAMPLES: usize = 5;

fn main() -> Result<(), String> {
    let counts = parse_counts()?;
    println!(
        "host manifest benchmark (release={}, binary_max_bytes={}, json_max_bytes={}, parse_samples={})",
        !cfg!(debug_assertions),
        HOST_MANIFEST_MAX_BYTES,
        HOST_MANIFEST_JSON_MAX_BYTES,
        PARSE_SAMPLES
    );
    println!(
        "functions,build_ms,binary_bytes,binary_bytes_per_function,binary_serialize_ms,binary_parse_median_ms,binary_parse_min_ms,hash_ms,json_bytes,json_serialize_ms,json_parse_median_ms,json_unhashed_parse_median_ms"
    );

    for function_count in counts {
        benchmark(function_count)?;
    }
    Ok(())
}

fn parse_counts() -> Result<Vec<usize>, String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return Ok(DEFAULT_COUNTS.to_vec());
    }
    args.into_iter()
        .map(|argument| {
            let count = argument
                .parse::<usize>()
                .map_err(|error| format!("invalid function count `{argument}`: {error}"))?;
            if count == 0 || count > HOST_MANIFEST_MAX_FUNCTIONS {
                return Err(format!(
                    "function count must be between 1 and {HOST_MANIFEST_MAX_FUNCTIONS}"
                ));
            }
            Ok(count)
        })
        .collect()
}

fn benchmark(function_count: usize) -> Result<(), String> {
    let started = Instant::now();
    let contract = build_contract(function_count)?;
    let build = started.elapsed();

    let started = Instant::now();
    let binary = contract.to_manifest_bytes()?;
    let binary_serialize = started.elapsed();
    let binary_parse = measure_binary_parse(&binary)?;

    let started = Instant::now();
    black_box(contract.contract_hash());
    let hash = started.elapsed();

    let started = Instant::now();
    let json = contract.to_manifest_json()?;
    let json_serialize = started.elapsed();
    let unhashed_json = without_optional_hash(&json);
    let json_parse = measure_json_parse(&json)?;
    let unhashed_json_parse = measure_json_parse(&unhashed_json)?;

    println!(
        "{function_count},{:.3},{},{:.1},{:.3},{:.3},{:.3},{:.3},{},{:.3},{:.3},{:.3}",
        milliseconds(build),
        binary.len(),
        binary.len() as f64 / function_count as f64,
        milliseconds(binary_serialize),
        milliseconds(binary_parse.median),
        milliseconds(binary_parse.min),
        milliseconds(hash),
        json.len(),
        milliseconds(json_serialize),
        milliseconds(json_parse.median),
        milliseconds(unhashed_json_parse.median),
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct ParseSample {
    median: Duration,
    min: Duration,
}

fn measure_binary_parse(manifest: &[u8]) -> Result<ParseSample, String> {
    measure_parse(|| HostContract::from_manifest_bytes(black_box(manifest)))
}

fn measure_json_parse(json: &str) -> Result<ParseSample, String> {
    measure_parse(|| HostContract::from_manifest_json(black_box(json)))
}

fn measure_parse(
    mut parse: impl FnMut() -> Result<HostContract, String>,
) -> Result<ParseSample, String> {
    let mut samples = Vec::with_capacity(PARSE_SAMPLES);
    for _ in 0..PARSE_SAMPLES {
        let started = Instant::now();
        let parsed = parse()?;
        samples.push(started.elapsed());
        black_box(parsed);
    }
    samples.sort_unstable();
    Ok(ParseSample {
        median: samples[PARSE_SAMPLES / 2],
        min: samples[0],
    })
}

fn without_optional_hash(json: &str) -> String {
    json.lines()
        .filter(|line| !line.contains("\"contract_hash\"") && !line.contains("\"hash_algorithm\""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_contract(function_count: usize) -> Result<HostContract, String> {
    let mut contract = HostContract::new();
    contract.register_module("unity_engine::generated", 1)?;
    for index in 0..function_count {
        contract.register_function(
            index as u64 + 1,
            format!("unity_engine::generated::function_{index:05}"),
            FunctionSignature::fixed(
                vec![
                    Type::Integer(IntegerType::I32),
                    Type::Integer(IntegerType::I32),
                ],
                Type::Integer(IntegerType::I32),
            ),
            "unity.generated",
        )?;
    }
    Ok(contract)
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
