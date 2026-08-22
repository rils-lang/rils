use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::Parser;
use rils::{BytecodeModule, Engine, HostContract, Project, ProjectKind, RilsLibrary};

use crate::args::{Cli, CliCommand, HostManifestCommand, LibraryCommand};

pub(crate) fn run(arguments: Vec<String>) -> ExitCode {
    let cli = Cli::try_parse_from(std::iter::once("rils".to_owned()).chain(arguments))
        .unwrap_or_else(|error| error.exit());
    match (cli.command, cli.script) {
        (None, None) => crate::repl::run(),
        (None, Some(path))
            if Path::new(&path)
                .extension()
                .is_some_and(|extension| extension == "rilbc") =>
        {
            run_bytecode(&path)
        }
        (None, Some(path)) => run_source_file(&path),
        (Some(CliCommand::Compile(command)), None) => {
            compile_file(&command.input, command.output.as_deref())
        }
        (Some(CliCommand::Verify { path }), None) => verify_bytecode(&path),
        (Some(CliCommand::Run { path }), None) => run_path(&path),
        (Some(CliCommand::Library { command }), None) => match command {
            LibraryCommand::Compile(command) => {
                compile_library(&command.input, command.output.as_deref())
            }
            LibraryCommand::Verify { path } => verify_library(&path),
        },
        (Some(CliCommand::HostManifest { command }), None) => match command {
            HostManifestCommand::Compile(command) => {
                compile_host_manifest(&command.input, command.output.as_deref())
            }
            HostManifestCommand::ExportJson(command) => {
                export_host_manifest_json(&command.input, command.output.as_deref())
            }
            HostManifestCommand::Link { input, output } => link_host_manifests(&input, &output),
        },
        (_, Some(_)) => unreachable!("clap does not allow a command and script together"),
    }
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

fn run_path(path: &str) -> ExitCode {
    let path = Path::new(path);
    if path.is_dir() {
        return run_project(path);
    }
    run_bytecode(path.to_string_lossy().as_ref())
}

fn run_project(directory: &Path) -> ExitCode {
    let manifest = directory.join("rils.toml");
    if !manifest.is_file() {
        eprintln!(
            "`{}` is not a Rils project directory: missing rils.toml",
            directory.display()
        );
        return ExitCode::FAILURE;
    }
    let project = match Project::from_file(&manifest) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("failed to load `{}`: {error}", manifest.display());
            return ExitCode::FAILURE;
        }
    };
    if project.kind() != ProjectKind::Bin {
        eprintln!(
            "project `{}` is a library and cannot be run",
            project.name()
        );
        return ExitCode::FAILURE;
    }
    let Some(entry) = project
        .source_roots()
        .iter()
        .map(|source_root| source_root.join("main.rils"))
        .find(|candidate| candidate.is_file())
    else {
        eprintln!("project `{}` has no main.rils entry point", project.name());
        return ExitCode::FAILURE;
    };
    run_source_file(&entry)
}

fn run_source_file(path: impl AsRef<Path>) -> ExitCode {
    let path = path.as_ref();
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("failed to read `{}`: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };

    match Engine::new().eval_file(path) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error.render(path.to_string_lossy().as_ref(), &source));
            ExitCode::FAILURE
        }
    }
}
