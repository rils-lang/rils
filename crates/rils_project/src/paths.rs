use std::path::{Component, Path, PathBuf};

use crate::error::{ProjectError, project_error};

pub(crate) fn absolutize(path: &Path) -> Result<PathBuf, ProjectError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| project_error(format!("cannot resolve current directory: {error}")))?
    };
    Ok(normalize_path(&absolute))
}

pub(crate) fn normalize_under_root(
    root: &Path,
    path: &Path,
    label: &str,
) -> Result<PathBuf, ProjectError> {
    let absolute = absolutize(&root.join(path))?;
    if !absolute.starts_with(root) {
        return Err(project_error(format!(
            "{label} `{}` escapes project root `{}`",
            path.display(),
            root.display()
        )));
    }
    Ok(absolute)
}

pub(crate) fn ancestors_within<'a>(
    start: &'a Path,
    boundary: Option<&'a Path>,
) -> impl Iterator<Item = &'a Path> {
    start.ancestors().take_while(move |directory| {
        boundary.is_none_or(|boundary| directory.starts_with(boundary))
    })
}

pub(crate) fn is_identifier(value: &str) -> bool {
    if matches!(
        value,
        "crate" | "self" | "super" | "core" | "std" | "prelude"
    ) {
        return false;
    }
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}
