use super::check_policy::validate_build_entry_selection;
use super::registry_requests::{activate_registry_key_after_wallet_approval, registry_authorisation_status_removes_pending_key};
use super::typescript_builder::{
    action_requires_entry_witness, builder_action_error_contexts_json, typescript_builder_index, typescript_builder_manifest,
};
use super::*;
use crate::cli::artifact::ArtifactOperation;

#[test]
fn a_disappeared_generated_lockfile_returns_a_typed_io_diagnostic() {
    let root = tempfile::tempdir().unwrap();
    let error = read_generated_lockfile(root.path()).unwrap_err();
    assert_eq!(error.code.as_deref(), Some("CLI0003"));
    assert_eq!(error.category, crate::error::CompileErrorCategory::Io);
    assert_eq!(error.category.exit_code(), 74);
}

#[test]
fn certification_uses_its_typed_argument_declaration_and_global_output_policy() {
    for output in [vec!["--json"], vec!["--message-format", "json"]] {
        let mut arguments = vec!["cellc"];
        arguments.extend(output);
        arguments.extend([
            "certify",
            "--plugin",
            "novaseal-profile-v0",
            "--repo-root",
            "evidence",
            "--report",
            "input.json",
            "-o",
            "result.json",
            "--require-production",
        ]);
        let matches = CliParser::command().try_get_matches_from(arguments).unwrap();
        let Command::Certify(args) = CliParser::parse_matches(matches).unwrap() else { panic!("expected certify command") };
        assert_eq!(args.plugin, "novaseal-profile-v0");
        assert_eq!(args.repo_root, Some(PathBuf::from("evidence")));
        assert_eq!(args.report, Some(PathBuf::from("input.json")));
        assert_eq!(args.output, Some(PathBuf::from("result.json")));
        assert!(args.json && args.require_production);
    }
    let error = CliParser::command().try_get_matches_from(["cellc", "certify"]).unwrap_err();
    assert_eq!(error.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn publish_snapshot_carries_exactly_the_authoritative_source_files() {
    // #27: the publish snapshot's source files must be the exact set
    // compilation and the signed source_hash cover — no stray root files,
    // no drift between the three collectors.
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(
            root.path().join("Cell.toml"),
            b"[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"demo\"\nnamespace = \"cellscript\"\nversion = \"1.2.3\"\nentry = \"src/main.cell\"\n",
        )
        .unwrap();
    std::fs::write(root.path().join("src/main.cell"), b"module demo;\n").unwrap();
    std::fs::write(root.path().join("src/helper.cell"), b"module demo::helper;\n").unwrap();
    std::fs::write(root.path().join("rogue.cell"), b"module rogue;\n").unwrap();
    std::fs::write(root.path().join("Cell.lock"), b"lock bytes\n").unwrap();
    let manifest: PackageManifest = toml::from_str(&std::fs::read_to_string(root.path().join("Cell.toml")).unwrap()).unwrap();

    let bytes = build_generated_source_snapshot_bytes(root.path(), &manifest).unwrap();
    let snapshot: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let paths =
        snapshot["files"].as_array().unwrap().iter().map(|entry| entry["path"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert!(paths.contains(&"Cell.toml".to_string()));
    assert!(paths.contains(&"Cell.lock".to_string()));
    assert!(paths.contains(&"src/main.cell".to_string()));
    assert!(paths.contains(&"src/helper.cell".to_string()));
    assert!(!paths.iter().any(|path| path == "rogue.cell"), "stray root source must not enter the snapshot");
    for entry in snapshot["files"].as_array().unwrap() {
        let path = root.path().join(entry["path"].as_str().unwrap());
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            entry["blake2b256"].as_str().unwrap(),
            &crate::hex_encode(&crate::ckb_blake2b256(&bytes)),
            "snapshot entry hash must match disk bytes for {}",
            entry["path"].as_str().unwrap()
        );
    }
}

#[test]
fn policy_artifact_selection_is_explicit_and_preserved_by_build_and_check() {
    let build = CliParser::command().try_get_matches_from(["cellc", "build", "--artifact", "token-policy"]).unwrap();
    let Command::Build(build) = CliParser::parse_matches(build).unwrap() else {
        panic!("expected build command");
    };
    assert_eq!(build.artifact.as_deref(), Some("token-policy"));
    assert!(build.entry_action.is_none() && build.entry_lock.is_none());
    validate_build_entry_selection(&build).unwrap();

    let check = CliParser::command().try_get_matches_from(["cellc", "check", "--artifact", "token-policy", "--all-targets"]).unwrap();
    let Command::Check(check) = CliParser::parse_matches(check).unwrap() else {
        panic!("expected check command");
    };
    assert_eq!(check.artifact.as_deref(), Some("token-policy"));
    assert!(check.all_targets);

    for command in ["build", "check"] {
        let matches = CliParser::command().try_get_matches_from(["cellc", command]).unwrap();
        match CliParser::parse_matches(matches).unwrap() {
            Command::Build(args) => assert!(args.artifact.is_none()),
            Command::Check(args) => assert!(args.artifact.is_none()),
            _ => panic!("unexpected command"),
        }
    }
}

#[test]
fn policy_artifact_selection_rejects_mixed_entry_authorities() {
    for flag in ["--entry-action", "--entry-lock"] {
        let error =
            CliParser::command().try_get_matches_from(["cellc", "build", "--artifact", "token-policy", flag, "main"]).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    }
    for args in [
        BuildArgs { artifact: Some("token-policy".into()), entry_action: Some("main".into()), ..Default::default() },
        BuildArgs { artifact: Some("token-policy".into()), entry_lock: Some("main".into()), ..Default::default() },
        BuildArgs { entry_action: Some("main".into()), entry_lock: Some("main".into()), ..Default::default() },
    ] {
        assert!(validate_build_entry_selection(&args).unwrap_err().message.contains("mutually exclusive"));
    }
}

#[test]
fn add_dependency_parses_chain_identity_safe_environment_policy() {
    let matches = CliParser::command()
        .try_get_matches_from(["cellc", "add", "peer", "--path", "deps/peer", "--use-environment", "production"])
        .unwrap();
    let Command::Add(args) = CliParser::parse_matches(matches).unwrap() else {
        panic!("expected add command");
    };
    assert_eq!(args.use_environment.as_deref(), Some("production"));
    assert!(!args.environment_independent);
    let Dependency::Detailed(dependency) = dependency_from_add_args(&args) else {
        panic!("expected detailed dependency");
    };
    assert_eq!(dependency.use_environment.as_deref(), Some("production"));

    let independent = CliParser::command().try_get_matches_from(["cellc", "add", "peer", "--environment-independent"]).unwrap();
    let Command::Add(args) = CliParser::parse_matches(independent).unwrap() else {
        panic!("expected add command");
    };
    assert!(args.environment_independent);
    assert!(matches!(dependency_from_add_args(&args), Dependency::Detailed(DetailedDependency { environment_independent: true, .. })));

    let conflict = CliParser::command().try_get_matches_from([
        "cellc",
        "add",
        "peer",
        "--use-environment",
        "production",
        "--environment-independent",
    ]);
    assert_eq!(conflict.unwrap_err().kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn generated_builder_witness_requirement_matches_the_entry_payload_contract() {
    let cases = [
        ("action main() -> u64 {  return 0; }", false),
        (
            "resource Token has consume { amount: u64 }\n\
                 action main(input token: Token) -> u64 {  consume token; return 0; }",
            false,
        ),
        (
            "shared Config { value: u64 }\n\
                 action main(read config: Config) -> u64 {  require config.value > 0; return 0; }",
            false,
        ),
        (
            "resource Token has consume { amount: u64 }\n\
                 action main(token: Token) -> u64 {  consume token; return 0; }",
            false,
        ),
        (
            "shared Config { value: u64 }\n\
                 action main(read config: Config, witness expected: u64) -> u64 {\n\
                      require config.value == expected; return 0;\n\
                 }",
            true,
        ),
        // A zero-byte unit still selects the canonical entry payload
        // envelope; testing whether encoding [] fails would miss it.
        ("action main(witness marker: ()) -> u64 {  return 0; }", true),
    ];
    {
        let edition = crate::CURRENT_EDITION;
        for (declarations, expected) in cases {
            let source = format!("module generated_builder_witness;\n{declarations}\n");
            let metadata = crate::compile_metadata(&source, edition, None).expect("builder fixture compiles");
            let action = &metadata.actions[0];
            assert_eq!(action_requires_entry_witness(action), expected, "{edition:?}: {declarations}");
            let contexts = builder_action_error_contexts_json(&[action]);
            assert_eq!(contexts[0]["entry_witness_required"], expected, "error context must agree with payload ABI");
            let manifest = typescript_builder_manifest("@test/bindings", &metadata, &[action], "test-metadata", None, None).unwrap();
            assert_eq!(manifest["actions"][0]["entry_witness_required"], expected, "builder manifest must agree with payload ABI");
        }
    }
}

#[test]
fn generated_builder_preserves_typed_commitment_and_opening_abi() {
    let source = r#"
module generated_builder_committed_state;
struct State { counter: u64, }
action reveal(witness expected: Commitment<State>, witness opening: Opening<State>) -> u64 {

        let state = commitment::open(expected, opening);
        return state.counter;
}
"#;
    let metadata = crate::compile_metadata(source, crate::CURRENT_EDITION, None).expect("committed-state builder fixture");
    let action = &metadata.actions[0];
    let manifest = typescript_builder_manifest("@test/committed", &metadata, &[action], "test-metadata", None, None).unwrap();
    let params = manifest["actions"][0]["params"].as_array().unwrap();
    assert_eq!(params[0]["ty"], "Commitment<State>");
    assert_eq!(params[0]["fixed_byte_len"], 32);
    assert_eq!(params[0]["schema_pointer_abi"], false);
    assert_eq!(params[1]["ty"], "Opening<State>");
    assert!(params[1]["fixed_byte_len"].is_null());
    assert_eq!(params[1]["schema_pointer_abi"], true);
    assert_eq!(manifest["actions"][0]["entry_witness_required"], true);

    let generated = typescript_builder_index("@test/committed", &metadata, &[action], "test-metadata", None, None).unwrap();
    assert!(generated.contains("schema_pointer_abi"));
    assert!(generated.contains("Opening<State>"));
    assert!(generated.contains("entryWitnessRequired"));
}

#[cfg(feature = "vm-runner")]
#[test]
fn run_outcome_uses_the_resolved_entry_contract() {
    let metadata = crate::compile_metadata(
        r#"
module selected_entry;
lock decoy() -> bool {

        false
}
action selected(witness value: u64) -> u64 {

        return value;
}
"#,
        crate::CURRENT_EDITION,
        None,
    )
    .unwrap();
    let entry = run_entry_outcome(&metadata).expect("one resolved action entry");
    assert_eq!(entry.kind, "action");
    assert_eq!(entry.name, "selected");
}

#[test]
fn test_command_execution() {
    let _cmd = Command::Clean(CleanArgs::default());
}

#[test]
fn publish_parses_continuous_browser_authorisation() {
    let matches = CliParser::command().try_get_matches_from(["cellc", "publish", "--authorise", "--no-open"]).unwrap();
    let Command::Publish(args) = CliParser::parse_matches(matches).unwrap() else {
        panic!("expected publish command");
    };
    assert!(args.authorise);
    assert!(args.no_open);
    assert!(args.capability_key_id.is_none());
}

#[test]
fn publish_authorisation_rejects_an_existing_key_override() {
    let result = CliParser::command().try_get_matches_from([
        "cellc",
        "publish",
        "--authorise",
        "--capability-key-id",
        "cap_0123456789abcdef0123456789abcdef",
    ]);
    assert!(result.is_err());
}

#[test]
fn browser_authorisation_activates_only_the_server_confirmed_key() {
    for status in ["pending", "cancelled", "expired"] {
        let persisted = std::cell::Cell::new(false);
        activate_registry_key_after_wallet_approval(status, None, "cap_local", || {
            persisted.set(true);
            Ok(())
        })
        .unwrap();
        assert!(!persisted.get(), "{status} must not activate a publishing key");
    }

    for status in ["authorised", "review_pending"] {
        let persisted = std::cell::Cell::new(false);
        activate_registry_key_after_wallet_approval(status, Some("cap_local"), "cap_local", || {
            persisted.set(true);
            Ok(())
        })
        .unwrap();
        assert!(persisted.get(), "{status} must preserve the approved publishing key");
    }

    let persisted = std::cell::Cell::new(false);
    let mismatch = activate_registry_key_after_wallet_approval("authorised", Some("cap_remote"), "cap_local", || {
        persisted.set(true);
        Ok(())
    });
    assert!(mismatch.is_err());
    assert!(!persisted.get(), "a mismatched server key must not be persisted");

    for status in ["authorised", "review_pending"] {
        let activated = std::cell::Cell::new(false);
        let missing = activate_registry_key_after_wallet_approval(status, None, "cap_local", || {
            activated.set(true);
            Ok(())
        });
        assert!(missing.is_err(), "{status} must require a returned key id");
        assert!(!activated.get(), "{status} must not activate a key without its id");
    }
}

#[test]
fn browser_authorisation_removes_pending_keys_only_for_explicit_terminal_failure() {
    for status in ["pending", "authorised", "review_pending", "unreachable", "deadline_elapsed"] {
        assert!(!registry_authorisation_status_removes_pending_key(status), "{status} must preserve the pending key");
    }
    for status in ["cancelled", "expired"] {
        assert!(registry_authorisation_status_removes_pending_key(status), "{status} must remove the pending key");
    }
}

#[test]
fn registry_keychain_state_distinguishes_pending_and_active_keys() {
    let pending = RegistryKeychainSecret {
        schema: REGISTRY_KEYCHAIN_SECRET_SCHEMA.to_string(),
        status: "pending".to_string(),
        pkcs8_b64: "cGVuZGluZw==".to_string(),
        session_id: Some("auth_0123456789abcdef0123456789abcdef".to_string()),
        expires_at: Some("2026-08-05T12:00:00.000Z".to_string()),
        pending_expires_at_unix_seconds: Some(1_786_000_000),
    };
    let encoded = serde_json::to_string(&pending).unwrap();
    let decoded: RegistryKeychainSecret = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.schema, REGISTRY_KEYCHAIN_SECRET_SCHEMA);
    assert_eq!(decoded.status, "pending");
    assert_eq!(decoded.session_id.as_deref(), Some("auth_0123456789abcdef0123456789abcdef"));

    let active = RegistryKeychainSecret {
        schema: REGISTRY_KEYCHAIN_SECRET_SCHEMA.to_string(),
        status: "active".to_string(),
        pkcs8_b64: pending.pkcs8_b64,
        session_id: None,
        expires_at: None,
        pending_expires_at_unix_seconds: None,
    };
    assert_eq!(serde_json::from_str::<RegistryKeychainSecret>(&serde_json::to_string(&active).unwrap()).unwrap().status, "active");
}

#[test]
fn record_deployment_parses_explicit_testnet_network() {
    let code_hash = format!("0x{}", "11".repeat(32));
    let tx_hash = format!("0x{}", "22".repeat(32));
    let matches = CliParser::command()
        .try_get_matches_from([
            "cellc",
            "artifact",
            "record-deployment",
            "acme/demo@1.0.0",
            "--network",
            "testnet",
            "--code-hash",
            &code_hash,
            "--hash-type",
            "data1",
            "--dep-type",
            "code",
            "--tx-hash",
            &tx_hash,
            "--index",
            "0",
            "--capability-key-id",
            "cap_test",
        ])
        .unwrap();
    let Command::Artifact(ArtifactArgs { operation: ArtifactOperation::RecordDeployment { network, .. } }) =
        CliParser::parse_matches(matches).unwrap()
    else {
        panic!("expected artifact record-deployment command");
    };
    assert_eq!(network, "testnet");
}

#[test]
fn registry_api_urls_require_https_except_for_loopback() {
    assert_eq!(registry_origin_from_api_base("https://registry.example/api").unwrap(), "https://registry.example");
    assert!(resolve_registry_api_base(Some("http://registry.example".to_string())).is_err());
    assert!(resolve_registry_api_base(Some("http://127.0.0.1:8232".to_string())).is_ok());
    assert!(resolve_registry_api_base(Some("http://[::1]:8232".to_string())).is_ok());
    assert!(resolve_registry_api_base(Some("https://user:secret@registry.example".to_string())).is_err());
    assert!(resolve_registry_api_base(Some("https://registry.example?next=http://internal".to_string())).is_err());
}

#[test]
fn registry_http_client_does_not_follow_redirects() {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        stream
                .write_all(
                    b"HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:9/internal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
    });

    let response = registry_http_client()
        .unwrap()
        .post(format!("http://{address}/publish"))
        .body("signed metadata")
        .send()
        .expect("redirect response should be returned without following Location");
    assert_eq!(response.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
    server.join().unwrap();
}

#[test]
fn visible_021_group_arguments_have_help_text() {
    let command = CliParser::command();
    let mut missing = Vec::new();
    for group in ["explain", "auth", "registry", "publish", "install", "tx", "deploy"] {
        let Some(subcommand) = command.get_subcommands().find(|subcommand| subcommand.get_name() == group) else {
            missing.push(format!("cellc {group}: missing command"));
            continue;
        };
        collect_missing_visible_arg_help(&mut missing, &format!("cellc {group}"), subcommand);
    }

    assert!(missing.is_empty(), "visible 0.21 CLI arguments missing help text:\n{}", missing.join("\n"));
}

fn collect_missing_visible_arg_help(missing: &mut Vec<String>, path: &str, command: &clap::Command) {
    if command.is_hide_set() {
        return;
    }
    for arg in command.get_arguments().filter(|arg| !arg.is_hide_set()) {
        let help = arg.get_help().map(|help| help.to_string()).unwrap_or_default();
        if help.trim().is_empty() {
            missing.push(format!("{path} {}", arg.get_id()));
        }
    }
    for subcommand in command.get_subcommands() {
        collect_missing_visible_arg_help(missing, &format!("{path} {}", subcommand.get_name()), subcommand);
    }
}
