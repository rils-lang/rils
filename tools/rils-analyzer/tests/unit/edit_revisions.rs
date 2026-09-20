use super::*;

#[test]
fn edits_refresh_highlighting_and_new_symbol_definitions() {
    let uri = "file:///edit-revisions.rils";
    let (connection, _client) = Connection::memory();
    let mut server = Server {
        connection,
        documents: HashMap::new(),
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        host_types: HashSet::new(),
        projects: Vec::new(),
        compilation: CompilationSession::default(),
        next_source_id: 1,
    };
    let before = include_str!("../fixtures/edit_revisions/before.rils");
    let after = include_str!("../fixtures/edit_revisions/after.rils");
    server
        .handle_notification(Notification::new(
            "textDocument/didOpen".into(),
            json!({"textDocument": {"uri": uri, "text": before}}),
        ))
        .unwrap();
    server.handle_notification(Notification::new("textDocument/didChange".into(),
        json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": after}]}))).unwrap();
    for (name, expected_line) in [("value", 2), ("added", 0)] {
        let offset = after.rfind(name).unwrap();
        let pos = position(after, offset);
        let target = server
            .definition(&json!({"textDocument": {"uri": uri},
            "position": {"line": pos[0], "character": pos[1]}}))
            .unwrap();
        assert_eq!(target["uri"], uri);
        assert_eq!(target["range"]["start"]["line"], expected_line);
        assert_eq!(target["range"]["start"]["character"], 3);
    }
    let tokens = server
        .semantic_tokens(&json!({"textDocument": {"uri": uri}}))
        .unwrap();
    let data = tokens["data"].as_array().unwrap();
    let mut line = 0;
    let mut character = 0;
    let mut found_shifted_function = false;
    for token in data.chunks_exact(5) {
        let delta = token[0].as_u64().unwrap();
        line += delta;
        character = if delta == 0 { character } else { 0 } + token[1].as_u64().unwrap();
        if line == 2 && character == 3 {
            assert_eq!(token[2], 5);
            found_shifted_function = true;
        }
    }
    assert!(found_shifted_function);

    // Invalid text must not receive semantic token spans from the old revision.
    server.handle_notification(Notification::new("textDocument/didChange".into(),
        json!({"textDocument": {"uri": uri, "version": 3}, "contentChanges": [{"text": "let ="}]}))).unwrap();
    assert_eq!(
        server
            .semantic_tokens(&json!({"textDocument": {"uri": uri}}))
            .unwrap()["data"],
        json!([])
    );
}
