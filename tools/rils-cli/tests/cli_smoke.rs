use std::{
    fs,
    process::{self, Command},
    time::{SystemTime, UNIX_EPOCH},
};

fn temporary_directory() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock precedes the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("rils-cli-{}/{}", process::id(), nonce));
    fs::create_dir_all(&directory).expect("create temporary CLI test directory");
    directory
}

#[test]
fn compiles_verifies_and_runs_a_bytecode_module() {
    let directory = temporary_directory();
    let source = directory.join("main.rils");
    let bytecode = directory.join("main.rilbc");
    fs::write(&source, "40 + 2\n").expect("write CLI test source");

    let cli = env!("CARGO_BIN_EXE_rils");
    let compile = Command::new(cli)
        .args([
            "compile",
            source.to_str().unwrap(),
            "-o",
            bytecode.to_str().unwrap(),
        ])
        .output()
        .expect("run rils compile");
    assert!(
        compile.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let verify = Command::new(cli)
        .args(["verify", bytecode.to_str().unwrap()])
        .output()
        .expect("run rils verify");
    assert!(
        verify.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );

    let run = Command::new(cli)
        .args(["run", bytecode.to_str().unwrap()])
        .output()
        .expect("run rils run");
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");

    fs::remove_dir_all(&directory).expect("remove temporary CLI test directory");
}

#[test]
fn displays_help_without_arguments() {
    let cli = env!("CARGO_BIN_EXE_rils");
    let help = Command::new(cli)
        .output()
        .expect("run rils without arguments");
    assert!(
        help.status.success(),
        "help failed: {}",
        String::from_utf8_lossy(&help.stderr)
    );
    let output = String::from_utf8_lossy(&help.stdout);
    assert!(output.contains("Usage: rils"));
    assert!(output.contains("repl"));
}

#[test]
fn runs_a_project_directory() {
    let directory = temporary_directory();
    let source_directory = directory.join("src");
    fs::create_dir_all(&source_directory).expect("create project source directory");
    fs::write(
        directory.join("rils.toml"),
        "[project]\nname = \"cli_project\"\nscript_paths = [\"src\"]\n",
    )
    .expect("write project manifest");
    fs::write(
        source_directory.join("main.rils"),
        "fn main() -> i32 { 42 }\n",
    )
    .expect("write project entry");

    let cli = env!("CARGO_BIN_EXE_rils");
    let run = Command::new(cli)
        .args(["run", directory.to_str().unwrap()])
        .output()
        .expect("run rils project directory");
    assert!(
        run.status.success(),
        "project run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    fs::remove_dir_all(&directory).expect("remove temporary CLI test directory");
}
