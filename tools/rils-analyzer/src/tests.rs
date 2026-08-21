use super::{
    Document, Project, Server, SourceId, Type, analysis, diagnostics, file_uri_to_path,
    function_declaration, offset, path_to_file_uri, position, workspace_projects,
};
use lsp_server::Connection;
use rils_compiler::{
    HostCallKind, HostContract, HostReceiver, HostThreadAffinity, HostTypeTransport,
};
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
fn provides_generic_signature_help_for_option_map() {
    let text = "fn double(value: i32) -> usize { 2usize }\nlet value = Some(1);\nvalue.map(";
    let uri = "file:///builtin-generic-signature.rils".to_owned();
    let server = test_server(&uri, text, HashMap::new(), HostContract::new());

    let help = server
        .signature_help(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 10 }
        }))
        .unwrap();
    assert_eq!(
        help["signatures"][0]["label"],
        "fn map(fn(i32) -> U) -> Option<U>"
    );
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
        host_types: HashSet::new(),
        projects: Vec::new(),
        next_source_id: 1,
    }
}

fn completion_named(item: &serde_json::Value, name: &str) -> bool {
    item.get("filterText").unwrap_or(&item["label"]) == name
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
        host_types: HashSet::new(),
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
        Some("```rils\ntype IntBox = Box<i32>\n```\n\nmodule `crate`")
    );
}

#[test]
fn classifies_self_receivers_and_references_as_keywords() {
    let text = r#"struct Counter { value: i32 }
impl Counter {
    fn read(&self) -> i32 {
        self.value
    }
}"#;
    let uri = "file:///self-keyword.rils".to_owned();
    let server = test_server(&uri, text, HashMap::new(), HostContract::new());

    let tokens = server
        .semantic_tokens(&json!({ "textDocument": { "uri": uri } }))
        .unwrap();
    let self_tokens = tokens["data"]
        .as_array()
        .expect("semantic token data")
        .chunks_exact(5)
        .filter(|token| token[2] == 4 && token[3] == 11)
        .count();

    assert_eq!(self_tokens, 2, "{tokens}");
}

#[test]
fn resolves_self_types_and_associated_method_paths() {
    let text = r#"struct Counter { value: i32 }
impl Counter {
    fn new(value: i32) -> Self {
        Self { value: value }
    }
    fn answer() -> Self {
        Self::new(42)
    }
}"#;
    let uri = "file:///self-type.rils".to_owned();
    let server = test_server(&uri, text, HashMap::new(), HostContract::new());

    let self_type_offset = text.find("-> Self").expect("Self return type") + 3;
    let self_type_position = position(text, self_type_offset);
    let hover = server
        .hover(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": self_type_position[0], "character": self_type_position[1] }
        }))
        .unwrap();
    assert_eq!(
        hover["contents"]["value"].as_str(),
        Some("```rils\nstruct Counter {\n    value: i32,\n}\n```\n\nmodule `crate`")
    );
    let definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": self_type_position[0], "character": self_type_position[1] }
        }))
        .unwrap();
    assert_eq!(
        definition["range"]["start"],
        json!({ "line": 0, "character": 7 })
    );

    let method_offset = text.find("Self::new").expect("Self associated method") + 6;
    let method_position = position(text, method_offset);
    let hover = server
        .hover(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": method_position[0], "character": method_position[1] }
        }))
        .unwrap();
    assert_eq!(
        hover["contents"]["value"].as_str(),
        Some("```rils\nfn new(value: i32) -> Self\n```")
    );
    let definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": method_position[0], "character": method_position[1] }
        }))
        .unwrap();
    assert_eq!(
        definition["range"]["start"],
        json!({ "line": 2, "character": 7 })
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
        host_types: HashSet::new(),
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
    assert_eq!(functions[0]["label"], "fn add(i32, i32) -> i32");
    assert_eq!(functions[0]["insertText"], "add");
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
fn completes_inherited_methods_for_named_host_types() {
    let mut contract = HostContract::new();
    contract
        .register_type(
            "unity_engine::Object",
            None::<String>,
            HostTypeTransport::HostHandle,
        )
        .unwrap();
    contract
        .register_type(
            "unity_engine::GameObject",
            Some("unity_engine::Object"),
            HostTypeTransport::HostHandle,
        )
        .unwrap();
    contract
        .register_function(
            110,
            "unity_engine::object::get",
            FunctionSignature::fixed(Vec::new(), Type::named("unity_engine::GameObject")),
            "unity.object",
        )
        .unwrap();
    contract
        .register_function_with_options_and_receiver(
            111,
            "unity_engine::object::instance_id",
            FunctionSignature::fixed(vec![Type::named("unity_engine::Object")], Type::I32),
            "unity.object",
            HostCallKind::Direct,
            HostThreadAffinity::MainThread,
            Some(HostReceiver::Ref),
        )
        .unwrap();

    let host_functions = contract.signatures();
    let host_types = contract
        .types()
        .map(|declaration| declaration.name.clone())
        .collect::<HashSet<_>>();
    let text =
        "use unity_engine::*;\nfn inspect(object: GameObject) {\n    object.instance_id();\n}";
    let uri = "file:///named-host-completion.rils".to_owned();
    let (connection, _client) = Connection::memory();
    let mut documents = HashMap::new();
    documents.insert(
        uri.clone(),
        Document {
            source_id: SourceId::UNKNOWN,
            text: text.into(),
            analysis: rils_frontend::analysis::analyze_with_host_declarations(
                text,
                &host_functions,
                &host_types,
            ),
        },
    );
    let server = Server {
        connection,
        documents,
        workspace_documents: HashSet::new(),
        host_contract: contract,
        host_functions,
        host_types,
        projects: Vec::new(),
        next_source_id: 1,
    };

    let methods = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 14 }
        }))
        .unwrap();
    assert!(
        methods
            .as_array()
            .is_some_and(|items| items.iter().any(|item| {
                completion_named(item, "instance_id")
                    && item["detail"] == "fn instance_id(unity_engine::Object) -> i32"
            })),
        "{methods}"
    );
}

#[test]
fn completes_integer_intrinsic_methods_and_associated_functions() {
    let text = "let value: i32 = 1;\nvalue.checked_add(1i32);\ni16::try_from(1usize);\ni16::M;";
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
        host_types: HashSet::new(),
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
        methods.as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| completion_named(item, "checked_add"))
                && items
                    .iter()
                    .any(|item| completion_named(item, "checked_pow"))
        }),
        "{methods}"
    );

    let associated = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 9 }
        }))
        .unwrap();
    assert!(completion_named(&associated[0], "try_from"));

    let constants = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 3, "character": 6 }
        }))
        .unwrap();
    assert!(constants.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["label"] == "MIN" && item["kind"] == 21)
            && items.iter().any(|item| item["label"] == "MAX")
    }));
}

#[test]
fn completes_float_intrinsic_methods() {
    let text = "let value: f32 = 1f32;\nvalue.s;\nf32::INFINITY;";
    let uri = "file:///float-intrinsics.rils".to_owned();
    let (connection, _client) = Connection::memory();
    let mut documents = HashMap::new();
    let document_analysis =
        rils_frontend::analysis::analyze_with_source_id(text, SourceId::new(1), &HashMap::new());
    assert!(document_analysis.is_ok(), "{document_analysis:?}");
    documents.insert(
        uri.clone(),
        Document {
            source_id: SourceId::new(1),
            text: text.into(),
            analysis: document_analysis,
        },
    );
    let server = Server {
        connection,
        documents,
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        host_types: HashSet::new(),
        projects: Vec::new(),
        next_source_id: 2,
    };

    let completion = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 7 }
        }))
        .unwrap();
    assert!(
        completion.as_array().is_some_and(|items| {
            items.iter().any(|item| completion_named(item, "sqrt"))
                && items.iter().any(|item| completion_named(item, "signum"))
        }),
        "{completion}"
    );

    let constants = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 7 }
        }))
        .unwrap();
    assert!(constants.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["label"] == "INFINITY" && item["kind"] == 21)
    }));
}

#[test]
fn completes_builtin_members_for_values_and_expressions() {
    let text = r#"let text = "alpha";
text."#;
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
        host_types: HashSet::new(),
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
    let string_items = complete(1, 5);
    assert!(
        string_items.as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| completion_named(item, "starts_with"))
                && items.iter().any(|item| completion_named(item, "chars"))
                && items.iter().any(|item| completion_named(item, "split"))
                && items
                    .iter()
                    .any(|item| completion_named(item, "to_uppercase"))
        }),
        "{string_items}"
    );

    let expression_text = "Some(1).";
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
        host_types: HashSet::new(),
        projects: Vec::new(),
        next_source_id: 3,
    };
    let option_items = expression_server
        .completion(&json!({
            "textDocument": { "uri": expression_uri },
            "position": { "line": 0, "character": 8 }
        }))
        .unwrap();
    assert!(
        option_items.as_array().is_some_and(|items| {
            items.iter().any(|item| completion_named(item, "unwrap"))
                && items.iter().any(|item| completion_named(item, "unwrap_or"))
                && items.iter().any(|item| completion_named(item, "map"))
                && items.iter().any(|item| completion_named(item, "and_then"))
                && items.iter().any(|item| completion_named(item, "or_else"))
        }),
        "{option_items}"
    );
}

#[test]
fn completes_members_from_the_last_valid_prefix_after_an_unrelated_parse_error() {
    let text = "let value = 1;\nlet incomplete = ;\nvalue.";
    let uri = "file:///recovered-member-completion.rils";
    let server = test_server(uri, text, HashMap::new(), HostContract::new());
    assert!(analysis(server.documents.get(uri).unwrap()).is_none());

    let items = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 6 }
        }))
        .unwrap();
    assert!(
        items.as_array().is_some_and(|items| items
            .iter()
            .any(|item| completion_named(item, "checked_add"))),
        "{items}"
    );
}

#[test]
fn completes_hash_collection_types_constructors_and_members() {
    let module_uri = "file:///hash-module.rils";
    let module_server = test_server(
        module_uri,
        "std::collections::",
        HashMap::new(),
        HostContract::new(),
    );
    let module_items = module_server
        .completion(&json!({
            "textDocument": { "uri": module_uri },
            "position": { "line": 0, "character": 18 }
        }))
        .unwrap();
    assert!(
        module_items.as_array().is_some_and(|items| {
            items.iter().any(|item| item["label"] == "HashMap")
                && items.iter().any(|item| item["label"] == "HashSet")
        }),
        "{module_items}"
    );

    let constructor_uri = "file:///hash-constructor.rils";
    let constructor_server = test_server(
        constructor_uri,
        "HashMap::",
        HashMap::new(),
        HostContract::new(),
    );
    let constructor_items = constructor_server
        .completion(&json!({
            "textDocument": { "uri": constructor_uri },
            "position": { "line": 0, "character": 9 }
        }))
        .unwrap();
    assert!(
        constructor_items
            .as_array()
            .is_some_and(|items| { items.iter().any(|item| completion_named(item, "new")) }),
        "{constructor_items}"
    );

    let member_uri = "file:///hash-members.rils";
    let member_text = "let mut map: HashMap<string, i32> = HashMap::new();\nmap.";
    let member_server = test_server(member_uri, member_text, HashMap::new(), HostContract::new());
    let member_items = member_server
        .completion(&json!({
            "textDocument": { "uri": member_uri },
            "position": { "line": 1, "character": 4 }
        }))
        .unwrap();
    assert!(
        member_items.as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| completion_named(item, "contains_key"))
                && items
                    .iter()
                    .any(|item| completion_named(item, "get_cloned"))
                && items
                    .iter()
                    .any(|item| completion_named(item, "values_cloned"))
        }),
        "{member_items}"
    );
}

#[test]
fn completes_built_in_iterator_consumers_and_adapters() {
    let text = r#"let iterator = "abc".chars();
iterator."#;
    let uri = "file:///iterator-members.rils".to_owned();
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
        host_types: HashSet::new(),
        projects: Vec::new(),
        next_source_id: 2,
    };
    let items = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 9 }
        }))
        .unwrap();
    assert!(
        items.as_array().is_some_and(|items| {
            items.iter().any(|item| completion_named(item, "next"))
                && items.iter().any(|item| completion_named(item, "count"))
                && items
                    .iter()
                    .any(|item| completion_named(item, "collect_vec"))
                && items.iter().any(|item| completion_named(item, "take"))
                && items.iter().any(|item| completion_named(item, "rev"))
                && items.iter().any(|item| completion_named(item, "map"))
                && items.iter().any(|item| completion_named(item, "filter"))
                && items.iter().any(|item| completion_named(item, "fold"))
                && items.iter().any(|item| completion_named(item, "any"))
                && items.iter().any(|item| completion_named(item, "enumerate"))
        }),
        "{items}"
    );
}

#[test]
fn completes_iterator_defaults_for_custom_iterator_implementations() {
    let text = r#"struct Counter { value: i32 }
impl Iterator for Counter {
    type Item = i32;
    fn next(&mut self) -> Option<i32> { None }
}
let iterator = Counter { value: 0 };
iterator."#;
    let uri = "file:///custom-iterator-members.rils".to_owned();
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
        host_types: HashSet::new(),
        projects: Vec::new(),
        next_source_id: 2,
    };
    let items = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 6, "character": 9 }
        }))
        .unwrap();
    assert!(
        items.as_array().is_some_and(|items| {
            items.iter().any(|item| completion_named(item, "map"))
                && items.iter().any(|item| completion_named(item, "fold"))
                && items.iter().any(|item| completion_named(item, "position"))
        }),
        "{items}"
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
        "pub fn add(left: i32, right: i32) -> i32 { left + right }\n\
         pub struct Sum { value: i32 }\n\
         pub fn sum(left: i32, right: i32) -> Sum { Sum { value: left + right } }\n\
         fn hidden() {}",
    )
    .unwrap();
    fs::write(
        scripts.join("other.rils"),
        "pub fn sub(left: i32, right: i32) -> i32 { left - right }",
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
        host_types: HashSet::new(),
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
    assert!(
        completion.as_array().is_some_and(|items| {
            items.iter().any(|item| completion_named(item, "add"))
                && !items.iter().any(|item| completion_named(item, "hidden"))
        }),
        "{completion}"
    );
    let definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 20 }
        }))
        .unwrap();
    let expected_uri = path_to_file_uri(&scripts.join("math.rils"));
    assert_eq!(definition["uri"].as_str(), Some(expected_uri.as_str()));

    let type_import = "use crate::math::Sum;\nlet total: Sum = Sum { value: 3 };";
    server
        .update_document(uri.clone(), type_import.into())
        .unwrap();
    for (line, character) in [(0, 18), (1, 11)] {
        let hover = server
            .hover(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }))
            .unwrap();
        assert_eq!(
            hover["contents"]["value"].as_str(),
            Some("```rils\nstruct Sum {\n    value: i32,\n}\n```\n\nmodule `math`")
        );
    }

    let typed_import = "use crate::math::{sum};\nfn main() { let total = sum(1, 2); total.value; }";
    server
        .update_document(uri.clone(), typed_import.into())
        .unwrap();
    let hover = server
        .hover(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 26 }
        }))
        .unwrap();
    assert_eq!(
        hover["contents"]["value"].as_str(),
        Some("```rils\nfn sum(left: i32, right: i32) -> Sum\n```")
    );
    let use_hover = server
        .hover(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 19 }
        }))
        .unwrap();
    assert_eq!(use_hover["contents"], hover["contents"]);
    let hints = server
        .inlay_hints(&json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 1, "character": 50 }
            }
        }))
        .unwrap();
    assert!(
        hints
            .as_array()
            .is_some_and(|hints| { hints.iter().any(|hint| hint["label"] == ": Sum") })
    );
    let field_hover = server
        .hover(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 42 }
        }))
        .unwrap();
    assert_eq!(
        field_hover["contents"]["value"].as_str(),
        Some("```rils\nfield value: i32\n```\n\ntype `Sum`")
    );
    let field_definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 42 }
        }))
        .unwrap();
    assert_eq!(
        field_definition["uri"].as_str(),
        Some(expected_uri.as_str())
    );
    let literal_field_hover = server
        .hover(&json!({
            "textDocument": { "uri": expected_uri },
            "position": { "line": 2, "character": 50 }
        }))
        .unwrap();
    assert_eq!(literal_field_hover["contents"], field_hover["contents"]);
    let literal_field_definition = server
        .definition(&json!({
            "textDocument": { "uri": expected_uri },
            "position": { "line": 2, "character": 50 }
        }))
        .unwrap();
    assert_eq!(
        literal_field_definition["uri"].as_str(),
        Some(expected_uri.as_str())
    );

    let multiple_globs = "use crate::other::*;\nuse crate::math::*;\nfn main() { add(1, 2); }";
    server
        .update_document(uri.clone(), multiple_globs.into())
        .unwrap();
    let definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 13 }
        }))
        .unwrap();
    assert_eq!(definition["uri"].as_str(), Some(expected_uri.as_str()));
    let references = server
        .references(&json!({
            "textDocument": { "uri": expected_uri },
            "position": { "line": 0, "character": 8 },
            "context": { "includeDeclaration": true }
        }))
        .unwrap();
    assert_eq!(references.as_array().map(Vec::len), Some(2));

    let source_id = server.documents[&uri].source_id;
    let grouped = "use crate::math::{a";
    server.documents.insert(
        uri.clone(),
        Document {
            source_id,
            text: grouped.into(),
            analysis: rils_frontend::analysis::analyze_with_source_id(
                grouped,
                source_id,
                &server.host_functions,
            ),
        },
    );
    let completion = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": grouped.len() }
        }))
        .unwrap();
    assert!(completion.as_array().is_some_and(|items| {
        items.iter().any(|item| completion_named(item, "add"))
            && !items.iter().any(|item| completion_named(item, "hidden"))
    }));

    let grouped_valid = "use crate::math::{add};\nfn main() { add(1, 2); }";
    server.documents.insert(
        uri.clone(),
        Document {
            source_id,
            text: grouped_valid.into(),
            analysis: rils_frontend::analysis::analyze_with_source_id(
                grouped_valid,
                source_id,
                &server.host_functions,
            ),
        },
    );
    let definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 13 }
        }))
        .unwrap();
    assert_eq!(definition["uri"].as_str(), Some(expected_uri.as_str()));

    let glob = "use crate::math::*;\nfn main() { add(1, 2); }";
    server.update_document(uri.clone(), glob.into()).unwrap();
    let glob_analysis = server.documents[&uri].analysis.as_ref().unwrap();
    assert!(
        !glob_analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("undefined name `add`"))
    );
    let definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 13 }
        }))
        .unwrap();
    assert_eq!(definition["uri"].as_str(), Some(expected_uri.as_str()));

    let broken_glob = "use crate::math::*;\nfn main() { missing(1, 2); }";
    server
        .update_document(uri.clone(), broken_glob.into())
        .unwrap();
    let broken_analysis = server.documents[&uri].analysis.as_ref().unwrap();
    assert!(
        broken_analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("undefined name `missing`"))
    );

    let grouped_alias = "use crate::{math as m};\nfn main() { m::a }";
    server.documents.insert(
        uri.clone(),
        Document {
            source_id,
            text: grouped_alias.into(),
            analysis: rils_frontend::analysis::analyze_with_source_id(
                grouped_alias,
                source_id,
                &server.host_functions,
            ),
        },
    );
    let completion = server
        .completion(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 16 }
        }))
        .unwrap();
    assert!(
        completion
            .as_array()
            .is_some_and(|items| items.iter().any(|item| completion_named(item, "add")))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn enum_variant_paths_go_to_variant_declarations() {
    let text = "enum Priority {\n    Low,\n    High,\n}\nlet priority = Priority::High;\nmatch priority {\n    Priority::Low => 1,\n    Priority::High => 2,\n}";
    let uri = "file:///enum-variant.rils".to_owned();
    let server = test_server(&uri, text, HashMap::new(), HostContract::new());

    let definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 4, "character": 26 }
        }))
        .unwrap();

    assert_eq!(definition["uri"].as_str(), Some(uri.as_str()));
    assert_eq!(
        definition["range"],
        json!({
            "start": { "line": 2, "character": 4 },
            "end": { "line": 2, "character": 8 }
        })
    );

    let pattern_type_hover = server
        .hover(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 6, "character": 6 }
        }))
        .unwrap();
    assert_eq!(
        pattern_type_hover["contents"]["value"].as_str(),
        Some("```rils\nenum Priority {\n    Low,\n    High,\n}\n```\n\nmodule `crate`")
    );
    let pattern_type_definition = server
        .definition(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 6, "character": 6 }
        }))
        .unwrap();
    assert_eq!(pattern_type_definition["uri"].as_str(), Some(uri.as_str()));
    assert_eq!(
        pattern_type_definition["range"],
        json!({
            "start": { "line": 0, "character": 5 },
            "end": { "line": 0, "character": 13 }
        })
    );
    let variant_hover = server
        .hover(&json!({
            "textDocument": { "uri": uri },
            "position": { "line": 6, "character": 16 }
        }))
        .unwrap();
    assert_eq!(
        variant_hover["contents"]["value"].as_str(),
        Some("```rils\nPriority::Low\n```\n\ntype `Priority`")
    );
}

#[test]
fn task_board_fields_keep_types_and_definitions_in_members_and_literals() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap();
    let kanban = repository.join("examples/task_board/src/kanban.rils");
    let uri = path_to_file_uri(&kanban);
    let (connection, _client) = Connection::memory();
    let mut server = Server {
        connection,
        documents: HashMap::new(),
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        host_types: HashSet::new(),
        projects: workspace_projects(repository).unwrap(),
        next_source_id: 1,
    };
    server.load_workspace().unwrap();
    let document_count = server.documents.len();
    let vscode_uri = uri.replace("kanban.rils", "%6Banban.rils");
    let open_text = fs::read_to_string(&kanban).unwrap();
    server
        .update_document(vscode_uri.clone(), open_text)
        .unwrap();
    assert_eq!(server.documents.len(), document_count);
    assert!(analysis(&server.documents[&uri]).is_some_and(|analysis| {
        analysis.symbols.iter().any(|symbol| {
            symbol.name == "Vec" && symbol.kind == rils_frontend::analysis::SymbolKind::Type
        })
    }));

    let hints = server
        .inlay_hints(&json!({ "textDocument": { "uri": vscode_uri } }))
        .unwrap();
    assert!(
        hints
            .as_array()
            .is_some_and(|hints| hints.iter().any(|hint| {
                hint["label"] == ": &mut Vec<Task>"
                    && hint["position"] == json!({ "line": 18, "character": 17 })
            }))
    );
    assert!(
        hints
            .as_array()
            .is_some_and(|hints| hints.iter().any(|hint| {
                hint["label"] == ": Task"
                    && hint["position"] == json!({ "line": 27, "character": 16 })
            }))
    );

    for (line, character, expected) in [(18, 31, "Vec<Task>"), (37, 13, "i32")] {
        let hover = server
            .hover(&json!({
                "textDocument": { "uri": vscode_uri },
                "position": { "line": line, "character": character }
            }))
            .unwrap();
        assert_eq!(
            hover["contents"]["value"].as_str(),
            Some(
                format!(
                    "```rils\nfield {}: {expected}\n```\n\ntype `{}`",
                    if line == 18 { "tasks" } else { "active" },
                    if line == 18 { "Board" } else { "Summary" }
                )
                .as_str()
            )
        );
        let definition = server
            .definition(&json!({
                "textDocument": { "uri": vscode_uri },
                "position": { "line": line, "character": character }
            }))
            .unwrap();
        assert_eq!(definition["uri"].as_str(), Some(uri.as_str()));
    }

    let explicit_iterator = server.documents[&uri].text.replace(
        "for task in self.tasks {",
        "for task in self.tasks.into_iter() {",
    );
    server
        .update_document(vscode_uri.clone(), explicit_iterator)
        .unwrap();
    let hints = server
        .inlay_hints(&json!({ "textDocument": { "uri": vscode_uri } }))
        .unwrap();
    assert!(
        hints
            .as_array()
            .is_some_and(|hints| hints.iter().any(|hint| {
                hint["label"] == ": Task"
                    && hint["position"] == json!({ "line": 27, "character": 16 })
            }))
    );
    let hover = server
        .hover(&json!({
            "textDocument": { "uri": vscode_uri },
            "position": { "line": 27, "character": 35 }
        }))
        .unwrap();
    assert_eq!(
        hover["contents"]["value"].as_str(),
        Some("```rils\nfn into_iter() -> SequenceIterator<Task>\n```")
    );
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
        host_types: HashSet::new(),
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
fn discovers_and_merges_default_manifest_directory() {
    let root = std::env::temp_dir().join(format!(
        "rils-analyzer-project-manifest-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join(".rils/manifest")).unwrap();
    let mut first = HostContract::new();
    first
        .register_function(
            201,
            "unity::object::is_valid",
            FunctionSignature::fixed(vec![Type::named("HostHandle")], Type::Bool),
            "unity.object",
        )
        .unwrap();
    let mut second = HostContract::new();
    second
        .register_function(
            202,
            "unity::object::instance_id",
            FunctionSignature::fixed(vec![Type::named("HostHandle")], Type::I32),
            "unity.object",
        )
        .unwrap();
    fs::write(
        root.join(".rils/manifest/object.rilhm"),
        first.to_manifest_bytes().unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(".rils/manifest/identity.rilhm"),
        second.to_manifest_bytes().unwrap(),
    )
    .unwrap();
    let (connection, _client) = Connection::memory();
    let mut server = Server {
        connection,
        documents: HashMap::new(),
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        host_types: HashSet::new(),
        projects: vec![Project::from_root(&root).unwrap()],
        next_source_id: 1,
    };
    server.load_host_manifests(&json!({})).unwrap();
    assert!(
        server
            .host_functions
            .contains_key("unity::object::is_valid")
    );
    assert!(
        server
            .host_functions
            .contains_key("unity::object::instance_id")
    );
    fs::remove_dir_all(root).unwrap();
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

#[test]
fn workspace_projects_index_nested_projects_without_treating_package_paths_as_modules() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "rils-analyzer-workspace-projects-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("root.rils"), "let answer = 42;").unwrap();

    let nested = root.join("com.rils-lang.rils-for-unity");
    fs::create_dir_all(nested.join("src")).unwrap();
    fs::write(
        nested.join("rils.toml"),
        "[project]\nname = \"rils_for_unity\"\nscript_paths = [\"src\"]\n",
    )
    .unwrap();
    fs::write(nested.join("src/behaviour.rils"), "pub fn awake() {}").unwrap();

    let projects = workspace_projects(&root).unwrap();
    assert_eq!(projects.len(), 2);
    assert!(projects[0].module("root").is_some());
    assert!(projects[1].module("behaviour").is_some());
    fs::remove_dir_all(root).unwrap();
}
