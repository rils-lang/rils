use super::*;

#[test]
fn source_ids_survive_edits_and_parsing_is_revision_scoped() {
    let mut database = SourceDatabase::default();
    let id = database.set_source("src/main.rils", "let value = 1;");
    let first = database.parse(id).expect("initial source parses");

    assert_eq!(database.set_source("src/main.rils", "let value = 20;"), id);
    assert_eq!(database.revision(id), Some(1));
    let second = database.parse(id).expect("updated source parses");
    let crate::ast::Stmt::Let {
        span: first_span, ..
    } = &first.statements[0]
    else {
        panic!("expected first let statement");
    };
    let crate::ast::Stmt::Let {
        span: second_span, ..
    } = &second.statements[0]
    else {
        panic!("expected second let statement");
    };
    assert_ne!(*first_span, *second_span);

    database.set_source("src/main.rils", "let value = 20;");
    assert_eq!(database.revision(id), Some(1));
}

#[test]
fn reserved_source_starts_its_first_text_at_revision_zero() {
    let mut database = SourceDatabase::default();
    let id = database.reserve("src/lib.rils");

    database.set_source_with_id(id, "src/lib.rils", "fn value() { 1 }");

    assert_eq!(database.revision(id), Some(0));
    assert!(database.parse(id).is_ok());
}

#[test]
fn module_graph_creates_stable_parent_nodes() {
    let mut graph = ModuleGraph::default();
    let source = SourceId::new(7);
    let child = graph.register("foo::bar", source);
    let parent = graph.module_by_path("foo").expect("parent module");

    assert_eq!(graph.module(child).unwrap().parent, Some(parent.id));
    assert_eq!(graph.module(child).unwrap().source, Some(source));
    assert_eq!(graph.register("foo::bar", source), child);
}

#[test]
fn module_graph_resolves_sources_relative_paths_and_children() {
    let mut graph = ModuleGraph::default();
    let main = graph.register("main", SourceId::new(1));
    let foo = graph.register("foo", SourceId::new(2));
    let bar = graph.register("foo::bar", SourceId::new(3));

    assert_eq!(graph.module_for_source(SourceId::new(3)).unwrap().id, bar);
    assert_eq!(graph.resolve(main, "crate::foo::bar").unwrap().id, bar);
    assert_eq!(graph.resolve(bar, "self").unwrap().id, bar);
    assert_eq!(graph.resolve(bar, "super").unwrap().id, foo);
    assert_eq!(
        graph
            .children(foo)
            .map(|module| module.id)
            .collect::<Vec<_>>(),
        vec![bar]
    );
}

#[test]
fn project_semantic_index_exposes_one_shared_module_view() {
    let mut index = ProjectSemanticIndex::default();
    let main_source = SourceId::new(10);
    let child_source = SourceId::new(11);
    index.register("main", main_source);
    let child = index.register("feature::child", child_source);

    assert_eq!(index.module(child_source).unwrap().id, child);
    assert_eq!(
        index
            .resolve(main_source, "crate::feature::child")
            .unwrap()
            .id,
        child
    );
    assert!(index.modules().any(|module| module.id == child));
}

#[test]
fn project_semantic_index_collects_document_definitions() {
    let source = SourceId::new(20);
    let analysis =
        crate::analysis::analyze_with_source_id("fn value() { 1 }", source, &HashMap::new())
            .unwrap();
    let definition = analysis
        .def_map
        .definitions()
        .find(|definition| definition.name == "value")
        .unwrap()
        .clone();
    let mut index = ProjectSemanticIndex::default();

    index.index_def_map(&analysis.def_map);

    assert_eq!(index.definition(definition.id), Some(&definition));
}

#[test]
fn compilation_session_keeps_project_and_source_identities_together() {
    let mut session = CompilationSession::default();
    let first = session.register_project("workspace/first");
    let second = session.register_project("workspace/second");
    let first_source = session
        .sources_mut()
        .set_source("first/src/main.rils", "fn main() {}");
    let second_source = session
        .sources_mut()
        .set_source("second/src/main.rils", "fn main() {}");

    let first_module = session
        .project_mut(first)
        .unwrap()
        .register("main", first_source);
    let second_module = session
        .project_mut(second)
        .unwrap()
        .register("main", second_source);

    assert_ne!(first, second);
    assert_eq!(session.register_project("workspace/first"), first);
    assert_eq!(session.project_id("workspace/second"), Some(second));
    assert_eq!(
        session
            .project(first)
            .unwrap()
            .module(first_source)
            .unwrap()
            .id,
        first_module
    );
    assert_eq!(
        session
            .project(second)
            .unwrap()
            .module(second_source)
            .unwrap()
            .id,
        second_module
    );
    assert_ne!(first_source, second_source);
}

#[test]
fn compilation_session_keeps_module_programs_structured() {
    let mut session = CompilationSession::default();
    let project = session.register_project("workspace/app");
    let main_source = session
        .sources_mut()
        .set_source("app/src/main.rils", "pub fn main() {}");
    let util_source = session
        .sources_mut()
        .set_source("app/src/util.rils", "pub fn value() { 1 }");
    let main_module = session
        .project_mut(project)
        .unwrap()
        .register("main", main_source);
    let util_module = session
        .project_mut(project)
        .unwrap()
        .register("support::util", util_source);
    let main_program = session.sources().parse(main_source).unwrap();
    let util_program = session.sources().parse(util_source).unwrap();

    let syntax = session.project_syntax_mut(project).unwrap();
    syntax.insert_module(main_module, main_program);
    syntax.insert_module(util_module, util_program);

    let syntax = session.project_syntax(project).unwrap();
    assert_eq!(syntax.modules().len(), 2);
    assert!(syntax.module(main_module).is_some());
    assert!(syntax.module(util_module).is_some());

    assert_eq!(
        session
            .project(project)
            .unwrap()
            .module(main_source)
            .unwrap()
            .path,
        "main"
    );
    assert_eq!(
        session
            .project(project)
            .unwrap()
            .module(util_source)
            .unwrap()
            .path,
        "support::util"
    );
}

#[test]
fn compilation_session_invalidates_cached_analysis_on_input_or_host_changes() {
    let mut session = CompilationSession::default();
    let project = session.register_project("workspace/app");
    let host = rils_host::HostContract::new();
    session.set_project_analysis(project, &host, crate::analysis::DocumentAnalysis::default());
    assert!(session.project_analysis(project, &host).is_some());

    let mut changed_host = host.clone();
    changed_host
        .register_type(
            "game::Object",
            None::<&str>,
            rils_host::HostTypeTransport::HostHandle,
        )
        .unwrap();
    assert!(session.project_analysis(project, &changed_host).is_none());

    session
        .sources_mut()
        .set_source("app/main.rils", "fn main() {}");
    assert!(session.project_analysis(project, &host).is_none());
}
