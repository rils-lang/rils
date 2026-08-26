use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    PROJECT_FILE_NAME, ProjectDependency, ProjectKind,
    error::{ProjectError, project_error},
    paths::{absolutize, is_identifier, normalize_under_root},
    project::ProjectBuild,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectConfig {
    project: Option<ProjectSection>,
    host: Option<HostSection>,
    #[serde(default)]
    dependencies: BTreeMap<String, DependencySection>,
    #[serde(default)]
    lib: Option<LibSection>,
    unity: Option<UnitySection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSection {
    name: String,
    #[serde(default)]
    src: SourcePaths,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SourcePaths {
    One(PathBuf),
    Many(Vec<PathBuf>),
}

impl Default for SourcePaths {
    fn default() -> Self {
        Self::Many(Vec::new())
    }
}

impl SourcePaths {
    fn into_paths(self) -> Vec<PathBuf> {
        match self {
            Self::One(path) => vec![path],
            Self::Many(paths) => paths,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibSection {
    #[serde(default)]
    prelude: Option<PathBuf>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostSection {
    manifest: Option<PathBuf>,
    #[serde(default)]
    manifests: Vec<PathBuf>,
    #[serde(default)]
    manifest_dirs: Vec<PathBuf>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DependencySection {
    path: PathBuf,
    #[serde(default)]
    prelude: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitySection {
    bindings: Option<UnityBindingsSection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnityBindingsSection {
    #[serde(default)]
    assemblies: Vec<String>,
    #[serde(default, rename = "manifest_dir")]
    _manifest_dir: Option<PathBuf>,
    #[serde(default, rename = "csharp_output")]
    _csharp_output: Option<PathBuf>,
}

pub(crate) fn load_project(path: PathBuf) -> Result<ProjectBuild, ProjectError> {
    let root = path
        .parent()
        .ok_or_else(|| project_error("rils.toml has no parent directory"))?
        .to_path_buf();
    let config = read_config(&path)?;
    let project = config
        .project
        .ok_or_else(|| project_error("rils.toml is missing `[project]`"))?;
    validate_project_name(&project.name)?;
    let source_roots = source_paths(project.src)
        .into_iter()
        .map(|path| normalize_under_root(&root, &path, "script path"))
        .collect::<Result<Vec<_>, _>>()?;
    let host = config.host.unwrap_or_default();
    let mut manifests = host.manifests;
    if let Some(manifest) = host.manifest {
        manifests.push(manifest);
    }
    let configured_manifests = manifests
        .into_iter()
        .map(|path| normalize_under_root(&root, &path, "host manifest"))
        .collect::<Result<Vec<_>, _>>()?;
    let configured_manifest_dirs = host
        .manifest_dirs
        .into_iter()
        .map(|path| normalize_under_root(&root, &path, "host manifest directory"))
        .collect::<Result<Vec<_>, _>>()?;
    let kind = if config.lib.is_some()
        || !source_roots
            .iter()
            .any(|root| root.join("main.rils").is_file())
    {
        ProjectKind::Lib
    } else {
        ProjectKind::Bin
    };
    let prelude = config
        .lib
        .and_then(|library| library.prelude)
        .map(|path| normalize_under_root(&root, &path, "library prelude"))
        .transpose()?;
    let unity_binding_assemblies = config
        .unity
        .and_then(|unity| unity.bindings)
        .map_or_else(Vec::new, |bindings| bindings.assemblies);
    validate_unity_binding_assemblies(&unity_binding_assemblies)?;
    Ok(ProjectBuild {
        root: root.clone(),
        manifest_path: Some(path),
        name: project.name,
        kind,
        prelude,
        source_roots,
        configured_manifests,
        configured_manifest_dirs,
        unity_binding_assemblies,
        dependencies: load_dependencies(&root, config.dependencies)?,
    })
}

fn read_config(path: &Path) -> Result<ProjectConfig, ProjectError> {
    let source = fs::read_to_string(path)
        .map_err(|error| project_error(format!("failed to read `{}`: {error}", path.display())))?;
    toml::from_str(&source)
        .map_err(|error| project_error(format!("invalid `{}`: {error}", path.display())))
}

fn load_dependencies(
    root: &Path,
    configured: BTreeMap<String, DependencySection>,
) -> Result<BTreeMap<String, ProjectDependency>, ProjectError> {
    let mut dependencies = BTreeMap::new();
    for (name, dependency) in configured {
        validate_project_name(&name)?;
        let dependency_root = absolutize(&root.join(dependency.path))?;
        if !dependency_root.is_dir() {
            return Err(project_error(format!(
                "dependency `{name}` path `{}` is not a directory",
                dependency_root.display()
            )));
        }
        let metadata = read_dependency_metadata(&dependency_root)?;
        if let Some(package_name) = &metadata.package_name
            && package_name != &name
        {
            return Err(project_error(format!(
                "dependency alias `{name}` does not match package name `{}`",
                package_name
            )));
        }
        let prelude = if dependency.prelude {
            find_prelude(&dependency_root, &metadata.source_roots)
        } else {
            metadata
                .prelude
                .map(|path| normalize_under_root(&dependency_root, &path, "dependency prelude"))
                .transpose()?
        };
        dependencies.insert(
            name.clone(),
            ProjectDependency {
                name,
                root: dependency_root,
                source_roots: metadata.source_roots,
                prelude,
            },
        );
    }
    Ok(dependencies)
}

struct DependencyMetadata {
    source_roots: Vec<PathBuf>,
    package_name: Option<String>,
    prelude: Option<PathBuf>,
}
fn read_dependency_metadata(root: &Path) -> Result<DependencyMetadata, ProjectError> {
    let path = root.join(PROJECT_FILE_NAME);
    if !path.is_file() {
        return Ok(DependencyMetadata {
            source_roots: vec![root.to_path_buf()],
            package_name: None,
            prelude: None,
        });
    }
    let config = read_config(&path)?;
    let Some(project) = config.project else {
        return Ok(DependencyMetadata {
            source_roots: vec![root.to_path_buf()],
            package_name: None,
            prelude: None,
        });
    };
    validate_project_name(&project.name)?;
    let source_roots = source_paths(project.src)
        .into_iter()
        .map(|path| normalize_under_root(root, &path, "dependency script path"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DependencyMetadata {
        source_roots,
        package_name: Some(project.name),
        prelude: config.lib.and_then(|library| library.prelude),
    })
}
fn source_paths(paths: SourcePaths) -> Vec<PathBuf> {
    let paths = paths.into_paths();
    if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths
    }
}
fn find_prelude(root: &Path, source_roots: &[PathBuf]) -> Option<PathBuf> {
    source_roots
        .iter()
        .map(|root| root.join("prelude.rils"))
        .chain([root.join("prelude.rils")])
        .find(|path| path.is_file())
}
pub(crate) fn validate_project_name(name: &str) -> Result<(), ProjectError> {
    if is_identifier(name) {
        Ok(())
    } else {
        Err(project_error(format!(
            "project name `{name}` must be a valid Rils identifier"
        )))
    }
}
fn validate_unity_binding_assemblies(assemblies: &[String]) -> Result<(), ProjectError> {
    let mut seen = HashSet::new();
    for assembly in assemblies {
        if assembly.is_empty()
            || assembly
                .split('.')
                .any(|segment| segment.is_empty() || !is_identifier(segment))
        {
            return Err(project_error(format!(
                "`{assembly}` is not a valid Unity assembly name"
            )));
        }
        if !seen.insert(assembly) {
            return Err(project_error(format!(
                "Unity binding assembly `{assembly}` is configured more than once"
            )));
        }
    }
    Ok(())
}
