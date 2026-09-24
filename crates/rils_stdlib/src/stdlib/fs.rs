//! Filesystem functions implemented by the host Rust standard library.

use rils_builtins_macros::decl_rils;

#[decl_rils(std::fs)]
mod native {
    use super::super::{prelude::Result, string::String};
    use crate::stdlib::io::Error;

    fn error(path: &str, cause: std::io::Error) -> Error {
        Error {
            kind: cause.kind(),
            message: cause.to_string(),
            path: Some(path.to_owned()),
        }
    }

    fn convert<T>(path: &str, result: std::io::Result<T>) -> Result<T, Error> {
        match result {
            Ok(value) => Result::Ok(value),
            Err(cause) => Result::Err(error(path, cause)),
        }
    }

    /// Reads a UTF-8 file.
    #[rils_fn]
    pub fn read_to_string(path: String) -> Result<String, rils_stdlib::stdlib::io::Error> {
        let path: std::string::String = path.into();
        match convert(&path, std::fs::read_to_string(&path)) {
            Result::Ok(value) => Result::Ok(value.into()),
            Result::Err(error) => Result::Err(error),
        }
    }

    /// Writes a UTF-8 file.
    #[rils_fn]
    pub fn write(path: String, contents: String) -> Result<(), rils_stdlib::stdlib::io::Error> {
        let path: std::string::String = path.into();
        let contents: std::string::String = contents.into();
        convert(&path, std::fs::write(&path, contents))
    }

    /// Appends to a UTF-8 file.
    #[rils_fn]
    pub fn append(path: String, contents: String) -> Result<(), rils_stdlib::stdlib::io::Error> {
        use std::io::Write;
        let path: std::string::String = path.into();
        let contents: std::string::String = contents.into();
        let result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(contents.as_bytes()));
        convert(&path, result)
    }

    /// Checks whether a path exists.
    #[rils_fn]
    pub fn try_exists(path: String) -> Result<bool, rils_stdlib::stdlib::io::Error> {
        let path: std::string::String = path.into();
        convert(&path, std::path::Path::new(&path).try_exists())
    }

    /// Creates a directory tree.
    #[rils_fn]
    pub fn create_dir_all(path: String) -> Result<(), rils_stdlib::stdlib::io::Error> {
        let path: std::string::String = path.into();
        convert(&path, std::fs::create_dir_all(&path))
    }

    /// Removes a file.
    #[rils_fn]
    pub fn remove_file(path: String) -> Result<(), rils_stdlib::stdlib::io::Error> {
        let path: std::string::String = path.into();
        convert(&path, std::fs::remove_file(&path))
    }

    /// Removes an empty directory.
    #[rils_fn]
    pub fn remove_dir(path: String) -> Result<(), rils_stdlib::stdlib::io::Error> {
        let path: std::string::String = path.into();
        convert(&path, std::fs::remove_dir(&path))
    }

    /// Lists directory entries.
    #[rils_fn]
    pub fn read_dir(path: String) -> Result<Vec<String>, rils_stdlib::stdlib::io::Error> {
        let path: std::string::String = path.into();
        let result = std::fs::read_dir(&path).and_then(|entries| {
            let mut paths = entries
                .map(|entry| {
                    entry.and_then(|entry| {
                        entry
                            .path()
                            .to_str()
                            .map(|path| String::from(path.to_owned()))
                            .ok_or_else(|| {
                                std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "non-UTF-8 path",
                                )
                            })
                    })
                })
                .collect::<std::io::Result<Vec<_>>>()?;
            paths.sort();
            Ok(paths)
        });
        convert(&path, result)
    }
}

pub use native::{
    append, create_dir_all, read_dir, read_to_string, remove_dir, remove_file, try_exists, write,
};
