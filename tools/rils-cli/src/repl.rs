use std::{
    io::{self, Write},
    process::ExitCode,
};

use rils::Engine;

pub(crate) fn run() -> ExitCode {
    println!("Rils {} — type `exit` to leave", env!("CARGO_PKG_VERSION"));
    let mut engine = Engine::new();
    let stdin = io::stdin();

    loop {
        print!("> ");
        if io::stdout().flush().is_err() {
            return ExitCode::FAILURE;
        }

        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                eprintln!("failed to read input: {error}");
                return ExitCode::FAILURE;
            }
        }

        if matches!(line.trim(), "exit" | "quit") {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }

        match engine.eval(&line) {
            Ok(value) if value != rils::Value::Unit => println!("{value}"),
            Ok(_) => {}
            Err(error) => eprintln!("{}", error.render("<repl>", &line)),
        }
    }

    ExitCode::SUCCESS
}
