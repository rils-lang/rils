use clap::Parser;

use super::{Cli, CliCommand, HostManifestCommand};

#[test]
fn parses_nested_host_manifest_link_command() {
    let cli = Cli::try_parse_from([
        "rils",
        "host-manifest",
        "link",
        "project",
        "--output",
        "host.rilhm",
    ])
    .expect("parse host-manifest link");

    assert!(matches!(
        cli.command,
        Some(CliCommand::HostManifest {
            command: HostManifestCommand::Link { input, output }
        }) if input == "project" && output == "host.rilhm"
    ));
}
