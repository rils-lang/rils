use super::*;

fn workspace() -> (Server, Connection, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reexports");
    let (connection, client) = Connection::memory();
    // Keep the receiving endpoint alive while update_document publishes.
    let server = Server {
        connection,
        documents: HashMap::new(),
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        host_types: HashSet::new(),
        projects: ["one", "two"]
            .into_iter()
            .map(|name| Project::from_file(root.join(name).join("rils.toml")).unwrap())
            .collect(),
        compilation: CompilationSession::default(),
        next_source_id: 1,
    };
    (server, client, root)
}

fn at(server: &Server, uri: &str, needle: &str) -> Value {
    let text = &server.documents[uri].text;
    let offset = text.find(needle).unwrap();
    let [line, character] = position(text, offset);
    json!({"textDocument": {"uri": uri}, "position": {"line": line, "character": character}})
}

#[test]
fn reexports_share_navigation_hover_completion_and_reference_identity() {
    let (mut server, _client, root) = workspace();
    server.load_workspace().unwrap();
    let main = path_to_file_uri(&root.join("one/src/main.rils"));
    let origin = path_to_file_uri(&root.join("one/src/origin.rils"));
    for document in server.documents.values() {
        assert!(
            document.analysis.as_ref().unwrap().diagnostics.is_empty(),
            "{:?}",
            document.analysis
        );
    }
    let call = at(&server, &main, "calculate(41)");
    let definition = server.definition(&call).unwrap();
    assert_eq!(definition["uri"], origin);
    assert_eq!(
        definition["range"]["start"],
        at(&server, &origin, "compute")["position"]
    );
    let hover = server.hover(&call).unwrap();
    assert!(
        hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("value: i32"),
        "{hover}"
    );
    let field = server.definition(&at(&server, &main, "value\n")).unwrap();
    assert_eq!(field["uri"], origin);
    let completion = server
        .completion(&at(&server, &main, "calculate, Data"))
        .unwrap();
    assert!(
        completion
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["filterText"] == "calculate"),
        "{completion}"
    );
    assert!(
        !completion
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["filterText"] == "hidden")
    );
    let references = server.references(&at(&server, &origin, "compute")).unwrap();
    let locations = references.as_array().unwrap();
    assert_eq!(
        locations.len(),
        locations
            .iter()
            .map(Value::to_string)
            .collect::<HashSet<_>>()
            .len()
    );
    assert!(locations.iter().any(|location| location["uri"] == main));
    assert!(
        locations
            .iter()
            .any(|location| location["uri"] == path_to_file_uri(&root.join("one/src/relay.rils")))
    );
    assert!(
        locations
            .iter()
            .all(|location| !location["uri"].as_str().unwrap().contains("/two/")),
        "{references}"
    );
}

#[test]
fn edits_retarget_exports_without_changing_project_ids_or_crossing_projects() {
    let (mut server, _client, root) = workspace();
    server.load_workspace().unwrap();
    let ids = server
        .projects
        .iter()
        .map(|project| {
            server
                .compilation
                .project_id(&project_session_name(project))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let main = path_to_file_uri(&root.join("one/src/main.rils"));
    let api = path_to_file_uri(&root.join("one/src/api.rils"));
    let origin = path_to_file_uri(&root.join("one/src/origin.rils"));
    server
        .update_document(
            api.clone(),
            include_str!("../fixtures/reexports/retarget.rils").into(),
        )
        .unwrap();
    let target = server
        .definition(&at(&server, &main, "calculate(41)"))
        .unwrap();
    assert_eq!(
        target["range"]["start"],
        at(&server, &origin, "replacement")["position"]
    );
    server
        .update_document(
            api.clone(),
            include_str!("../fixtures/reexports/invalid_edit.rils").into(),
        )
        .unwrap();
    assert_eq!(
        server
            .definition(&at(&server, &main, "calculate(41)"))
            .unwrap(),
        target
    );
    assert!(
        server.documents[&api]
            .analysis
            .as_ref()
            .unwrap()
            .diagnostics
            .iter()
            .any(|error| error.message.contains("parse error"))
    );
    for (project, id) in server.projects.iter().zip(ids) {
        assert_eq!(
            server
                .compilation
                .project_id(&project_session_name(project)),
            Some(id)
        );
    }
    let other_main = path_to_file_uri(&root.join("two/src/main.rils"));
    assert_eq!(
        server
            .definition(&at(&server, &other_main, "calculate(41)"))
            .unwrap()["uri"],
        path_to_file_uri(&root.join("two/src/origin.rils"))
    );
    server
        .update_document(
            api.clone(),
            include_str!("../fixtures/reexports/private_export.rils").into(),
        )
        .unwrap();
    let source = server.documents[&api].source_id;
    assert!(
        server.documents[&api]
            .analysis
            .as_ref()
            .unwrap()
            .diagnostics
            .iter()
            .any(|error| error.span.source == source
                && error.message.contains("cannot resolve public re-export"))
    );
}
