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
