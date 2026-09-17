use rils_frontend::{
    ModuleGraph, ProjectSyntax, SourceDatabase, SourceId, SymbolKind, analyze_project_with_host,
    exports::{ExportTable, collect_exports, resolve_export, resolve_reexports},
};
use std::collections::HashMap;

fn load(files: &[(&str, &str)]) -> (SourceDatabase, ModuleGraph, ProjectSyntax) {
    let mut sources = SourceDatabase::default();
    let mut modules = ModuleGraph::default();
    let mut syntax = ProjectSyntax::default();
    for (name, source) in files {
        let id = sources.set_source(*name, *source);
        let module = modules.register(name, id);
        syntax.insert_module(module, sources.parse(id).unwrap());
    }
    (sources, modules, syntax)
}

fn table(
    syntax: &ProjectSyntax,
    modules: &ModuleGraph,
    analysis: Option<&rils_frontend::analysis::DocumentAnalysis>,
) -> (
    ExportTable,
    Vec<rils_frontend::analysis::AnalysisDiagnostic>,
) {
    let units = syntax
        .modules()
        .map(|(id, program)| (vec![modules.module(id).unwrap().path.clone()], program))
        .collect::<Vec<_>>();
    let mut exports = HashMap::new();
    for (path, program) in &units {
        collect_exports(program, path, analysis, false, &mut exports);
    }
    let diagnostics = resolve_reexports(
        &mut exports,
        units
            .iter()
            .map(|(path, program)| (path.as_slice(), *program)),
    );
    (exports, diagnostics)
}

#[test]
fn reexports_preserve_original_identity_signature_fields_and_module_aliases() {
    let (_, modules, syntax) = load(&[
        ("main", include_str!("fixtures/reexports/main.rils")),
        ("api", include_str!("fixtures/reexports/api.rils")),
        ("relay", include_str!("fixtures/reexports/relay.rils")),
        ("origin", include_str!("fixtures/reexports/origin.rils")),
    ]);
    let analysis = analyze_project_with_host(&syntax, &modules, &rils_host::HostContract::new());
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let (exports, errors) = table(&syntax, &modules, Some(&analysis));
    assert!(errors.is_empty(), "{errors:?}");
    let original = resolve_export(&exports, &[], &["origin".into(), "compute".into()]).unwrap();
    for path in [vec!["api", "calculate"], vec!["api", "original", "compute"]] {
        let path = path.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let alias = resolve_export(&exports, &[], &path).unwrap();
        assert_eq!(alias.span, original.span);
        assert_eq!(alias.definition_id, original.definition_id);
        assert!(alias.definition_id.is_some());
        assert_eq!(alias.inferred_type, original.inferred_type);
    }
    let data = resolve_export(&exports, &[], &["api".into(), "Data".into()]).unwrap();
    assert_eq!(data.fields.len(), 1);
    assert_eq!(data.fields[0].name, "value");
    assert_eq!(
        resolve_export(&exports, &[], &["api".into(), "Label".into()])
            .unwrap()
            .kind,
        SymbolKind::Trait
    );
    let main = modules.module_by_path("main").unwrap().source.unwrap();
    let call = analysis
        .symbols
        .iter()
        .find(|symbol| {
            symbol.span.source == main && symbol.name == "calculate" && !symbol.is_definition
        })
        .unwrap();
    assert_eq!(call.definition_id, original.definition_id);
    let generic = analysis
        .symbols
        .iter()
        .find(|symbol| {
            symbol.span.source == main
                && symbol.is_definition
                && symbol.name == "Data"
                && symbol.definition_span == Some(symbol.span)
        })
        .unwrap();
    let parameter_type = analysis
        .symbols
        .iter()
        .find(|symbol| {
            symbol.span.source == main
                && !symbol.is_definition
                && symbol.name == "Data"
                && symbol.span.start > generic.span.start
        })
        .unwrap();
    assert_eq!(parameter_type.definition_id, generic.symbol_id);
}

#[test]
fn cyclic_private_and_ambiguous_reexports_report_the_import_source() {
    let (_, modules, syntax) = load(&[
        ("cycle_a", include_str!("fixtures/reexports/cycle_a.rils")),
        ("cycle_b", include_str!("fixtures/reexports/cycle_b.rils")),
        ("conflict", include_str!("fixtures/reexports/conflict.rils")),
        ("private", include_str!("fixtures/reexports/private.rils")),
        ("origin", include_str!("fixtures/reexports/origin.rils")),
        ("other", include_str!("fixtures/reexports/other.rils")),
    ]);
    let (exports, errors) = table(&syntax, &modules, None);
    for name in ["cycle_a", "cycle_b", "conflict", "private"] {
        let source = modules.module_by_path(name).unwrap().source.unwrap();
        assert_ne!(source, SourceId::UNKNOWN);
        assert!(
            errors.iter().any(|error| error.span.source == source),
            "{name}: {errors:?}"
        );
    }
    assert!(resolve_export(&exports, &[], &["conflict".into(), "compute".into()]).is_none());
    assert!(resolve_export(&exports, &[], &["private".into(), "hidden".into()]).is_none());
}
