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
