use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    PROJECT_FILE_NAME, ProjectDependency, ProjectFile, ProjectKind, config,
    error::ProjectError,
    files,
    paths::{absolutize, ancestors_within, is_identifier},
};

pub(crate) struct ProjectBuild {
    pub(crate) root: PathBuf,
    pub(crate) manifest_path: Option<PathBuf>,
    pub(crate) name: String,
    pub(crate) kind: ProjectKind,
    pub(crate) prelude: Option<PathBuf>,
    pub(crate) source_roots: Vec<PathBuf>,
    pub(crate) configured_manifests: Vec<PathBuf>,
    pub(crate) configured_manifest_dirs: Vec<PathBuf>,
    pub(crate) unity_binding_assemblies: Vec<String>,
    pub(crate) dependencies: BTreeMap<String, ProjectDependency>,
}

pub use crate::types::Project;

impl Project {
    pub fn discover_manifest_directory(
        path: impl AsRef<Path>,
    ) -> Result<Vec<PathBuf>, ProjectError> {
        files::collect_manifest_files(&absolutize(path.as_ref())?)
    }

    pub fn discover(
        path: impl AsRef<Path>,
        workspace_root: Option<&Path>,
    ) -> Result<Self, ProjectError> {
        let path = absolutize(path.as_ref())?;
        if let Some(project) = Self::discover_configured(&path, workspace_root)? {
            return Ok(project);
        }
        let start = if path.is_dir() {
            path.clone()
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };
        let root = workspace_root
            .filter(|root| path.starts_with(root))
            .map(absolutize)
            .transpose()?
            .unwrap_or_else(|| {
                if path.is_file() && path.file_name().is_some_and(|name| name == "mod.rils") {
                    start.parent().unwrap_or(&start).to_path_buf()
                } else {
                    start
                }
            });
        Self::from_root(root)
    }

    pub fn discover_configured(
        path: impl AsRef<Path>,
        workspace_root: Option<&Path>,
    ) -> Result<Option<Self>, ProjectError> {
        let path = absolutize(path.as_ref())?;
        let start = if path.is_dir() {
            path
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };
        ancestors_within(&start, workspace_root)
            .map(|directory| directory.join(PROJECT_FILE_NAME))
            .find(|candidate| candidate.is_file())
            .map(Self::from_file)
            .transpose()
    }

    pub fn for_legacy_entry(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let root = absolutize(path.as_ref())?;
        let root = if root.is_dir() {
            root
        } else {
            root.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| is_identifier(name))
            .unwrap_or("project")
            .to_owned();
        let host_manifests =
            files::discover_default_host_manifests(&root, std::slice::from_ref(&root))?;
        Ok(Self {
            root: root.clone(),
            manifest_path: None,
            name,
            kind: ProjectKind::Bin,
            prelude: None,
            source_roots: vec![root],
            host_manifests,
            unity_binding_assemblies: Vec::new(),
            dependencies: BTreeMap::new(),
            modules: BTreeMap::new(),
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        Self::build(config::load_project(absolutize(path.as_ref())?)?)
    }

    pub fn from_root(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let root = absolutize(path.as_ref())?;
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| is_identifier(name))
            .unwrap_or("project")
            .to_owned();
        Self::build(ProjectBuild {
            root: root.clone(),
            manifest_path: None,
            name,
            kind: ProjectKind::Bin,
            prelude: None,
            source_roots: vec![root],
            configured_manifests: Vec::new(),
            configured_manifest_dirs: Vec::new(),
            unity_binding_assemblies: Vec::new(),
            dependencies: BTreeMap::new(),
        })
    }

    fn build(input: ProjectBuild) -> Result<Self, ProjectError> {
        let ProjectBuild {
            root,
            manifest_path,
            name,
            kind,
            prelude,
            source_roots,
            configured_manifests,
            configured_manifest_dirs,
            unity_binding_assemblies,
            dependencies,
        } = input;
        let host_manifests =
            if configured_manifests.is_empty() && configured_manifest_dirs.is_empty() {
                files::discover_default_host_manifests(&root, &source_roots)?
            } else {
                files::discover_configured_host_manifests(
                    configured_manifests,
                    &configured_manifest_dirs,
                )?
            };
        let mut modules = BTreeMap::new();
        for source_root in &source_roots {
            files::collect_modules(&mut modules, source_root, "")?;
        }
        for dependency in dependencies.values() {
            for source_root in &dependency.source_roots {
                files::collect_modules(&mut modules, source_root, &dependency.name)?;
            }
        }
        Ok(Self {
            root,
            manifest_path,
            name,
            kind,
            prelude,
            source_roots,
            host_manifests,
            unity_binding_assemblies,
            dependencies,
            modules,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn manifest_path(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn kind(&self) -> ProjectKind {
        self.kind
    }
    pub fn requires_entry(&self) -> bool {
        self.kind == ProjectKind::Bin
    }
    pub fn prelude(&self) -> Option<&Path> {
        self.prelude.as_deref()
    }
    pub fn source_roots(&self) -> &[PathBuf] {
        &self.source_roots
    }
    pub fn host_manifest(&self) -> Option<&Path> {
        (self.host_manifests.len() == 1).then(|| self.host_manifests[0].as_path())
    }
    pub fn host_manifests(&self) -> &[PathBuf] {
        &self.host_manifests
    }
    pub fn unity_binding_assemblies(&self) -> &[String] {
        &self.unity_binding_assemblies
    }
    pub fn dependencies(&self) -> impl ExactSizeIterator<Item = &ProjectDependency> {
        self.dependencies.values()
    }
    pub fn dependency(&self, name: &str) -> Option<&ProjectDependency> {
        self.dependencies.get(name)
    }
    pub fn modules(&self) -> impl ExactSizeIterator<Item = &ProjectFile> {
        self.modules.values()
    }
    pub fn module(&self, path: &str) -> Option<&ProjectFile> {
        self.modules.get(path)
    }
    pub fn module_for_file(&self, path: impl AsRef<Path>) -> Option<&ProjectFile> {
        let path = absolutize(path.as_ref()).ok()?;
        self.modules.values().find(|file| {
            file.path == path
                || fs::canonicalize(&file.path)
                    .ok()
                    .zip(fs::canonicalize(&path).ok())
                    .is_some_and(|(left, right)| left == right)
        })
    }
}
