use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use rils::{BytecodeModule, Engine};

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => repl(),
        [command, input] if command == "compile" => compile_file(input, None),
        [command, input, option, output] if command == "compile" && option == "-o" => {
            compile_file(input, Some(output))
        }
        [command, path] if command == "verify" => verify_bytecode(path),
        [command, path] if command == "run" => run_bytecode(path),
        [path]
            if Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "rilbc") =>
        {
            run_bytecode(path)
        }
        [path] => run_source_file(path),
        _ => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  rils [script.rils]");
    eprintln!("  rils compile <script.rils> [-o output.rilbc]");
    eprintln!("  rils verify <module.rilbc>");
    eprintln!("  rils run <module.rilbc>");
}

fn compile_file(input: &str, output: Option<&str>) -> ExitCode {
    let source = match fs::read_to_string(input) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("failed to read `{input}`: {error}");
            return ExitCode::FAILURE;
        }
    };
    let module = match rils::compile_file(input) {
        Ok(module) => module,
        Err(error) => {
            eprintln!("{}", error.render(input, &source));
            return ExitCode::FAILURE;
        }
    };
    let output = output.map_or_else(
        || PathBuf::from(input).with_extension("rilbc"),
        PathBuf::from,
    );
    match module.write_file(&output) {
        Ok(()) => {
            println!("wrote {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn verify_bytecode(path: &str) -> ExitCode {
    match BytecodeModule::read_file(path) {
        Ok(module) => {
            println!(
                "verified {path}: {} functions, {} instructions, {} imports",
                module.function_count(),
                module.instruction_count(),
                module.imports().len()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_bytecode(path: &str) -> ExitCode {
    let module = match BytecodeModule::read_file(path) {
        Ok(module) => module,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    match module.execute() {
        Ok(value) => {
            if value != rils::Value::Unit {
                println!("{value}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_source_file(path: &str) -> ExitCode {
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
