use super::*;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/workspace")
        .join(name)
}

#[test]
fn workspace_projects_index_nested_projects_without_treating_package_paths_as_modules() {
    let projects = workspace_projects(&fixture("loose")).unwrap();
    assert_eq!(projects.len(), 2);
    assert!(projects[0].module("root").is_some());
    assert!(projects[1].module("behaviour").is_some());
}

#[test]
fn workspace_projects_keep_good_projects_after_root_and_manifest_failures() {
    let load = workspace::workspace_projects_with_language(&fixture("mixed"), None).unwrap();
    let names = load.projects.iter().map(Project::name).collect::<Vec<_>>();
    assert_eq!(names, ["alpha", "beta"]);
    assert!(
        load.errors
            .iter()
            .any(|error| error.contains("legacy workspace root"))
    );
    assert!(load.errors.iter().any(|error| error.contains("broken")));
}

#[test]
fn scan_errors_preserve_results_and_allow_other_directories() {
    let root = fixture("mixed");
    let mut manifests = Vec::new();
    let mut errors = Vec::new();
    // A file produces a deterministic I/O failure without changing permissions.
    workspace::collect_nested_project_manifests(
        &root.join("alpha/rils.toml"),
        &mut manifests,
        &mut errors,
    );
    workspace::collect_nested_project_manifests(&root, &mut manifests, &mut errors);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("alpha"));
    assert_eq!(manifests.len(), 3);
}

#[test]
fn equivalent_language_package_paths_are_not_discovered_as_user_projects() {
    let root = fixture("loose");
    let equivalent = root.join("com.rils-lang.rils-for-unity/..");
    let load = workspace::workspace_projects_with_language(&equivalent, Some(&root)).unwrap();
    assert!(load.projects.is_empty());
    assert!(load.errors.is_empty());
}

#[test]
fn repository_workspace_discovers_example_projects_after_legacy_scan_errors() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let load = workspace::workspace_projects_with_language(repository, None).unwrap();
    for name in ["task_board", "telemetry_pipeline"] {
        assert!(load.projects.iter().any(|project| project.name() == name));
    }
}
