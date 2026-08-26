use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub(crate) root: PathBuf,
    pub(crate) manifest_path: Option<PathBuf>,
    pub(crate) name: String,
    pub(crate) kind: ProjectKind,
    pub(crate) prelude: Option<PathBuf>,
    pub(crate) source_roots: Vec<PathBuf>,
    pub(crate) host_manifests: Vec<PathBuf>,
    pub(crate) unity_binding_assemblies: Vec<String>,
    pub(crate) dependencies: BTreeMap<String, ProjectDependency>,
    pub(crate) modules: BTreeMap<String, ProjectFile>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectKind {
    Bin,
    Lib,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectFile {
    pub path: PathBuf,
    pub module_path: String,
}

/// A path-based Rils library made available under a crate name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectDependency {
    pub name: String,
    pub root: PathBuf,
    pub source_roots: Vec<PathBuf>,
    pub prelude: Option<PathBuf>,
}
