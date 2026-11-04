mod common;
#[path = "cli/fixtures.rs"]
mod fixtures;

use base64::Engine as _;
use common::cellc_command;
use sha2::{Digest as _, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

include!("cli/surface.rs");
include!("cli/packages.rs");
include!("cli/registry.rs");
include!("cli/mcp.rs");
include!("cli/inspection.rs");
include!("cli/protocol.rs");
include!("cli/artifacts.rs");
include!("cli/scenarios.rs");
include!("cli/workspace.rs");
include!("cli/runtime.rs");
include!("cli/cache.rs");

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash_json_for_test<T: serde::Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap();
    hex_lower(&cellscript::ckb_blake2b256(&bytes))
}

fn lock_package(root: &std::path::Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("lock").output().unwrap();
    assert!(output.status.success(), "lock failed: {}", String::from_utf8_lossy(&output.stderr));
}

fn ckb_script_hash_for_test(code_hash: &str, hash_type: &str, args: &str) -> String {
    let code_hash_bytes = hex::decode(code_hash.trim_start_matches("0x")).unwrap();
    let hash_type_byte = match hash_type {
        "data" => 0u8,
        "type" => 1u8,
        "data1" => 2u8,
        "data2" => 4u8,
        other => panic!("unknown hash_type: {other}"),
    };
    let args_bytes = hex::decode(args.trim_start_matches("0x")).unwrap();
    let mut args_molecule = Vec::with_capacity(4 + args_bytes.len());
    args_molecule.extend_from_slice(&(args_bytes.len() as u32).to_le_bytes());
    args_molecule.extend_from_slice(&args_bytes);

    let header_size = 4 + 4 * 3;
    let field_sizes = [32usize, 1usize, args_molecule.len()];
    let mut cursor = header_size;
    let mut offsets = Vec::with_capacity(3);
    for size in field_sizes {
        offsets.push(cursor);
        cursor += size;
    }

    let mut serialized = Vec::with_capacity(cursor);
    serialized.extend_from_slice(&(cursor as u32).to_le_bytes());
    for offset in offsets {
        serialized.extend_from_slice(&(offset as u32).to_le_bytes());
    }
    serialized.extend_from_slice(&code_hash_bytes);
    serialized.push(hash_type_byte);
    serialized.extend_from_slice(&args_molecule);

    format!("0x{}", hex_lower(&cellscript::ckb_blake2b256(&serialized)))
}

fn run_mcp_messages(messages: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cellscript-mcp"))
        .env("CELLSCRIPT_CELLC", env!("CARGO_BIN_EXE_cellc"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cellscript-mcp");

    {
        let stdin = child.stdin.as_mut().expect("mcp stdin");
        for message in messages {
            writeln!(stdin, "{}", serde_json::to_string(&message).unwrap()).unwrap();
        }
    }
    drop(child.stdin.take());

    let mut stdout = String::new();
    child.stdout.as_mut().expect("mcp stdout").read_to_string(&mut stdout).unwrap();
    let output = child.wait_with_output().expect("wait for cellscript-mcp");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    stdout.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

fn protocol_graph_pool_source() -> String {
    r#"
module test

resource Pool has store {
    reserve: u64
}

action swap(input pool: Pool) -> output: Pool {
    transition pool -> output
    verification
        require output.reserve == pool.reserve
}
"#
    .to_string()
}

fn protocol_graph_explicit_role_source() -> String {
    r#"
module test

enum SwapState {
    Pending,
    Claimed,
}

resource SwapLock has store {
    state: SwapState,
    participant: Address,
}

flow SwapLock.state {
    initial Pending;
    terminal Claimed;
    Pending -> Claimed;
}

action claim(input swap: SwapLock, witness participant: Address) -> output: SwapLock {
    transition swap.state: Pending -> output.state: Claimed
    verification
        require participant == swap.participant
}
"#
    .to_string()
}

fn protocol_graph_conflicting_role_source() -> String {
    r#"
module test

enum SwapState {
    Pending,
    Refunded,
}

resource SwapLock has store {
    state: SwapState,
    initiator: Address,
    participant: Address,
}

flow SwapLock.state {
    initial Pending;
    terminal Refunded;
    Pending -> Refunded;
}

action refund(input swap: SwapLock, witness initiator: Address) -> output: SwapLock {
    transition swap.state: Pending -> output.state: Refunded
    verification
        require initiator == swap.participant
}
"#
    .to_string()
}

fn protocol_graph_weak_field_role_source() -> String {
    r#"
module test

resource Vault has store {
    owner: Address,
    amount: u64,
}

action inspect(input vault: Vault) -> output: Vault {
    verification
        require output.amount == vault.amount
}
"#
    .to_string()
}

/// Source whose `flow` block declares a genuine state-machine cycle (Open <-> Closed),
/// which must lower to a `RootRequired` template layout with `state_machine_acyclic = false`.
fn cyclic_flow_template_layout_source() -> String {
    r#"
module demo::cyclic_flow

resource Pool has store {
    state: u8
    reserve: u64
}

flow Pool.state {
    Open -> Closed;
    Closed -> Open;
}

action close(pool_before: Pool) -> pool_after: Pool {
    transition pool_before.state: Open -> pool_after.state: Closed
    verification
        require pool_after.reserve == pool_before.reserve
}

action reopen(pool_before: Pool) -> pool_after: Pool {
    transition pool_before.state: Closed -> pool_after.state: Open
    verification
        require pool_after.reserve == pool_before.reserve
}
"#
    .to_string()
}

/// Source whose `flow` block declares a linear (acyclic) state machine, which must
/// lower to a `PathOnlyAllowed` template layout with `state_machine_acyclic = true`.
fn acyclic_flow_template_layout_source() -> String {
    r#"
module demo::acyclic_flow

resource Grant has store {
    state: u8
    amount: u64
}

flow Grant.state {
    Granted -> Claimable;
    Claimable -> FullyClaimed;
}

action claim(grant_before: Grant) -> grant_after: Grant {
    transition grant_before.state: Claimable -> grant_after.state: FullyClaimed
    verification
        require grant_after.amount == grant_before.amount
}
"#
    .to_string()
}

fn write_publish_fixture_package(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2026"
name = "demo"
version = "1.2.3"
namespace = "cellscript"
description = "Demo package"
license = "MIT"
repository = "https://example.com/cellscript/demo"
entry = "src/main.cell"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module demo::main

action identity(value: u64) -> u64 {
    verification
        value
}
"#,
    )
    .unwrap();
}

fn write_declared_artifact_fixture(root: &std::path::Path) {
    std::fs::write(
        root.join("Artifact.toml"),
        r#"schema = "cellscript-registry-artifact"
namespace = "cellscript"
name = "rust-contract"
release = "1.0.0"
kind = "deployable_contract"
language = "rust"
bundle = "artifact-bundle.json"
description = "Rust CKB contract"
repository = "https://example.com/cellscript/rust-contract"
"#,
    )
    .unwrap();
    let abi_hash = hex::encode(cellscript::ckb_blake2b256(b"abi"));
    let profile_contract = serde_json::json!({
        "schema": "cellscript-registry-profile-contract-v1",
        "artifact_kind": "deployable_contract",
        "profile": "ckb_executable",
        "build": {
            "target": "riscv64imac-unknown-none-elf",
            "toolchain": "rustc 1.97.1",
            "profile": "release",
            "source_revision": "0123456789abcdef",
            "reproducible": false
        },
        "security": { "status": "review_required" },
        "ckb": {
            "vm_version": "2",
            "script_role": "type",
            "hash_type": "data1",
            "dep_type": "code",
            "abi_hash": abi_hash
        }
    });
    let bundle = serde_json::json!({
        "schema": "cellscript-registry-bundle",
        "namespace": "cellscript",
        "name": "rust-contract",
        "release": "1.0.0",
        "profile": "ckb_executable",
        "manifest_json": cellscript::package::registry::canonical_artifact_contract_json(&profile_contract).unwrap(),
        "objects": [
            {"role":"source","content_base64":"c291cmNl"},
            {"role":"executable","content_base64":"ZWxm"},
            {"role":"abi","content_base64":"YWJp"}
        ]
    });
    std::fs::write(root.join("artifact-bundle.json"), serde_json::to_vec_pretty(&bundle).unwrap()).unwrap();
}

fn locked_build_from_metadata_for_test(metadata: &cellscript::CompileMetadata) -> cellscript::package::LockedBuildInfo {
    let abi = serde_json::json!({
        "edition": metadata.edition,
        "compatibility_profile": &metadata.compatibility_profile,
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": {
            "metadata": metadata.metadata_schema_version,
            "source": metadata.source_metadata_schema_version,
            "artifact": metadata.artifact_metadata_schema_version,
            "constraints": metadata.constraints_metadata_schema_version,
        },
        "target_profile": metadata.target_profile.name.as_str(),
        "types": &metadata.types,
        "actions": &metadata.actions,
        "functions": &metadata.functions,
        "locks": &metadata.locks,
        "molecule_schema_manifest": &metadata.molecule_schema_manifest,
        "cell_data_codec_manifest": &metadata.cell_data_codec_manifest,
    });
    cellscript::package::LockedBuildInfo {
        edition: cellscript::CURRENT_EDITION,
        compatibility_profile_hash: hash_json_for_test(&metadata.compatibility_profile),
        compiler_version: Some(metadata.compiler_version.clone()),
        target_profile: Some(metadata.target_profile.name.clone()),
        artifact_hash: metadata.artifact_hash.clone(),
        metadata_hash: Some(hash_json_for_test(metadata)),
        schema_hash: Some(metadata.molecule_schema_manifest.manifest_hash.clone()),
        cell_data_codec_manifest_hash: Some(metadata.cell_data_codec_manifest.manifest_hash.clone()),
        abi_hash: Some(hash_json_for_test(&abi)),
        constraints_hash: Some(hash_json_for_test(&metadata.constraints)),
    }
}

fn start_mock_ckb_rpc(responses: Vec<(&'static str, serde_json::Value)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for (expected_method, result) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request_body(&mut stream);
            let request_json: serde_json::Value = serde_json::from_slice(&request).unwrap();
            assert_eq!(request_json["method"], expected_method);
            let response_body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_json["id"].clone(),
                "result": result,
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    format!("http://{}", addr)
}

#[derive(Debug)]
struct MockRegistryRequest {
    path: String,
    headers: Vec<(String, String)>,
    body: serde_json::Value,
}

impl MockRegistryRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find_map(|(header_name, value)| header_name.eq_ignore_ascii_case(name).then_some(value.as_str()))
    }
}

fn start_mock_registry_api_capture_request(
    response_body: serde_json::Value,
) -> (String, std::sync::mpsc::Receiver<MockRegistryRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (path, headers, request) = read_http_request_path_headers_and_body(&mut stream);
        let request_json: serde_json::Value = serde_json::from_slice(&request).unwrap();
        tx.send(MockRegistryRequest { path, headers, body: request_json }).unwrap();
        let response_body = response_body.to_string();
        let response = format!(
            "HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{}", addr), rx)
}

fn start_mock_registry_api_retry_then_success(
    response_body: serde_json::Value,
) -> (String, std::sync::mpsc::Receiver<MockRegistryRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for status in ["503 Service Unavailable", "202 Accepted"] {
            let (mut stream, _) = listener.accept().unwrap();
            let (path, headers, request) = read_http_request_path_headers_and_body(&mut stream);
            let request_json: serde_json::Value = serde_json::from_slice(&request).unwrap();
            tx.send(MockRegistryRequest { path, headers, body: request_json }).unwrap();
            let response_body = if status.starts_with("202") {
                response_body.to_string()
            } else {
                serde_json::json!({"error": {"code": "temporary_unavailable"}}).to_string()
            };
            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (format!("http://{}", addr), rx)
}

fn start_mock_registry_api_expect_path(
    expected_path: &str,
    response_body: serde_json::Value,
) -> (String, std::sync::mpsc::Receiver<serde_json::Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let expected_path = expected_path.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (path, request) = read_http_request_path_and_body(&mut stream);
        assert_eq!(path, expected_path);
        let request_json: serde_json::Value = serde_json::from_slice(&request).unwrap();
        tx.send(request_json).unwrap();
        let response_body = response_body.to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{}", addr), rx)
}

fn read_http_request_body(stream: &mut std::net::TcpStream) -> Vec<u8> {
    read_http_request_path_and_body(stream).1
}

fn read_http_request_path_and_body(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    let (path, _headers, body) = read_http_request_path_headers_and_body(stream);
    (path, body)
}

fn read_http_request_path_headers_and_body(stream: &mut std::net::TcpStream) -> (String, Vec<(String, String)>, Vec<u8>) {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "mock RPC request ended before headers");
        request.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let path = headers.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/").to_string();
            let parsed_headers = headers
                .lines()
                .skip(1)
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    Some((name.trim().to_string(), value.trim().to_string()))
                })
                .collect();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let body_start = header_end + 4;
            while request.len() < body_start + content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert_ne!(read, 0, "mock RPC request ended before body");
                request.extend_from_slice(&buffer[..read]);
            }
            return (path, parsed_headers, request[body_start..body_start + content_length].to_vec());
        }
    }
}

fn write_live_registry_fixture(root: &std::path::Path, data_hash: &str) {
    write_live_registry_fixture_with(root, data_hash, data_hash, "data1", None);
}

fn write_live_registry_fixture_with(root: &std::path::Path, data_hash: &str, code_hash: &str, hash_type: &str, type_id: Option<&str>) {
    let out_point = "0xaaaa:0".to_string();
    let mut lockfile = cellscript::package::Lockfile::new();
    lockfile.package = cellscript::package::LockfilePackageInfo {
        edition: cellscript::CURRENT_EDITION,
        name: "token".to_string(),
        version: "1.0.0".to_string(),
        namespace: Some("cellscript".to_string()),
        source_hash: Some("source_hash".to_string()),
        compiler_source_hash: None,
        compiler_requirement: format!(">={}", cellscript::VERSION),
        resolver_compiler_version: cellscript::VERSION.to_string(),
    };
    lockfile.package_build = Some(cellscript::package::LockedBuildInfo {
        edition: cellscript::CURRENT_EDITION,
        compatibility_profile_hash: "test-compatibility-profile".to_string(),
        compiler_version: Some("0.20.0".to_string()),
        target_profile: Some("ckb".to_string()),
        artifact_hash: Some("artifact_hash".to_string()),
        metadata_hash: Some("metadata_hash".to_string()),
        schema_hash: Some("schema_hash".to_string()),
        cell_data_codec_manifest_hash: Some("codec_manifest_hash".to_string()),
        abi_hash: Some("abi_hash".to_string()),
        constraints_hash: Some("constraints_hash".to_string()),
    });
    lockfile.deployment.insert(
        "aggron4".to_string(),
        cellscript::package::LockfileDeploymentRef {
            record: out_point.clone(),
            record_hash: None,
            code_hash: Some(code_hash.to_string()),
            out_point: Some(out_point.clone()),
            data_hash: Some(data_hash.to_string()),
        },
    );
    lockfile.write_to_root(root).unwrap();

    let deployed = cellscript::package::DeployedManifest {
        version: cellscript::package::DeployedManifest::CURRENT_VERSION,
        schema: cellscript::package::DEPLOYED_MANIFEST_SCHEMA.to_string(),
        package: cellscript::package::DeployedPackageInfo {
            edition: cellscript::CURRENT_EDITION,
            name: "token".to_string(),
            version: "1.0.0".to_string(),
            source_hash: Some("source_hash".to_string()),
        },
        build: Some(cellscript::package::DeployedBuildInfo {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            compiler_version: Some("0.20.0".to_string()),
            artifact_hash: Some("artifact_hash".to_string()),
            metadata_hash: Some("metadata_hash".to_string()),
            schema_hash: Some("schema_hash".to_string()),
            cell_data_codec_manifest_hash: Some("codec_manifest_hash".to_string()),
            abi_hash: Some("abi_hash".to_string()),
            constraints_hash: Some("constraints_hash".to_string()),
        }),
        deployments: vec![cellscript::package::DeploymentRecord {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            network: "aggron4".to_string(),
            chain_id: "ckb-testnet".to_string(),
            tx_hash: "0xaaaa".to_string(),
            output_index: 0,
            code_hash: code_hash.to_string(),
            hash_type: hash_type.to_string(),
            dep_type: "code".to_string(),
            data_hash: data_hash.to_string(),
            out_point,
            artifact_hash: Some("artifact_hash".to_string()),
            metadata_hash: Some("metadata_hash".to_string()),
            schema_hash: Some("schema_hash".to_string()),
            cell_data_codec_manifest_hash: Some("codec_manifest_hash".to_string()),
            abi_hash: Some("abi_hash".to_string()),
            constraints_hash: Some("constraints_hash".to_string()),
            compiler_version: Some("0.20.0".to_string()),
            type_id: type_id.map(str::to_string),
            script_role: Some(cellscript::package::ScriptRole::Type),
            status: Some(cellscript::package::DeploymentStatus::Active),
            upgrade_lineage: None,
            audit_report_hash: None,
            publisher_signature: None,
            cell_deps: vec![],
        }],
    };
    deployed.write_to_root(root).unwrap();
}

fn live_cell_rpc_result(status: &str, data_hash: &str) -> serde_json::Value {
    live_cell_rpc_result_with_type(status, data_hash, serde_json::Value::Null)
}

fn live_cell_rpc_result_with_type(status: &str, data_hash: &str, type_script: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "status": status,
        "cell": {
            "output": {
                "capacity": "0x0",
                "lock": {
                    "code_hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "hash_type": "data1",
                    "args": "0x"
                },
                "type": type_script
            },
            "data": {
                "content": "0x00",
                "hash": data_hash
            }
        }
    })
}

/// Build a fully-valid offline registry verify fixture whose single deployment
/// optionally declares an `upgrade_lineage`. Used by the lineage tests.
fn write_offline_fixture_with_lineage(root: &std::path::Path, lineage: Option<&str>) {
    let out_point = "0xbbbb:0".to_string();
    let mut lockfile = cellscript::package::Lockfile::new();
    lockfile.package = cellscript::package::LockfilePackageInfo {
        edition: cellscript::CURRENT_EDITION,
        name: "token".to_string(),
        version: "1.0.0".to_string(),
        namespace: Some("cellscript".to_string()),
        source_hash: Some("source_hash".to_string()),
        compiler_source_hash: None,
        compiler_requirement: format!(">={}", cellscript::VERSION),
        resolver_compiler_version: cellscript::VERSION.to_string(),
    };
    lockfile.package_build = Some(cellscript::package::LockedBuildInfo {
        edition: cellscript::CURRENT_EDITION,
        compatibility_profile_hash: "test-compatibility-profile".to_string(),
        compiler_version: Some("0.20.0".to_string()),
        target_profile: Some("ckb".to_string()),
        artifact_hash: Some("artifact_hash".to_string()),
        metadata_hash: Some("metadata_hash".to_string()),
        schema_hash: Some("schema_hash".to_string()),
        cell_data_codec_manifest_hash: Some("codec_manifest_hash".to_string()),
        abi_hash: Some("abi_hash".to_string()),
        constraints_hash: Some("constraints_hash".to_string()),
    });
    lockfile.deployment.insert(
        "aggron4".to_string(),
        cellscript::package::LockfileDeploymentRef {
            record: out_point.clone(),
            record_hash: None,
            code_hash: Some("0xbbbb".to_string()),
            out_point: Some(out_point.clone()),
            data_hash: Some("0xcccc".to_string()),
        },
    );
    lockfile.write_to_root(root).unwrap();

    let deployed = cellscript::package::DeployedManifest {
        version: cellscript::package::DeployedManifest::CURRENT_VERSION,
        schema: cellscript::package::DEPLOYED_MANIFEST_SCHEMA.to_string(),
        package: cellscript::package::DeployedPackageInfo {
            edition: cellscript::CURRENT_EDITION,
            name: "token".to_string(),
            version: "1.0.0".to_string(),
            source_hash: Some("source_hash".to_string()),
        },
        build: Some(cellscript::package::DeployedBuildInfo {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            compiler_version: Some("0.20.0".to_string()),
            artifact_hash: Some("artifact_hash".to_string()),
            metadata_hash: Some("metadata_hash".to_string()),
            schema_hash: Some("schema_hash".to_string()),
            cell_data_codec_manifest_hash: Some("codec_manifest_hash".to_string()),
            abi_hash: Some("abi_hash".to_string()),
            constraints_hash: Some("constraints_hash".to_string()),
        }),
        deployments: vec![cellscript::package::DeploymentRecord {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            network: "aggron4".to_string(),
            chain_id: "ckb-testnet".to_string(),
            tx_hash: "0xbbbb".to_string(),
            output_index: 0,
            code_hash: "0xbbbb".to_string(),
            hash_type: "data1".to_string(),
            dep_type: "code".to_string(),
            data_hash: "0xcccc".to_string(),
            out_point,
            artifact_hash: Some("artifact_hash".to_string()),
            metadata_hash: Some("metadata_hash".to_string()),
            schema_hash: Some("schema_hash".to_string()),
            cell_data_codec_manifest_hash: Some("codec_manifest_hash".to_string()),
            abi_hash: Some("abi_hash".to_string()),
            constraints_hash: Some("constraints_hash".to_string()),
            compiler_version: Some("0.20.0".to_string()),
            type_id: None,
            script_role: Some(cellscript::package::ScriptRole::Type),
            status: Some(cellscript::package::DeploymentStatus::Active),
            upgrade_lineage: lineage.map(str::to_string),
            audit_report_hash: None,
            publisher_signature: None,
            cell_deps: vec![],
        }],
    };
    deployed.write_to_root(root).unwrap();
}

/// Source declaring an xUDT group-amount-conservation invariant, with the
/// action calling the matching `xudt::require_group_amount_conserved()` helper
/// so the invariant lowers to a checked runtime helper.
fn xudt_conserved_source_with_runtime_helper() -> &'static str {
    r#"
module demo::main

resource Token has store, create, consume {
    amount: u128,
}

invariant xudt_group_transfer_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)
}

action transfer(input: Token) -> output: Token {
    verification
        xudt::require_group_amount_conserved()
        preserve output from input {
            amount
        }
}
"#
}

/// Source declaring the same invariant but without the action-side helper call, so
/// the aggregate is recognised as runtime-helper-backed but not yet discharged.
fn xudt_conserved_source_without_helper_call() -> &'static str {
    r#"
module demo::main

resource Token has store, create, consume {
    amount: u128,
}

invariant xudt_group_transfer_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)
}

action transfer(input: Token) -> output: Token {
    verification
        preserve output from input {
            amount
        }
}
"#
}

fn write_xudt_package(root: &std::path::Path, source: &str) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2026"
name = "demo"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src").join("main.cell"), source).unwrap();
}

fn atomic_swap_inline_source() -> &'static str {
    r#"
module demo::atomic_swap

resource SwapLock has store, create, consume, replace, burn, read_ref {
    swap_id: Hash,
    initiator: Address,
    participant: Address,
    hashlock: Hash,
    timeout_timepoint: u64,
    asset_type: AssetType,
    amount: u64,
    state: u8,
}

receipt PreimageClaim has create, consume, burn {
    swap_id: Hash,
    preimage: Hash,
    claimed_by: Address,
    claimed_at: u64,
}

enum AssetType {
    Native,
    Token(Hash),
}

flow SwapLock.state {
    initial Pending;
    terminal Claimed, Refunded;

    Pending -> Claimed;
    Pending -> Refunded;
}

action initiate_swap(swap_id: Hash, initiator: Address, participant: Address, hashlock: Hash, timeout_timepoint: u64, asset_type: AssetType, amount: u64) -> swap_lock: SwapLock {
    verification
        require amount > 0, "zero amount"
        require timeout_timepoint > 0, "zero timeout"
        create swap_lock = SwapLock { swap_id, initiator, participant, hashlock, timeout_timepoint, asset_type, amount, state: Pending } with_lock(initiator)

}

action claim_with_preimage(lock: SwapLock, preimage: Hash, claimed_by: Address, current_timepoint: u64) -> (claim: PreimageClaim, updated_lock: SwapLock) {
    transition lock.state: Pending -> updated_lock.state: Claimed
    verification
        require claimed_by == lock.participant, "not the participant"
        require current_timepoint < lock.timeout_timepoint, "claim window expired"
        require hash_blake2b(preimage) == lock.hashlock, "wrong preimage"
        consume lock
        create claim = PreimageClaim { swap_id: lock.swap_id, preimage, claimed_by, claimed_at: current_timepoint }
        create updated_lock = SwapLock { swap_id: lock.swap_id, initiator: lock.initiator, participant: lock.participant, hashlock: lock.hashlock, timeout_timepoint: lock.timeout_timepoint, asset_type: lock.asset_type, amount: lock.amount, state: Claimed } with_lock(lock.initiator)

}

action refund_after_timeout(lock: SwapLock, refunded_by: Address, current_timepoint: u64) -> updated_lock: SwapLock {
    transition lock.state: Pending -> updated_lock.state: Refunded
    verification
        require refunded_by == lock.initiator, "not the initiator"
        require current_timepoint >= lock.timeout_timepoint + 100, "timeout not reached"
        consume lock
        create updated_lock = SwapLock { swap_id: lock.swap_id, initiator: lock.initiator, participant: lock.participant, hashlock: lock.hashlock, timeout_timepoint: lock.timeout_timepoint, asset_type: lock.asset_type, amount: lock.amount, state: Refunded } with_lock(refunded_by)

}
"#
}

// ── Workspace e2e tests ──────────────────────────────────────────────────────

fn write_workspace_member(root: &std::path::Path, directory: &str, name: &str, dependencies: &[(&str, &str)]) -> std::path::PathBuf {
    let member = root.join(directory);
    std::fs::create_dir_all(member.join("src")).unwrap();
    let mut manifest =
        format!("[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2026\"\nname = \"{name}\"\nversion = \"1.0.0\"\n");
    for (alias, path) in dependencies {
        manifest.push_str(&format!("\n[dependencies.{alias}]\npath = \"{path}\"\n"));
    }
    std::fs::write(member.join("Cell.toml"), manifest).unwrap();
    std::fs::write(
        member.join("src/main.cell"),
        format!("module {name}\n\naction run() -> u64 {{\n    verification\n        1\n}}\n"),
    )
    .unwrap();
    member
}

// ── Incremental compilation e2e tests ────────────────────────────────────────
