use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use rils::{BytecodeModule, Engine, HostContract, RilsLibrary};

pub(crate) fn run(arguments: Vec<String>) -> ExitCode {
    match arguments.as_slice() {
        [] => crate::repl::run(),
        [command, input] if command == "compile" => compile_file(input, None),
        [command, input, option, output] if command == "compile" && option == "-o" => {
            compile_file(input, Some(output))
        }
        [command, path] if command == "verify" => verify_bytecode(path),
        [command, path] if command == "run" => run_bytecode(path),
        [command, action, input] if command == "library" && action == "compile" => {
            compile_library(input, None)
        }
        [command, action, input, option, output]
            if command == "library" && action == "compile" && option == "-o" =>
        {
            compile_library(input, Some(output))
        }
        [command, action, path] if command == "library" && action == "verify" => {
            verify_library(path)
        }
        [command, action, input] if command == "host-manifest" && action == "compile" => {
            compile_host_manifest(input, None)
        }
        [command, action, input, option, output]
            if command == "host-manifest" && action == "compile" && option == "-o" =>
        {
            compile_host_manifest(input, Some(output))
        }
        [command, action, input] if command == "host-manifest" && action == "export-json" => {
            export_host_manifest_json(input, None)
        }
        [command, action, input, option, output]
            if command == "host-manifest" && action == "export-json" && option == "-o" =>
        {
            export_host_manifest_json(input, Some(output))
        }
        [command, action, input, option, output]
            if command == "host-manifest" && action == "link" && option == "-o" =>
        {
            link_host_manifests(input, output)
        }
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
    eprintln!("  rils library compile <directory|rils.toml|source.rils> [-o output.rilslib]");
    eprintln!("  rils library verify <library.rilslib>");
    eprintln!("  rils host-manifest compile <contract.json> [-o contract.rilhm]");
    eprintln!("  rils host-manifest export-json <contract.rilhm> [-o contract.json]");
    eprintln!("  rils host-manifest link <directory|rils.toml> -o contract.rilhm");
}

fn compile_library(input: &str, output: Option<&str>) -> ExitCode {
    let library = match rils::compile_library(input) {
        Ok(library) => library,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let output = output.map(PathBuf::from).unwrap_or_else(|| {
        let input = Path::new(input);
        let directory = if input.is_dir() {
            input
        } else {
            input.parent().unwrap_or_else(|| Path::new("."))
        };
        directory.join(format!("{}.rilslib", library.name()))
    });
    match library.write_file(&output) {
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

fn verify_library(path: &str) -> ExitCode {
    match RilsLibrary::read_file(path) {
        Ok(library) => {
            println!(
                "verified {path}: library `{}`, {} functions, {} instructions",
                library.name(),
                library.module().function_count(),
                library.module().instruction_count()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn link_host_manifests(input: &str, output: &str) -> ExitCode {
    let input = Path::new(input);
    let paths = if input.is_file() && input.file_name().is_some_and(|name| name == "rils.toml") {
        rils::Project::from_file(input).map(|project| project.host_manifests().to_vec())
    } else if input.is_dir() {
        let configured = input.join("rils.toml");
        if configured.is_file() {
            rils::Project::from_file(configured).map(|project| project.host_manifests().to_vec())
        } else {
            rils::Project::discover_manifest_directory(input)
        }
    } else {
        Err(rils::ProjectError {
            message: format!(
                "`{}` is not a manifest directory or rils.toml",
                input.display()
            ),
        })
    };
    let paths = match paths {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("failed to discover host manifest fragments: {error}");
            return ExitCode::FAILURE;
        }
    };
    if paths.is_empty() {
        eprintln!("no .rilhm fragments found for `{}`", input.display());
        return ExitCode::FAILURE;
    }
    let mut linked: Option<HostContract> = None;
    for path in &paths {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("failed to read `{}`: {error}", path.display());
                return ExitCode::FAILURE;
            }
        };
        let fragment = match HostContract::from_manifest_bytes(&bytes) {
            Ok(fragment) => fragment,
            Err(error) => {
                eprintln!("invalid host manifest `{}`: {error}", path.display());
                return ExitCode::FAILURE;
            }
        };
        if let Some(linked) = &mut linked {
            if let Err(error) = linked.merge(&fragment) {
                eprintln!("cannot merge host manifest `{}`: {error}", path.display());
                return ExitCode::FAILURE;
            }
        } else {
            linked = Some(fragment);
        }
    }
    let linked = linked.expect("non-empty fragment list");
    let bytes = match linked.to_manifest_bytes() {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("failed to encode linked host manifest: {error}");
            return ExitCode::FAILURE;
        }
    };
    match fs::write(output, bytes) {
        Ok(()) => {
            println!(
                "linked {} fragments into {} ({})",
                paths.len(),
                output,
                linked.contract_hash()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to write `{output}`: {error}");
            ExitCode::FAILURE
        }
    }
}

fn compile_host_manifest(input: &str, output: Option<&str>) -> ExitCode {
    let json = match fs::read_to_string(input) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("failed to read `{input}`: {error}");
            return ExitCode::FAILURE;
        }
    };
    let contract = match HostContract::from_manifest_json(&json) {
        Ok(contract) => contract,
        Err(error) => {
            eprintln!("invalid JSON host manifest `{input}`: {error}");
            return ExitCode::FAILURE;
        }
    };
    let manifest = match contract.to_manifest_bytes() {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("failed to encode host manifest: {error}");
            return ExitCode::FAILURE;
        }
    };
    let output = output.map_or_else(
        || PathBuf::from(input).with_extension("rilhm"),
        PathBuf::from,
    );
    match fs::write(&output, manifest) {
        Ok(()) => {
            println!("wrote {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to write `{}`: {error}", output.display());
            ExitCode::FAILURE
        }
    }
}

fn export_host_manifest_json(input: &str, output: Option<&str>) -> ExitCode {
    let bytes = match fs::read(input) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("failed to read `{input}`: {error}");
            return ExitCode::FAILURE;
        }
    };
    let contract = match HostContract::from_manifest_bytes(&bytes) {
        Ok(contract) => contract,
        Err(error) => {
            eprintln!("invalid binary host manifest `{input}`: {error}");
            return ExitCode::FAILURE;
        }
    };
    let json = match contract.to_manifest_json() {
        Ok(json) => json,
        Err(error) => {
            eprintln!("failed to encode JSON host manifest: {error}");
            return ExitCode::FAILURE;
        }
    };
    let output = output.map_or_else(
        || PathBuf::from(input).with_extension("json"),
        PathBuf::from,
    );
    match fs::write(&output, json) {
        Ok(()) => {
            println!("wrote {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to write `{}`: {error}", output.display());
            ExitCode::FAILURE
        }
    }
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
            if let Some(source_name) = module.source_name(error.span.source) {
                eprintln!(
                    "{error} at {source_name}:{}..{}",
                    error.span.start, error.span.end
                );
            } else {
                eprintln!("{error}");
            }
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
