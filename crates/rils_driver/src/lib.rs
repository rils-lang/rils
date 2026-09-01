//! Backend-independent project discovery, source loading, and analysis preparation.

mod error;
mod loading;
mod sources;

pub use error::DriverError;
pub use loading::{discover_entry_project, load_file_modules};
pub use sources::ProjectSources;
