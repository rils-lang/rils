use super::{
    Document, Project, Server, SourceId, Type, analysis, diagnostics, file_uri_to_path,
    function_declaration, offset, path_to_file_uri, position,
};
use lsp_server::Connection;
use rils_compiler::HostContract;
use rils_frontend::FunctionSignature;
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    fs,
};

#[test]
fn positions_use_utf16_characters() {
    let source = "let 名字 = \"😀\";\n名字";
    for byte_offset in [0, 4, 10, source.len()] {
        let position = position(source, byte_offset);
        assert_eq!(offset(source, position[0], position[1]), byte_offset);
    }
}

#[test]
fn formats_higher_order_function_declarations() {
    let ty = Type::function(Vec::new(), Type::function(Vec::new(), Type::I32));
    assert_eq!(
        function_declaration("make_value", &ty),
        "fn make_value() -> fn() -> i32"
    );
}

#[test]
fn provides_signature_help_for_incomplete_user_calls() {
    let text = "fn add(left: i32, right: i32) -> i32 { left + right }\nadd(1, ";
    let uri = "file:///signature.rils".to_owned();
    let server = test_server(&uri, text, HashMap::new(), HostContract::new());

    let help = server
        .signature_help(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 7 }
        }))
        .unwrap();
    assert_eq!(help["signatures"][0]["label"], "fn add(i32, i32) -> i32");
    assert_eq!(help["activeParameter"], 1);
}

#[test]
fn provides_signature_help_for_host_functions() {
    let mut contract = HostContract::new();
    contract
        .register_function(
            100,
            "unity_engine::math::add",
            FunctionSignature::fixed(
                vec![
                    Type::Float(rils_frontend::FloatType::F32),
                    Type::Float(rils_frontend::FloatType::F32),
                ],
                Type::Float(rils_frontend::FloatType::F32),
            ),
            "unity.math",
        )
        .unwrap();
    let host_functions = contract
        .functions()
        .map(|function| (function.name.clone(), function.signature.clone()))
        .collect::<HashMap<_, _>>();
    let text = "use unity_engine::math as math;\nmath::add(1f32, ";
    let uri = "file:///host-signature.rils".to_owned();
    let server = test_server(&uri, text, host_functions, contract);

    let help = server
        .signature_help(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 16 }
        }))
        .unwrap();
    assert_eq!(help["signatures"][0]["label"], "fn add(f32, f32) -> f32");
    assert_eq!(help["activeParameter"], 1);
}

#[test]
fn provides_signature_help_for_builtin_methods() {
    let text = "let text = \"alpha\";\ntext.replace(\"a\", ";
    let uri = "file:///builtin-signature.rils".to_owned();
    let server = test_server(&uri, text, HashMap::new(), HostContract::new());

    let help = server
        .signature_help(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 18 }
        }))
        .unwrap();
    assert_eq!(
        help["signatures"][0]["label"],
        "fn replace(string, string) -> string"
    );
    assert_eq!(help["activeParameter"], 1);
}

#[test]
fn provides_signature_help_for_integer_intrinsics() {
    let text = "let value: i32 = 1;\nvalue.checked_add(";
    let uri = "file:///integer-signature.rils".to_owned();
    let server = test_server(&uri, text, HashMap::new(), HostContract::new());

    let help = server
        .signature_help(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 18 }
        }))
        .unwrap();
    assert_eq!(
        help["signatures"][0]["label"],
        "fn checked_add(i32) -> Option<i32>"
    );
    assert_eq!(help["activeParameter"], 0);
}

fn test_server(
    uri: &str,
    text: &str,
    host_functions: HashMap<String, FunctionSignature>,
    host_contract: HostContract,
) -> Server {
    let (connection, _client) = Connection::memory();
    let mut documents = HashMap::new();
    documents.insert(
        uri.to_owned(),
        Document {
            source_id: SourceId::UNKNOWN,
            text: text.into(),
            analysis: rils_frontend::analysis::analyze_with_host_functions(text, &host_functions),
        },
    );
    Server {
        connection,
        documents,
        workspace_documents: HashSet::new(),
        host_contract,
        host_functions,
        projects: Vec::new(),
        next_source_id: 1,
    }
}

#[test]
fn hover_shows_expanded_type_aliases() {
    let text = "struct Box<T> { value: T }\ntype ValueBox<T> = Box<T>;\ntype IntBox = ValueBox<i32>;\nlet value: IntBox = Box { value: 1 };";
    let uri = "file:///aliases.rils".to_owned();
    let (connection, _client) = Connection::memory();
    let mut documents = HashMap::new();
    documents.insert(
        uri.clone(),
        Document {
            source_id: SourceId::UNKNOWN,
            text: text.into(),
            analysis: rils_frontend::analysis::analyze(text),
        },
    );
    let server = Server {
        connection,
        documents,
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        projects: Vec::new(),
        next_source_id: 1,
    };

    let hover = server
        .hover(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 5 }
        }))
        .unwrap();
    assert_eq!(
        hover
            .pointer("/contents/value")
            .and_then(|value| value.as_str()),
        Some("```rils\ntype IntBox = Box<i32>\n```")
    );
}

#[test]
fn completes_host_modules_functions_and_aliases() {
    let mut contract = HostContract::new();
    contract
        .register_function(
            100,
            "unity_engine::math::add",
            FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
            "unity.math",
        )
        .unwrap();
    contract
        .register_function(
            101,
            "unity_engine::math::subtract",
            FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
            "unity.math",
        )
        .unwrap();
    contract
        .register_function(
            102,
            "unity_engine::time::frame_count",
            FunctionSignature::fixed(Vec::new(), Type::Integer(rils_frontend::IntegerType::U64)),
            "unity.time",
        )
        .unwrap();
    let host_functions = contract
        .functions()
        .map(|function| (function.name.clone(), function.signature.clone()))
        .collect::<HashMap<_, _>>();
    let text = "use unity_engine::math as math;\nmath::a";
    let uri = "file:///completion.rils".to_owned();
    let (connection, _client) = Connection::memory();
    let mut documents = HashMap::new();
    documents.insert(
        uri.clone(),
        Document {
            source_id: SourceId::UNKNOWN,
            text: text.into(),
            analysis: rils_frontend::analysis::analyze_with_host_functions(text, &host_functions),
        },
    );
    let server = Server {
        connection,
        documents,
        workspace_documents: HashSet::new(),
        host_contract: contract,
        host_functions,
        projects: Vec::new(),
        next_source_id: 1,
    };

    let functions = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 7 }
        }))
        .unwrap();
    assert_eq!(functions.as_array().unwrap().len(), 1);
    assert_eq!(functions[0]["label"], "add");
    assert_eq!(functions[0]["detail"], "fn add(i32, i32) -> i32");
    assert!(
        functions[0]
            .pointer("/documentation/value")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("unity.math"))
    );

    let modules = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 18 }
        }))
        .unwrap();
    assert!(
        modules
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["label"] == "math"))
    );
}

#[test]
fn completes_integer_intrinsic_methods_and_associated_functions() {
    let text = "let value: i32 = 1;\nvalue.checked_add(1i32);\ni16::try_from(1usize);";
    let uri = "file:///intrinsics.rils".to_owned();
    let (connection, _client) = Connection::memory();
    let mut documents = HashMap::new();
    documents.insert(
        uri.clone(),
        Document {
            source_id: SourceId::UNKNOWN,
            text: text.into(),
            analysis: rils_frontend::analysis::analyze(text),
        },
    );
    let server = Server {
        connection,
        documents,
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        projects: Vec::new(),
        next_source_id: 1,
    };

    let methods = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 11 }
        }))
        .unwrap();
    assert!(
        methods
            .as_array()
            .is_some_and(|items| { items.iter().any(|item| item["label"] == "checked_add") })
    );

    let associated = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 9 }
        }))
        .unwrap();
    assert_eq!(associated[0]["label"], "try_from");
}

#[test]
fn completes_builtin_members_for_values_and_expressions() {
    let text = r#"let text = "alpha";
text.st"#;
    let uri = "file:///builtin-members.rils".to_owned();
    let (connection, _client) = Connection::memory();
    let mut documents = HashMap::new();
    documents.insert(
        uri.clone(),
        Document {
            source_id: SourceId::new(1),
            text: text.into(),
            analysis: rils_frontend::analysis::analyze_with_source_id(
                text,
                SourceId::new(1),
                &HashMap::new(),
            ),
        },
    );
    let server = Server {
        connection,
        documents,
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        projects: Vec::new(),
        next_source_id: 2,
    };
    let complete = |line, character| {
        server
            .completion(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }))
            .unwrap()
    };
    let string_items = complete(1, 7);
    assert!(
        string_items
            .as_array()
            .is_some_and(|items| { items.iter().any(|item| item["label"] == "starts_with") }),
        "{string_items}"
    );

    let expression_text = "Some(1).un";
    let expression_uri = "file:///builtin-expression.rils".to_owned();
    let (connection, _client) = Connection::memory();
    let mut documents = HashMap::new();
    documents.insert(
        expression_uri.clone(),
        Document {
            source_id: SourceId::new(2),
            text: expression_text.into(),
            analysis: rils_frontend::analysis::analyze_with_source_id(
                expression_text,
                SourceId::new(2),
                &HashMap::new(),
            ),
        },
    );
    let expression_server = Server {
        connection,
        documents,
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        projects: Vec::new(),
        next_source_id: 3,
    };
    let option_items = expression_server
        .completion(&json!({
            "textDocument": { "uri": expression_uri },
            "position": { "line": 0, "character": 10 }
        }))
        .unwrap();
    assert!(
        option_items.as_array().is_some_and(|items| {
            items.iter().any(|item| item["label"] == "unwrap")
                && items.iter().any(|item| item["label"] == "unwrap_or")
        }),
        "{option_items}"
    );
}

#[test]
fn completes_project_modules_public_items_and_crate_aliases() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "rils-analyzer-project-test-{}-{unique}",
        std::process::id()
    ));
    let scripts = root.join("Assets/Res/rils-script");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(
        root.join("rils.toml"),
        "[project]\nname = \"unity_game\"\nscript_paths = [\"Assets/Res/rils-script\"]\n",
    )
    .unwrap();
    fs::write(
        scripts.join("math.rils"),
        "pub fn add(left: i32, right: i32) -> i32 { left + right }\nfn hidden() {}",
    )
    .unwrap();
    let entry = scripts.join("main.rils");
    let text = "use crate::math as math;\nfn main() { math::add(1, 2); }";
    fs::write(&entry, text).unwrap();
    let project = Project::from_file(root.join("rils.toml")).unwrap();
    let (connection, _client) = Connection::memory();
    let mut server = Server {
        connection,
        documents: HashMap::new(),
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        projects: vec![project],
        next_source_id: 1,
    };
    server.load_workspace().unwrap();
    let uri = path_to_file_uri(&entry);
    let completion = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 19 }
        }))
        .unwrap();
    assert!(completion.as_array().is_some_and(|items| {
        items.iter().any(|item| item["label"] == "add")
            && !items.iter().any(|item| item["label"] == "hidden")
    }));
    let definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 20 }
        }))
        .unwrap();
    let expected_uri = path_to_file_uri(&scripts.join("math.rils"));
    assert_eq!(definition["uri"].as_str(), Some(expected_uri.as_str()));
    let references = server
        .references(&json!({
            "textDocument": { "uri": expected_uri },
            "position": { "line": 0, "character": 8 },
            "context": { "includeDeclaration": true }
        }))
        .unwrap();
    assert_eq!(references.as_array().map(Vec::len), Some(2));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn loads_binary_host_manifest_from_initialization_options() {
    let mut contract = HostContract::new();
    contract
        .register_function(
            100,
            "unity_engine::math::add",
            FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
            "unity.math",
        )
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "rils-analyzer-host-manifest-{}.rilhm",
        std::process::id()
    ));
    fs::write(&path, contract.to_manifest_bytes().unwrap()).unwrap();
    let (connection, _client) = Connection::memory();
    let mut server = Server {
        connection,
        documents: HashMap::new(),
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        projects: Vec::new(),
        next_source_id: 1,
    };
    let result = server.load_host_manifests(&json!({
        "initializationOptions": {
            "hostManifestPaths": [path.to_string_lossy()]
        }
    }));
    fs::remove_file(path).unwrap();
    result.unwrap();
    assert!(
        server
            .host_contract
            .function("unity_engine::math::add")
            .is_some()
    );
    assert!(
        server
            .host_functions
            .contains_key("unity_engine::math::add")
    );
}

#[test]
fn parse_errors_remain_diagnostics_not_request_failures() {
    let text = "let =";
    let result = rils_frontend::analysis::analyze(text);
    let document = Document {
        source_id: SourceId::UNKNOWN,
        text: text.into(),
        analysis: result,
    };
    assert!(analysis(&document).is_none());
    assert_eq!(diagnostics(&document.text, &document.analysis).len(), 1);
}

#[test]
fn publishes_control_flow_diagnostics() {
    let text = "fn value(flag: bool) -> i32 { if flag { 1 } }";
    let result = rils_frontend::analysis::analyze(text);
    let output = diagnostics(text, &result);
    assert_eq!(output.len(), 1);
    assert!(
        output[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("not all paths return"))
    );
}

#[test]
fn publishes_ownership_diagnostics() {
    let text = "fn invalid() { let value = \"owned\"; let moved = value; value; }";
    let result = rils_frontend::analysis::analyze(text);
    let output = diagnostics(text, &result);
    assert_eq!(output.len(), 1);
    assert!(
        output[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("use of moved value"))
    );
}

#[test]
fn publishes_warnings_with_lsp_warning_severity() {
    let text = "fn value() -> i32 { return 1; 2 }";
    let result = rils_frontend::analysis::analyze(text);
    let output = diagnostics(text, &result);
    assert!(output.iter().any(|diagnostic| {
        diagnostic["message"] == "unreachable statement" && diagnostic["severity"] == 2
    }));
}

#[test]
fn publishes_static_type_diagnostics() {
    let text = "fn value(input: i32) -> i32 { input } value(\"wrong\")";
    let result = rils_frontend::analysis::analyze(text);
    let output = diagnostics(text, &result);
    assert!(output.iter().any(|diagnostic| {
        diagnostic["message"]
            .as_str()
            .is_some_and(|message| message.contains("argument expects `i32`"))
            && diagnostic["severity"] == 1
    }));
}

#[test]
fn file_uris_round_trip_for_workspace_indexing() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("hello.rils");
    let uri = path_to_file_uri(&path);
    let decoded = file_uri_to_path(&uri).unwrap();
    assert_eq!(
        decoded.canonicalize().unwrap(),
        path.canonicalize().unwrap()
    );
}
