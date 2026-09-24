use super::*;

#[test]
fn standard_library_discovery_covers_distribution_layouts_and_explicit_overrides() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/language_package");
    let missing = fixtures.join("missing");
    for (executable, expected) in [
        (
            "toolchain/bin/analyzer.stub",
            "toolchain/lib/rils/packages/rils_stdlib",
        ),
        (
            "extension/server/analyzer.stub",
            "extension/sysroot/packages/rils_stdlib",
        ),
    ] {
        let resolved = workspace::resolve_language_package_root(
            None,
            Some(&fixtures.join(executable)),
            &missing,
        )
        .unwrap();
        assert_eq!(
            path_to_file_uri(&resolved),
            path_to_file_uri(&fixtures.join(expected))
        );
    }
    let configured = fixtures.join("extension/sysroot");
    let executable = fixtures.join("toolchain/bin/analyzer.stub");
    let expected = configured.join("packages/rils_stdlib");
    assert_eq!(
        workspace::resolve_language_package_root(Some(&configured), Some(&executable), &missing)
            .unwrap(),
        expected
    );
    assert!(
        workspace::resolve_language_package_root(Some(&missing), Some(&executable), &expected)
            .unwrap_err()
            .contains("RILS_SYSROOT")
    );
    assert_eq!(
        workspace::resolve_language_package_root(None, None, &expected).unwrap(),
        expected
    );
    assert!(workspace::resolve_language_package_root(None, None, &missing).is_err());
}

#[test]
fn loads_reserved_standard_library_modules_as_a_language_package() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rils_builtins")
        .join("stdlib");
    let package = Project::from_language_package(
        root.join("rils.toml"),
        LanguagePackageKind::StandardLibrary,
    )
    .unwrap();
    assert!(package.module("core::array").is_none());
    assert!(package.module("std::io").is_some());
}

#[test]
fn standard_library_signature_bodies_do_not_publish_user_diagnostics() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("rils_builtins")
        .join("stdlib");
    let (connection, client) = Connection::memory();
    let mut server = Server {
        connection,
        documents: HashMap::new(),
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        host_types: HashSet::new(),
        projects: vec![
            Project::from_language_package(
                root.join("rils.toml"),
                LanguagePackageKind::StandardLibrary,
            )
            .unwrap(),
        ],
        compilation: CompilationSession::default(),
        next_source_id: 1,
    };
    server.load_workspace().unwrap();
    let internal_diagnostics = server
        .documents
        .iter()
        .filter(|(uri, _)| uri.contains("rils_builtins"))
        .flat_map(|(_, document)| document.analysis.as_ref().unwrap().diagnostics.iter())
        .collect::<Vec<_>>();
    assert!(internal_diagnostics.iter().all(|diagnostic| {
        !diagnostic.message.contains("not all paths return")
            && !diagnostic.message.contains("already defined in this scope")
    }));
    server.publish_all_diagnostics().unwrap();
    let published = client
        .receiver
        .try_iter()
        .filter_map(|message| match message {
            lsp_server::Message::Notification(notification)
                if notification.method == "textDocument/publishDiagnostics" =>
            {
                Some(notification.params)
            }
            _ => None,
        })
        .filter(|params| {
            params["uri"]
                .as_str()
                .is_some_and(|uri| uri.contains("rils_builtins"))
        })
        .collect::<Vec<_>>();
    assert!(!published.is_empty());
    assert!(
        published
            .iter()
            .all(|params| params["diagnostics"].as_array().is_some_and(Vec::is_empty)),
        "{published:#?}"
    );

    let uri = path_to_file_uri(&root.join("prelude.rils"));
    for (source, expected) in [
        (
            include_str!("../fixtures/language_package/invalid_signature.rils"),
            "MissingType",
        ),
        (
            include_str!("../fixtures/language_package/invalid_syntax.rils"),
            "parse error",
        ),
        (
            include_str!("../fixtures/language_package/invalid_attribute.rils"),
            "does not accept arguments",
        ),
    ] {
        server.update_document(uri.clone(), source.into()).unwrap();
        let messages = client
            .receiver
            .try_iter()
            .filter_map(|message| match message {
                Message::Notification(notification)
                    if notification.method == "textDocument/publishDiagnostics"
                        && notification.params["uri"] == uri =>
                {
                    Some(notification.params)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            messages.iter().any(|params| params["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|diagnostic| diagnostic["message"].as_str().unwrap().contains(expected))),
            "{messages:#?}"
        );
    }
}

#[test]
fn workspace_projects_receive_the_standard_library_prelude_dependency() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/language_package/user");

    let uri = path_to_file_uri(&root);
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
    server
        .load_projects(&json!({
            "rootUri": uri,
            "workspaceFolders": [{ "uri": uri, "name": "fixture" }]
        }))
        .unwrap();
    let workspace = server
        .projects
        .iter()
        .find(|project| project.origin() == rils_project::ProjectOrigin::Workspace)
        .unwrap();
    assert_eq!(
        workspace.language_dependencies().collect::<Vec<_>>(),
        [LanguagePackageKind::StandardLibrary]
    );

    let stdlib = server
        .projects
        .iter()
        .find(|project| {
            project.origin()
                == rils_project::ProjectOrigin::Language(LanguagePackageKind::StandardLibrary)
        })
        .unwrap();
    let prelude = stdlib.prelude().expect("standard library has a prelude");
    let prelude_uri = path_to_file_uri(prelude);

    server.load_workspace().unwrap();
    assert!(server.documents.contains_key(&prelude_uri));
    let main_uri = path_to_file_uri(&root.join("main.rils"));
    let main_analysis = server.documents[&main_uri]
        .analysis
        .as_ref()
        .expect("workspace document has analysis");
    assert!(
        !main_analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("undefined name `type_of`"))
    );
}
