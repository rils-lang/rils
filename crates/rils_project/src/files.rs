use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    DEFAULT_HOST_MANIFEST_DIR, DEFAULT_HOST_MANIFEST_PATHS, PROJECT_FILE_NAME, ProjectFile,
    error::{ProjectError, project_error},
    paths::is_identifier,
};

pub(crate) fn collect_modules(
    modules: &mut BTreeMap<String, ProjectFile>,
    source_root: &Path,
    prefix: &str,
) -> Result<(), ProjectError> {
    let mut files = Vec::new();
    collect_rils_files(source_root, &mut files).map_err(|error| {
        project_error(format!(
            "failed to scan script path `{}`: {error}",
            source_root.display()
        ))
    })?;
    for path in files {
        if path.file_name().is_some_and(|name| name == "prelude.rils") {
            continue;
        }
        let local_path = module_path(source_root, &path)?;
        let module_path = if prefix.is_empty() {
            local_path
        } else {
            format!("{prefix}::{local_path}")
        };
        let file = ProjectFile {
            path: path.clone(),
            module_path: module_path.clone(),
        };
        if let Some(previous) = modules.insert(module_path.clone(), file) {
            return Err(project_error(format!(
                "module `{module_path}` is provided by both `{}` and `{}`",
                previous.path.display(),
                path.display()
            )));
        }
    }
    Ok(())
}

pub(crate) fn discover_default_host_manifests(
    root: &Path,
    source_roots: &[PathBuf],
) -> Result<Vec<PathBuf>, ProjectError> {
    let default_dir = root.join(DEFAULT_HOST_MANIFEST_DIR);
    if default_dir.is_dir() {
        return collect_manifest_files(&default_dir);
    }
    Ok(std::iter::once(root)
        .chain(source_roots.iter().map(PathBuf::as_path))
        .flat_map(|base| {
            DEFAULT_HOST_MANIFEST_PATHS
                .iter()
                .map(move |relative| base.join(relative))
        })
        .find(|candidate| candidate.is_file())
        .into_iter()
        .collect())
}

pub(crate) fn discover_configured_host_manifests(
    manifests: Vec<PathBuf>,
    directories: &[PathBuf],
) -> Result<Vec<PathBuf>, ProjectError> {
    let mut discovered = manifests;
    for directory in directories {
        discovered.extend(collect_manifest_files(directory)?);
    }
    discovered.sort();
    discovered.dedup();
    Ok(discovered)
}

pub(crate) fn collect_manifest_files(root: &Path) -> Result<Vec<PathBuf>, ProjectError> {
    if !root.is_dir() {
        return Err(project_error(format!(
            "host manifest directory `{}` is not a directory",
            root.display()
        )));
    }
    let mut output = Vec::new();
    collect_files_with_extension(root, "rilhm", &mut output).map_err(|error| {
        project_error(format!(
            "failed to scan host manifest directory `{}`: {error}",
            root.display()
        ))
    })?;
    output.sort();
    Ok(output)
}

pub(crate) fn module_path(source_root: &Path, file: &Path) -> Result<String, ProjectError> {
    let relative = file.strip_prefix(source_root).map_err(|_| {
        project_error(format!(
            "script `{}` is outside source root `{}`",
            file.display(),
            source_root.display()
        ))
    })?;
    let mut segments = relative
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let stem = relative
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| project_error(format!("invalid script filename `{}`", file.display())))?;
    if stem != "mod" {
        segments.push(stem.to_owned());
    }
    if segments.is_empty() || segments.iter().any(|segment| !is_identifier(segment)) {
        return Err(project_error(format!(
            "script `{}` does not map to a valid module path",
            file.display()
        )));
    }
    Ok(segments.join("::"))
}

fn collect_files_with_extension(
    root: &Path,
    extension: &str,
    output: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files_with_extension(&path, extension, output)?;
        } else if file_type.is_file() && path.extension().is_some_and(|actual| actual == extension)
        {
            output.push(path);
        }
    }
    Ok(())
}

fn collect_rils_files(root: &Path, output: &mut Vec<PathBuf>) -> io::Result<()> {
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("script path `{}` is not a directory", root.display()),
        ));
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some(".git" | ".rils" | "target" | "node_modules" | "dist" | "Library")
            ) && !path.join(PROJECT_FILE_NAME).is_file()
            {
                collect_rils_files(&path, output)?;
            }
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "rils")
        {
            output.push(path);
        }
    }
    output.sort();
    Ok(())
}
