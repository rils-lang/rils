use std::{
    env, fs,
    io::{self, Write},
    process::ExitCode,
};

use rils::Engine;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => repl(),
        [path] => run_file(path),
        _ => {
            eprintln!("usage: rils [script.rils]");
            ExitCode::from(2)
        }
    }
}

fn run_file(path: &str) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("failed to read `{path}`: {error}");
            return ExitCode::FAILURE;
        }
    };

    match Engine::new().eval_file(path) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error.render(path, &source));
            ExitCode::FAILURE
        }
    }
}

fn repl() -> ExitCode {
    println!("Rils 0.1.0 — type `exit` to leave");
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
