use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "rils",
    version,
    about = "Command-line tooling for the Rils scripting language",
    subcommand_precedence_over_arg = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<CliCommand>,

    #[arg(value_name = "SCRIPT")]
    pub(crate) script: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CliCommand {
    /// Start an interactive Rils session.
    Repl,
    Compile(OutputCommand),
    Verify {
        path: String,
    },
    Run {
        path: String,
    },
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    HostManifest {
        #[command(subcommand)]
        command: HostManifestCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct OutputCommand {
    pub(crate) input: String,

    #[arg(short, long, value_name = "OUTPUT")]
    pub(crate) output: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum LibraryCommand {
    Compile(OutputCommand),
    Verify { path: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum HostManifestCommand {
    Compile(OutputCommand),
    ExportJson(OutputCommand),
    Link {
        input: String,

        #[arg(short, long, value_name = "OUTPUT")]
        output: String,
    },
}

#[cfg(test)]
#[path = "../tests/unit/args.rs"]
mod tests;
