//! Project discovery and module catalog for Rils.

mod config;
mod error;
mod files;
mod paths;
mod project;
mod types;

pub use error::ProjectError;
pub use project::Project;
pub use types::{ProjectDependency, ProjectFile, ProjectKind};

pub const PROJECT_FILE_NAME: &str = "rils.toml";
pub const DEFAULT_HOST_MANIFEST_PATHS: &[&str] =
    &[".rils/host.rilhm", "host.rilhm", "rils-host.rilhm"];
pub const DEFAULT_HOST_MANIFEST_DIR: &str = ".rils/manifest";
