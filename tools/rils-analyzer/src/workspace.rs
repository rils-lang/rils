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
    if language_root.is_some_and(|language_root| root == language_root) {
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

    let mut projects = vec![Project::from_root(root)?];
    let mut manifests = Vec::new();
    collect_nested_project_manifests(root, &mut manifests)?;
    manifests.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });

    let mut configured_roots = Vec::new();
    let mut errors = Vec::new();
    for manifest in manifests {
        let project_root = manifest
            .parent()
            .expect("project manifest always has a parent");
        if configured_roots
            .iter()
            .any(|configured_root: &PathBuf| project_root.starts_with(configured_root))
            || language_root.is_some_and(|language_root| project_root == language_root)
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
    if let Some(configured) = std::env::var_os("RILS_SYSROOT").map(PathBuf::from) {
        let package = configured.join("packages/rils_stdlib");
        if package.join(rils_project::PROJECT_FILE_NAME).is_file() {
            return Ok(package);
        }
        return Err(format!(
            "RILS_SYSROOT does not contain packages/rils_stdlib/rils.toml: {}",
            configured.display()
        ));
    }
    let executable = std::env::current_exe().ok();
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
        .chain(std::iter::once(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/rils_builtins/stdlib"),
        ))
        .map(|path| path.components().collect::<PathBuf>())
        .find(|path| path.join(rils_project::PROJECT_FILE_NAME).is_file())
        .ok_or_else(|| {
            "Rils standard library package was not found beside the analyzer executable".to_owned()
        })
}

fn collect_nested_project_manifests(
    root: &Path,
    manifests: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_dir() {
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
        collect_nested_project_manifests(&path, manifests)?;
    }
    Ok(())
}
