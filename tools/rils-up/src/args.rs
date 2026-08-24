use std::{
    env,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};

use crate::{config, install, self_update, shim};

#[derive(Debug, Parser)]
#[command(
    name = "rils-up",
    version,
    about = "Install and select Rils toolchains"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Download a stable or explicitly versioned toolchain.
    Install {
        #[arg(default_value = "stable")]
        toolchain: String,
    },
    /// Download the newest stable toolchain and select it globally.
    Update,
    /// Select the global default toolchain.
    Default { toolchain: String },
    /// List installed toolchains.
    List,
    /// Display the executable selected for a managed command.
    Which {
        #[arg(default_value = "rils")]
        command: String,
    },
    /// Manage the project-local .rils-version selection.
    Override {
        #[command(subcommand)]
        command: OverrideCommand,
    },
    /// Remove an installed toolchain.
    Uninstall { toolchain: String },
    /// Print the Rils installation directory.
    Home,
    /// Manage the independently versioned rils-up executable.
    #[command(name = "self")]
    Self_ {
        #[command(subcommand)]
        command: SelfCommand,
    },
    #[command(name = "__complete-self-update", hide = true)]
    CompleteSelfUpdate {
        #[arg(long)]
        home: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum OverrideCommand {
    /// Pin the current project directory to one installed version.
    Set { toolchain: String },
    /// Remove the override in the current directory.
    Unset,
    /// Show the override selected from the current directory hierarchy.
    Show,
}

#[derive(Debug, Subcommand)]
enum SelfCommand {
    /// Update rils-up without changing any installed Rils toolchain.
    Update,
}

pub(crate) fn run() -> Result<u8, String> {
    let cli = Cli::parse();
    if let Command::CompleteSelfUpdate { home } = cli.command {
        self_update::complete_scheduled_update(&home)?;
        return Ok(0);
    }
    let home = config::rils_home()?;
    self_update::refresh_proxies_if_managed(&home);
    match cli.command {
        Command::Install { toolchain } => {
            let result = install::install(&home, &toolchain)?;
            if result.installed {
                println!("Installed Rils {}", result.version);
            } else {
                println!("Rils {} is already installed", result.version);
            }
            print_path_hint(&home);
        }
        Command::Update => {
            let result = install::install(&home, "stable")?;
            let version = install::set_default(&home, &result.version)?;
            println!("The default Rils toolchain is now {version}");
            print_path_hint(&home);
        }
        Command::Default { toolchain } => {
            let version = install::set_default(&home, &toolchain)?;
            println!("The default Rils toolchain is now {version}");
        }
        Command::List => list(&home)?,
        Command::Which { command } => {
            let (version, path) = shim::resolved_command(&home, &command, None)?;
            println!("{} ({version})", path.display());
        }
        Command::Override { command } => override_command(&home, command)?,
        Command::Uninstall { toolchain } => {
            let version = install::uninstall(&home, &toolchain)?;
            println!("Uninstalled Rils {version}");
        }
        Command::Home => println!("{}", home.display()),
        Command::Self_ { command } => match command {
            SelfCommand::Update => match self_update::update(&home)? {
                self_update::UpdateStatus::Current(version) => {
                    println!("rils-up {version} is already current");
                }
                self_update::UpdateStatus::Updated(version) => {
                    println!("Updated rils-up to {version}");
                }
                self_update::UpdateStatus::Scheduled(version) => {
                    println!("rils-up {version} will finish installing after this process exits");
                }
            },
        },
        Command::CompleteSelfUpdate { .. } => unreachable!(),
    }
    Ok(0)
}

fn list(home: &Path) -> Result<(), String> {
    let settings = config::read_settings(home)?;
    let versions = install::installed_toolchains(home)?;
    if versions.is_empty() {
        println!("No Rils toolchains are installed");
        return Ok(());
    }
    for version in versions {
        let marker = if settings.default.as_deref() == Some(&version) {
            " (default)"
        } else {
            ""
        };
        println!("{version}{marker}");
    }
    Ok(())
}

fn override_command(home: &Path, command: OverrideCommand) -> Result<(), String> {
    let current = env::current_dir()
        .map_err(|error| format!("Could not determine the current directory: {error}"))?;
    match command {
        OverrideCommand::Set { toolchain } => {
            let version = config::validate_toolchain(&toolchain)?;
            install::require_installed(home, &version)?;
            let path = config::set_override(&current, &version)?;
            println!("Pinned {} to Rils {version}", path.display());
        }
        OverrideCommand::Unset => {
            if config::remove_override(&current)? {
                println!("Removed {}", current.join(".rils-version").display());
            } else {
                println!("No override exists in {}", current.display());
            }
        }
        OverrideCommand::Show => match config::find_override(&current)? {
            Some(version) => println!("{version}"),
            None => println!("No project override is active"),
        },
    }
    Ok(())
}

fn print_path_hint(home: &Path) {
    let bin = home.join("bin");
    let in_path = env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .any(|entry| paths_equal(&entry, &bin));
    if !in_path {
        println!(
            "Add {} to PATH to use rils and rils-analyzer",
            bin.display()
        );
    }
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}
