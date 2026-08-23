mod args;
mod metrics;
mod runner;
mod scenarios;

fn main() -> std::process::ExitCode {
    runner::run()
}
