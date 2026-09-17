mod support;
use rils_frontend::{FunctionSignature, Type};
use rils_host::{HOST_CONTRACT_ABI_VERSION, HostContract};
use serde_json::json;
use std::fs;
use support::{Client, Scratch, fixture};

#[test]
#[ignore = "manual smoke test of the entire checkout, including unrelated project fixtures"]
fn repository_workspace_survives_invalid_test_manifests() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let path = root.to_string_lossy().replace('\\', "/");
    let path = path.strip_prefix("//?/").unwrap_or(&path);
    let mut client = Client::start(
        json!({"capabilities":{}, "rootUri":format!("file:///{path}")}),
        None,
    );
    client.open_healthy();
    client.assert_healthy();
    assert!(format!("{:?}", client.notifications).contains("invalid host manifest"));
    client.shutdown();
}

fn contract(id: u64) -> HostContract {
    let mut contract = HostContract::new();
    contract
        .register_function(
            id,
            "test::host_answer",
            FunctionSignature::fixed(vec![], Type::I32),
            "test",
        )
        .unwrap();
    contract
}

#[test]
fn startup_isolates_manifest_failures_and_reload_preserves_last_good_contract() {
    let scratch = Scratch::new();
    let good = scratch.0.join("a-good.rilhm");
    let conflict = scratch.0.join("z-conflict.rilhm");
    let abi = scratch.0.join("abi.rilhm");
    fs::write(&good, contract(101).to_manifest_bytes().unwrap()).unwrap();
    let mut conflicting = contract(102);
    conflicting.register_module("should_not_leak", 1).unwrap();
    fs::write(&conflict, conflicting.to_manifest_bytes().unwrap()).unwrap();
    fs::write(
        &abi,
        HostContract::with_versions(HOST_CONTRACT_ABI_VERSION + 1, 1)
            .unwrap()
            .to_manifest_bytes()
            .unwrap(),
    )
    .unwrap();
    let mut client = Client::start(
        json!({"capabilities":{}, "initializationOptions":{"hostManifestPaths":[
            fixture("invalid.rilhm"), good, conflict, abi, scratch.0.join("missing.rilhm")
        ]}}),
        None,
    );
    client.open_healthy();
    client.assert_healthy();
    let root_completion = client.request("textDocument/completion", json!({
        "textDocument":{"uri":"file:///resilience/healthy.rils"},"position":{"line":0,"character":0}
    }));
    assert!(!root_completion.to_string().contains("should_not_leak"));
    let messages = format!("{:?}", client.notifications);
    for expected in [
        "invalid host manifest",
        "failed to read host manifest",
        "uses ABI",
        "failed to merge host manifest",
    ] {
        assert!(
            messages.contains(expected),
            "missing {expected}: {messages}"
        );
    }
    client.notify(
        "textDocument/didOpen",
        json!({"textDocument":{
            "uri":"file:///resilience/host.rils", "languageId":"rils", "version":1,
            "text":fs::read_to_string(fixture("host.rils")).unwrap()
        }}),
    );
    let completion = |client: &mut Client| {
        client.request("textDocument/completion", json!({
        "textDocument":{"uri":"file:///resilience/host.rils"},"position":{"line":0,"character":6}
    })).to_string()
    };
    assert!(completion(&mut client).contains("host_answer"));
    client.notify(
        "rils/hostManifestChanged",
        json!({"hostManifestPaths":[fixture("invalid.rilhm")]}),
    );
    client.assert_healthy();
    assert!(completion(&mut client).contains("host_answer"));
    client.notify("rils/hostManifestChanged", json!({"hostManifestPaths":[]}));
    client.assert_healthy();
    assert!(!completion(&mut client).contains("host_answer"));
    client.notify(
        "rils/hostManifestChanged",
        json!({"hostManifestPaths":[good]}),
    );
    client.assert_healthy();
    assert!(completion(&mut client).contains("host_answer"));
    client.shutdown();
}

#[test]
fn malformed_notifications_and_requests_do_not_kill_the_service() {
    let mut client = Client::start(json!({"capabilities":{}}), None);
    client.open_healthy();
    client.assert_healthy();
    for (method, params) in [
        ("textDocument/didOpen", json!({})),
        (
            "textDocument/didChange",
            json!({"textDocument":{"uri":"file:///resilience/healthy.rils"}}),
        ),
        (
            "textDocument/didChange",
            json!({"textDocument":{"uri":"file:///resilience/healthy.rils"},"contentChanges":[{"text":"bad","range":{}}]}),
        ),
        ("textDocument/didClose", json!({"textDocument":{"uri":42}})),
        (
            "rils/hostManifestChanged",
            json!({"hostManifestPaths":[42]}),
        ),
        ("rils/hostManifestChanged", json!({})),
    ] {
        client.notify(method, params);
        client.assert_healthy();
    }
    assert_eq!(
        client.request("textDocument/hover", json!({}))["error"]["code"],
        -32602
    );
    assert_eq!(
        client.request("unsupported/test", json!({}))["error"]["code"],
        -32601
    );
    client.assert_healthy();
    client.shutdown();
}

#[test]
fn missing_sysroot_and_broken_workspace_do_not_block_loose_scripts() {
    let scratch = Scratch::new();
    let mut client = Client::start(
        json!({"capabilities":{}, "rootUri":format!("file:///{}", scratch.0.join("missing").to_string_lossy().replace('\\', "/")),
        "initializationOptions":{"hostManifestPaths":42}}),
        Some(&scratch.0),
    );
    client.open_healthy();
    client.assert_healthy();
    let messages = format!("{:?}", client.notifications);
    assert!(messages.contains("RILS_SYSROOT"));
    assert!(messages.contains("hostManifestPaths"));
    assert!(messages.contains("failed to"));
    client.shutdown();
}

#[test]
fn discovered_bad_manifest_and_unreadable_source_leave_other_files_available() {
    let scratch = Scratch::new();
    fs::create_dir_all(scratch.0.join("src")).unwrap();
    fs::create_dir_all(scratch.0.join(".rils/manifest")).unwrap();
    fs::copy(fixture("project/rils.toml"), scratch.0.join("rils.toml")).unwrap();
    fs::copy(
        fixture("project/src/main.rils"),
        scratch.0.join("src/main.rils"),
    )
    .unwrap();
    fs::copy(
        fixture("invalid.rilhm"),
        scratch.0.join(".rils/manifest/broken.rilhm"),
    )
    .unwrap();
    fs::write(scratch.0.join("src/unreadable.rils"), [0xff, 0xfe]).unwrap();
    let uri = format!("file:///{}", scratch.0.to_string_lossy().replace('\\', "/"));
    let mut client = Client::start(json!({"capabilities":{},"rootUri":uri}), None);
    client.open_healthy();
    client.assert_healthy();
    let messages = format!("{:?}", client.notifications);
    assert!(messages.contains("invalid host manifest"), "{messages}");
    assert!(messages.contains("failed to read source"), "{messages}");
    let result = client.request(
        "textDocument/documentSymbol",
        json!({"textDocument":{"uri":format!("{uri}/src/main.rils")}}),
    );
    assert!(
        result["result"]
            .as_array()
            .is_some_and(|symbols| !symbols.is_empty()),
        "{result}"
    );
    client.shutdown();
}
