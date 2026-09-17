//! Workspace and trusted language-package discovery.

use std::{
    fs,
    path::{Path, PathBuf},
};

use rils_project::Project;

use crate::AnyError;

pub(crate) struct WorkspaceLoad {
    pub(crate) projects: Vec<Project>,
    pub(crate) errors: Vec<String>,
}

#[cfg(test)]
pub(crate) fn workspace_projects(root: &Path) -> Result<Vec<Project>, AnyError> {
    Ok(workspace_projects_with_language(root, None)?.projects)
}

pub(crate) fn workspace_projects_with_language(
    root: &Path,
    language_root: Option<&Path>,
) -> Result<WorkspaceLoad, AnyError> {
    if language_root.is_some_and(|language_root| same_directory(root, language_root)) {
        return Ok(WorkspaceLoad {
            projects: Vec::new(),
            errors: Vec::new(),
        });
    }
    let manifest = root.join(rils_project::PROJECT_FILE_NAME);
    if manifest.is_file() {
        return Ok(WorkspaceLoad {
            projects: vec![Project::from_file(manifest)?],
            errors: Vec::new(),
        });
    }

    // A workspace root without a manifest is kept as a legacy project for
    // loose `.rils` files.  Its recursive scan is best-effort, though: a
    // repository can contain fixtures or tooling scripts whose paths are not
    // valid module identifiers.  Do not let one such file prevent explicit
    // nested `rils.toml` projects from loading (for example the examples
    // workspace contains both `task_board` and `telemetry_pipeline`).
    let mut projects = Vec::new();
    let mut errors = Vec::new();
    match Project::from_root(root) {
        Ok(project) => projects.push(project),
        Err(error) => errors.push(format!(
            "failed to load legacy workspace root `{}`: {error}",
            root.display()
        )),
    }
    let mut manifests = Vec::new();
    collect_nested_project_manifests(root, &mut manifests, &mut errors);
    manifests.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });

    let mut configured_roots = Vec::new();
    for manifest in manifests {
        let project_root = manifest
            .parent()
            .expect("project manifest always has a parent");
        if configured_roots
            .iter()
            .any(|configured_root: &PathBuf| project_root.starts_with(configured_root))
            || language_root
                .is_some_and(|language_root| same_directory(project_root, language_root))
        {
            continue;
        }
        match Project::from_file(&manifest) {
            Ok(project) => {
                configured_roots.push(project.root().to_path_buf());
                projects.push(project);
            }
            Err(error) => errors.push(format!("failed to load `{}`: {error}", manifest.display())),
        }
    }
    Ok(WorkspaceLoad { projects, errors })
}

pub(crate) fn language_package_root() -> Result<PathBuf, String> {
    resolve_language_package_root(
        std::env::var_os("RILS_SYSROOT")
            .map(PathBuf::from)
            .as_deref(),
        std::env::current_exe().ok().as_deref(),
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/rils_builtins/stdlib"),
    )
}

pub(crate) fn resolve_language_package_root(
    configured: Option<&Path>,
    executable: Option<&Path>,
    development_root: &Path,
) -> Result<PathBuf, String> {
    if let Some(configured) = configured {
        let package = configured.join("packages/rils_stdlib");
        if package.join(rils_project::PROJECT_FILE_NAME).is_file() {
            return Ok(package);
        }
        return Err(format!(
            "RILS_SYSROOT does not contain packages/rils_stdlib/rils.toml: {}",
            configured.display()
        ));
    }
    executable
        .into_iter()
        .flat_map(|path| {
            path.parent()
                .map(|parent| {
                    vec![
                        parent.join("../lib/rils/packages/rils_stdlib"),
                        parent.join("../sysroot/packages/rils_stdlib"),
                    ]
                })
                .unwrap_or_default()
        })
        .chain(std::iter::once(development_root.to_path_buf()))
        .map(|path| path.components().collect::<PathBuf>())
        .find(|path| path.join(rils_project::PROJECT_FILE_NAME).is_file())
        .ok_or_else(|| {
            "Rils standard library package was not found beside the analyzer executable".to_owned()
        })
}

fn same_directory(left: &Path, right: &Path) -> bool {
    crate::support::path_to_file_uri(left) == crate::support::path_to_file_uri(right)
}

pub(crate) fn collect_nested_project_manifests(
    root: &Path,
    manifests: &mut Vec<PathBuf>,
    errors: &mut Vec<String>,
) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("failed to scan `{}`: {error}", root.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!(
                    "failed to read entry in `{}`: {error}",
                    root.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                errors.push(format!("failed to inspect `{}`: {error}", path.display()));
                continue;
            }
        };
        if !file_type.is_dir() {
            continue;
        }
        if matches!(
            entry.file_name().to_str(),
            Some(".git" | ".rils" | "target" | "node_modules" | "dist" | "Library")
        ) {
            continue;
        }
        let manifest = path.join(rils_project::PROJECT_FILE_NAME);
        if manifest.is_file() {
            manifests.push(manifest);
            continue;
        }
        collect_nested_project_manifests(&path, manifests, errors);
    }
}
