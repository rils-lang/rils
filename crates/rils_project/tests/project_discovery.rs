use std::path::{Path, PathBuf};

use rils_project::{PROJECT_FILE_NAME, Project, ProjectKind};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn loads_complete_configured_project() {
    let root = fixture("full_project");
    let project = Project::discover(root.join("scripts/main.rils"), None).unwrap();

    assert_eq!(project.name(), "unity_game");
    assert_eq!(project.kind(), ProjectKind::Lib);
    assert!(project.module("main").is_some());
    assert_eq!(
        project.prelude(),
        Some(root.join("scripts/prelude.rils").as_path())
    );
    assert_eq!(project.host_manifests().len(), 2);
    let dependency = project.dependency("rils_for_unity").unwrap();

    assert_eq!(dependency.source_roots, vec![dependency.root.join("src")]);
    assert_eq!(
        dependency.prelude,
        Some(dependency.root.join("src/prelude.rils"))
    );
    assert!(project.module("rils_for_unity::behaviour").is_some());
    assert_eq!(project.dependencies().len(), 1);
    assert_eq!(
        project.unity_binding_assemblies(),
        ["UnityEngine.CoreModule"]
    );
}

#[test]
fn discovers_default_manifest_fragments_in_stable_order() {
    let root = fixture("default_manifests");
    let project = Project::from_file(root.join(PROJECT_FILE_NAME)).unwrap();

    assert_eq!(project.host_manifests().len(), 2);
    assert!(project.host_manifests()[0] < project.host_manifests()[1]);
}

#[test]
fn legacy_root_skips_nested_configured_projects() {
    let legacy_root = fixture("legacy_root");
    let legacy = Project::from_root(&legacy_root).unwrap();
    assert!(legacy.module("root").is_some());
    assert!(legacy.module("external_package::behaviour").is_none());
}

#[test]
fn rejects_an_invalid_project_name() {
    let error =
        Project::from_file(fixture("invalid_project").join("invalid-project.toml")).unwrap_err();

    assert!(error.message.contains("must be a valid Rils identifier"));
}
