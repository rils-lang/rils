use super::*;
use crate::{ProjectSyntax, SourceDatabase, SymbolContainer};

#[test]
fn project_analysis_resolves_calls_across_independent_module_programs() {
    let mut sources = SourceDatabase::default();
    let api_source = sources.set_source("src/api.rils", "pub fn answer() -> i32 { 42 }");
    let main_source = sources.set_source(
        "src/feature/mod.rils",
        "fn local() -> i32 { 0 } fn main() -> i32 { self::local() + super::api::answer() }",
    );
    let mut modules = ModuleGraph::default();
    let api = modules.register("api", api_source);
    let main = modules.register("feature", main_source);
    let mut syntax = ProjectSyntax::default();
    syntax.insert_module(api, sources.parse(api_source).unwrap());
    syntax.insert_module(main, sources.parse(main_source).unwrap());

    let analysis =
        analyze_project_with_host_declarations(&syntax, &modules, &HashMap::new(), &HashSet::new());

    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    let answer = analysis
        .def_map
        .definitions()
        .find(|definition| definition.name == "answer")
        .unwrap();
    assert_eq!(
        answer.container,
        Some(SymbolContainer::Module("api".into()))
    );
    let answer_reference = analysis
        .symbols
        .iter()
        .find(|symbol| !symbol.is_definition && symbol.name == "answer")
        .unwrap();
    assert_eq!(answer_reference.definition_id, Some(answer.id));
    assert!(
        analysis
            .typeck_results
            .resolved_call_containing(main_source, answer_reference.span.start)
            .is_some()
    );
}
