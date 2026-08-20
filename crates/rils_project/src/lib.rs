use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

pub const PROJECT_FILE_NAME: &str = "rils.toml";
pub const DEFAULT_HOST_MANIFEST_PATHS: &[&str] =
    &[".rils/host.rilhm", "host.rilhm", "rils-host.rilhm"];
pub const DEFAULT_HOST_MANIFEST_DIR: &str = ".rils/manifest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    root: PathBuf,
    manifest_path: Option<PathBuf>,
    name: String,
    kind: ProjectKind,
    prelude: Option<PathBuf>,
    source_roots: Vec<PathBuf>,
    host_manifests: Vec<PathBuf>,
    dependencies: BTreeMap<String, ProjectDependency>,
    modules: BTreeMap<String, ProjectFile>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectError {
    pub message: String,
}

struct ProjectBuild {
    root: PathBuf,
    manifest_path: Option<PathBuf>,
    name: String,
    kind: ProjectKind,
    prelude: Option<PathBuf>,
    source_roots: Vec<PathBuf>,
    configured_manifests: Vec<PathBuf>,
    configured_manifest_dirs: Vec<PathBuf>,
    dependencies: BTreeMap<String, ProjectDependency>,
}

struct DependencyMetadata {
    source_roots: Vec<PathBuf>,
    package_name: Option<String>,
    prelude: Option<PathBuf>,
}

impl Project {
    pub fn discover_manifest_directory(
        path: impl AsRef<Path>,
    ) -> Result<Vec<PathBuf>, ProjectError> {
        let path = absolutize(path.as_ref())?;
        collect_manifest_files(&path)
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
        let path = absolutize(path.as_ref())?;
        let root = if path.is_dir() {
            path
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| is_identifier(name))
            .unwrap_or("project")
            .to_owned();
        let host_manifests = discover_default_host_manifests(&root, std::slice::from_ref(&root))?;
        Ok(Self {
            root: root.clone(),
            manifest_path: None,
            name,
            kind: ProjectKind::Bin,
            prelude: None,
            source_roots: vec![root],
            host_manifests,
            dependencies: BTreeMap::new(),
            modules: BTreeMap::new(),
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let path = absolutize(path.as_ref())?;
        let root = path
            .parent()
            .ok_or_else(|| project_error("rils.toml has no parent directory"))?
            .to_path_buf();
        let source = fs::read_to_string(&path).map_err(|error| {
            project_error(format!("failed to read `{}`: {error}", path.display()))
        })?;
        let config: ProjectConfig = toml::from_str(&source)
            .map_err(|error| project_error(format!("invalid `{}`: {error}", path.display())))?;
        let project = config
            .project
            .ok_or_else(|| project_error("rils.toml is missing `[project]`"))?;
        validate_project_name(&project.name)?;
        let source_paths = if project.script_paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            project.script_paths
        };
        let source_roots = source_paths
            .into_iter()
            .map(|source_root| normalize_under_root(&root, &source_root, "script path"))
            .collect::<Result<Vec<_>, _>>()?;
        let host = config.host.unwrap_or_default();
        let mut configured_manifests = host.manifests;
        if let Some(manifest) = host.manifest {
            configured_manifests.push(manifest);
        }
        let configured_manifests = configured_manifests
            .into_iter()
            .map(|manifest| normalize_under_root(&root, &manifest, "host manifest"))
            .collect::<Result<Vec<_>, _>>()?;
        let configured_manifest_dirs = host
            .manifest_dirs
            .into_iter()
            .map(|directory| normalize_under_root(&root, &directory, "host manifest directory"))
            .collect::<Result<Vec<_>, _>>()?;
        let library = config.lib;
        let kind = if library.is_some()
            || !source_roots
                .iter()
                .any(|source_root| source_root.join("main.rils").is_file())
        {
            ProjectKind::Lib
        } else {
            ProjectKind::Bin
        };
        let prelude = library
            .and_then(|library| library.prelude)
            .map(|path| normalize_under_root(&root, &path, "library prelude"))
            .transpose()?;
        let dependencies = load_dependencies(&root, config.dependencies)?;
        Self::build(ProjectBuild {
            root,
            manifest_path: Some(path),
            name: project.name,
            kind,
            prelude,
            source_roots,
            configured_manifests,
            configured_manifest_dirs,
            dependencies,
        })
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
            dependencies,
        } = input;
        let host_manifests =
            if configured_manifests.is_empty() && configured_manifest_dirs.is_empty() {
                discover_default_host_manifests(&root, &source_roots)?
            } else {
                discover_configured_host_manifests(configured_manifests, &configured_manifest_dirs)?
            };
        let mut modules = BTreeMap::new();
        for source_root in &source_roots {
            collect_modules(&mut modules, source_root, "")?;
        }
        for dependency in dependencies.values() {
            for source_root in &dependency.source_roots {
                collect_modules(&mut modules, source_root, &dependency.name)?;
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

fn collect_modules(
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
        // `prelude.rils` is injected by the dependency loader and is not a
        // user-addressable module path.
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectConfig {
    project: Option<ProjectSection>,
    host: Option<HostSection>,
    #[serde(default)]
    dependencies: BTreeMap<String, DependencySection>,
    #[serde(default)]
    lib: Option<LibSection>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSection {
    name: String,
    #[serde(default)]
    script_paths: Vec<PathBuf>,
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
        let source_roots = metadata.source_roots;
        let package_name = metadata.package_name;
        if let Some(package_name) = package_name {
            if package_name != name {
                return Err(project_error(format!(
                    "dependency alias `{name}` does not match package name `{package_name}`"
                )));
            }
        }
        let prelude = if dependency.prelude {
            find_prelude(&dependency_root, &source_roots)
        } else if let Some(path) = metadata.prelude {
            Some(normalize_under_root(
                &dependency_root,
                &path,
                "dependency prelude",
            )?)
        } else {
            None
        };
        dependencies.insert(
            name.clone(),
            ProjectDependency {
                name,
                root: dependency_root,
                source_roots,
                prelude,
            },
        );
    }
    Ok(dependencies)
}

fn read_dependency_metadata(root: &Path) -> Result<DependencyMetadata, ProjectError> {
    let config_path = root.join(PROJECT_FILE_NAME);
    if !config_path.is_file() {
        return Ok(DependencyMetadata {
            source_roots: vec![root.to_path_buf()],
            package_name: None,
            prelude: None,
        });
    }
    let source = fs::read_to_string(&config_path).map_err(|error| {
        project_error(format!(
            "failed to read `{}`: {error}",
            config_path.display()
        ))
    })?;
    let config: ProjectConfig = toml::from_str(&source)
        .map_err(|error| project_error(format!("invalid `{}`: {error}", config_path.display())))?;
    let Some(project) = config.project else {
        return Ok(DependencyMetadata {
            source_roots: vec![root.to_path_buf()],
            package_name: None,
            prelude: None,
        });
    };
    validate_project_name(&project.name)?;
    let paths = if project.script_paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        project.script_paths
    };
    let source_roots = paths
        .into_iter()
        .map(|path| normalize_under_root(root, &path, "dependency script path"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DependencyMetadata {
        source_roots,
        package_name: Some(project.name),
        prelude: config.lib.and_then(|library| library.prelude),
    })
}

fn find_prelude(root: &Path, source_roots: &[PathBuf]) -> Option<PathBuf> {
    source_roots
        .iter()
        .map(|source_root| source_root.join("prelude.rils"))
        .chain([root.join("prelude.rils")])
        .find(|path| path.is_file())
}

fn discover_default_host_manifests(
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

fn discover_configured_host_manifests(
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

fn collect_manifest_files(root: &Path) -> Result<Vec<PathBuf>, ProjectError> {
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

fn module_path(source_root: &Path, file: &Path) -> Result<String, ProjectError> {
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
                Some(".git" | ".rils" | "target" | "node_modules" | "dist")
            ) {
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

fn normalize_under_root(root: &Path, path: &Path, label: &str) -> Result<PathBuf, ProjectError> {
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

fn absolutize(path: &Path) -> Result<PathBuf, ProjectError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| project_error(format!("cannot resolve current directory: {error}")))?
    };
    Ok(normalize_path(&absolute))
}

fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;

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

fn ancestors_within<'a>(
    start: &'a Path,
    boundary: Option<&'a Path>,
) -> impl Iterator<Item = &'a Path> {
    start.ancestors().take_while(move |directory| {
        boundary.is_none_or(|boundary| directory.starts_with(boundary))
    })
}

fn validate_project_name(name: &str) -> Result<(), ProjectError> {
    if is_identifier(name) {
        Ok(())
    } else {
        Err(project_error(format!(
            "project name `{name}` must be a valid Rils identifier"
        )))
    }
}

fn is_identifier(value: &str) -> bool {
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

fn project_error(message: impl Into<String>) -> ProjectError {
    ProjectError {
        message: message.into(),
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ProjectError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_project() -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rils-project-test-{}-{unique}", std::process::id()))
    }

    #[test]
    fn validates_project_names() {
        assert!(validate_project_name("game_scripts").is_ok());
        assert!(validate_project_name("game-scripts").is_err());
        assert!(validate_project_name("crate").is_err());
    }

    #[test]
    fn maps_flat_and_nested_files_to_modules() {
        let root = Path::new("scripts");
        assert_eq!(
            module_path(root, Path::new("scripts/gameplay/player.rils")).unwrap(),
            "gameplay::player"
        );
        assert_eq!(
            module_path(root, Path::new("scripts/gameplay/mod.rils")).unwrap(),
            "gameplay"
        );
    }

    #[test]
    fn loads_configured_script_roots_and_manifest() {
        let root = temporary_project();
        let scripts = root.join("Assets/Res/rils-script");
        fs::create_dir_all(scripts.join("gameplay")).unwrap();
        fs::create_dir_all(root.join(".rils")).unwrap();
        fs::write(scripts.join("a.rils"), "let answer = 42;").unwrap();
        fs::write(scripts.join("gameplay/mod.rils"), "pub fn start() {}").unwrap();
        fs::write(root.join(".rils/host.rilhm"), b"manifest").unwrap();
        fs::write(
            root.join(PROJECT_FILE_NAME),
            r#"
                [project]
                name = "unity_game"
                script_paths = ["Assets/Res/rils-script"]

                [host]
                manifest = ".rils/host.rilhm"
            "#,
        )
        .unwrap();

        let project = Project::discover(scripts.join("a.rils"), None).unwrap();
        assert_eq!(project.name(), "unity_game");
        assert_eq!(project.module("a").unwrap().path, scripts.join("a.rils"));
        assert!(project.module("gameplay").is_some());
        assert_eq!(
            project.host_manifest(),
            Some(root.join(".rils/host.rilhm").as_path())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_recursive_manifest_fragments_in_stable_order() {
        let root = temporary_project();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join(".rils/manifest/unity")).unwrap();
        fs::create_dir_all(root.join(".rils/manifest/project")).unwrap();
        fs::write(root.join("scripts/main.rils"), "fn main() {}").unwrap();
        fs::write(root.join(".rils/manifest/unity/physics.rilhm"), b"a").unwrap();
        fs::write(root.join(".rils/manifest/project/game.rilhm"), b"b").unwrap();
        fs::write(
            root.join(PROJECT_FILE_NAME),
            "[project]\nname = \"game\"\nscript_paths = [\"scripts\"]\n",
        )
        .unwrap();

        let project = Project::from_file(root.join(PROJECT_FILE_NAME)).unwrap();
        assert_eq!(project.host_manifests().len(), 2);
        assert!(project.host_manifests()[0] < project.host_manifests()[1]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn supports_explicit_manifest_files_and_directories() {
        let root = temporary_project();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join("generated/modules")).unwrap();
        fs::write(root.join("scripts/main.rils"), "fn main() {}").unwrap();
        fs::write(root.join("generated/project.rilhm"), b"a").unwrap();
        fs::write(root.join("generated/modules/unity.rilhm"), b"b").unwrap();
        fs::write(
            root.join(PROJECT_FILE_NAME),
            r#"
                [project]
                name = "game"
                script_paths = ["scripts"]

                [host]
                manifests = ["generated/project.rilhm"]
                manifest_dirs = ["generated/modules"]
            "#,
        )
        .unwrap();

        let project = Project::from_file(root.join(PROJECT_FILE_NAME)).unwrap();
        assert_eq!(project.host_manifests().len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_path_dependencies_and_prelude() {
        let root = temporary_project();
        let dependency = root.join("Packages/rils_for_unity");
        fs::create_dir_all(dependency.join("src")).unwrap();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(
            dependency.join("rils.toml"),
            "[project]\nname = \"rils_for_unity\"\nscript_paths = [\"src\"]\n",
        )
        .unwrap();
        fs::write(dependency.join("src/behaviour.rils"), "pub fn awake() {}").unwrap();
        fs::write(dependency.join("src/prelude.rils"), "").unwrap();
        fs::write(
            root.join("rils.toml"),
            "[project]\nname = \"game\"\nscript_paths = [\"scripts\"]\n\n[dependencies.rils_for_unity]\npath = \"Packages/rils_for_unity\"\nprelude = true\n",
        )
        .unwrap();

        let project = Project::from_file(root.join(PROJECT_FILE_NAME)).unwrap();
        let dependency = project.dependency("rils_for_unity").unwrap();
        assert_eq!(dependency.source_roots, vec![dependency.root.join("src")]);
        assert_eq!(
            dependency.prelude,
            Some(dependency.root.join("src/prelude.rils"))
        );
        assert!(project.module("rils_for_unity::behaviour").is_some());
        assert_eq!(project.dependencies().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn records_library_prelude() {
        let root = temporary_project();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/prelude.rils"),
            "pub fn value() -> i32 { 42 }",
        )
        .unwrap();
        fs::write(root.join("src/module.rils"), "pub fn other() -> i32 { 1 }").unwrap();
        fs::write(
            root.join(PROJECT_FILE_NAME),
            "[project]\nname = \"sample\"\nscript_paths = [\"src\"]\n\n[lib]\nprelude = \"src/prelude.rils\"\n",
        )
        .unwrap();

        let project = Project::from_file(root.join(PROJECT_FILE_NAME)).unwrap();
        assert_eq!(
            project.prelude(),
            Some(root.join("src/prelude.rils").as_path())
        );
        fs::remove_dir_all(root).unwrap();
    }
}
