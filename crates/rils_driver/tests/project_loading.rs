use std::{fs, path::PathBuf};

use rils_driver::{ProjectSources, discover_entry_project, load_file_modules};
use rils_frontend::macros::STANDARD_NATIVE_MACROS;

#[test]
fn prepares_a_legacy_module_tree_without_a_backend() {
    let entry =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/module_tree/main.rils");
    let project = discover_entry_project(&entry).expect("legacy entry should form a project");
    let source = fs::read_to_string(&entry).expect("fixture entry should be readable");
    let mut sources = ProjectSources::default();
    sources.register_project(&project);
    let source_id = sources.register_source(&entry, &source);
    let mut program = sources
        .parse(source_id, STANDARD_NATIVE_MACROS)
        .expect("fixture entry should parse");

    load_file_modules(
        &mut program,
        &entry,
        &project,
        STANDARD_NATIVE_MACROS,
        &mut sources,
        true,
    )
    .expect("module tree should load");

    assert!(
        sources
            .session()
            .project_syntax(sources.project_id())
            .is_some()
    );
    assert!(program.statements.len() >= 2);
}
