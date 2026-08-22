use std::{
    hint::black_box,
    process::ExitCode,
    time::{Duration, Instant},
};

use clap::Parser;

use crate::{
    args::Args,
    metrics::{AllocationMetrics, TrackingAllocator, begin_measurement, finish_measurement},
    scenarios,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

pub(crate) fn run() -> ExitCode {
    let args = Args::parse();
    if args.iterations == 0 {
        eprintln!("--iterations must be greater than zero");
        return ExitCode::FAILURE;
    }
    if args.work == 0 {
        eprintln!("--work must be greater than zero");
        return ExitCode::FAILURE;
    }
    let benchmark = match scenarios::build(args.scenario, args.work) {
        Ok(benchmark) => benchmark,
        Err(error) => {
            eprintln!("failed to prepare benchmark: {error}");
            return ExitCode::FAILURE;
        }
    };
    for _ in 0..args.warmups {
        if let Err(error) = benchmark.run() {
            eprintln!("benchmark warm-up failed: {error}");
            return ExitCode::FAILURE;
        }
    }

    let mut samples = Vec::with_capacity(args.iterations);
    for _ in 0..args.iterations {
        let measurement = begin_measurement();
        let started = Instant::now();
        if let Err(error) = benchmark.run() {
            eprintln!("benchmark execution failed: {error}");
            return ExitCode::FAILURE;
        }
        samples.push(Sample {
            elapsed: started.elapsed(),
            allocations: finish_measurement(measurement),
        });
    }
    black_box(&samples);
    print_result(benchmark.name, args.work, args.warmups, &samples);
    ExitCode::SUCCESS
}

struct Sample {
    elapsed: Duration,
    allocations: AllocationMetrics,
}

fn print_result(name: &str, work: usize, warmups: usize, samples: &[Sample]) {
    let mut elapsed = samples
        .iter()
        .map(|sample| sample.elapsed.as_nanos())
        .collect::<Vec<_>>();
    elapsed.sort_unstable();
    let median = elapsed[elapsed.len() / 2];
    let p95 = elapsed[((elapsed.len() * 95).div_ceil(100)).saturating_sub(1)];
    let allocation_count = median_metric(samples, |metrics| metrics.allocation_count);
    let allocated_bytes = median_metric(samples, |metrics| metrics.allocated_bytes);
    let deallocated_bytes = median_metric(samples, |metrics| metrics.deallocated_bytes);
    let peak_live_bytes = median_metric(samples, |metrics| metrics.peak_live_bytes);
    println!(
        "{{\"schema_version\":1,\"scenario\":\"{name}\",\"work\":{work},\"warmups\":{warmups},\"samples\":{},\"median_ns\":{median},\"p95_ns\":{p95},\"median_allocations\":{allocation_count},\"median_allocated_bytes\":{allocated_bytes},\"median_deallocated_bytes\":{deallocated_bytes},\"median_peak_live_bytes\":{peak_live_bytes}}}",
        samples.len()
    );
}

fn median_metric(samples: &[Sample], metric: impl Fn(AllocationMetrics) -> u64) -> u64 {
    let mut values = samples
        .iter()
        .map(|sample| metric(sample.allocations))
        .collect::<Vec<_>>();
    values.sort_unstable();
    values[values.len() / 2]
}
