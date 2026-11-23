use super::super::output::CommandOutcome;
use super::entry_witness_values::decode_hex_arg;
use super::fixture_verification::ckb_fixture_manifest_report;
use super::{CkbHashArgs, CkbStdCompatArgs, CommandExecutor, VerifyCkbFixturesArgs};
use crate::error::Result;
use crate::{ENTRY_WITNESS_ABI, ENTRY_WITNESS_PLACEMENT_ABI, ENTRY_WITNESS_PLACEMENT_FIELD, ENTRY_WITNESS_PLACEMENT_SOURCE};
use std::path::Path;

impl CommandExecutor {
    pub(super) fn ckb_hash(args: CkbHashArgs) -> Result<()> {
        let source_count = usize::from(args.input.is_some()) + usize::from(args.hex.is_some()) + usize::from(args.file.is_some());
        if source_count > 1 {
            return Err(crate::error::CompileError::without_span(
                "ckb-hash accepts at most one input source: positional UTF-8 text, --hex, or --file",
            ));
        }
        let bytes = if let Some(hex) = args.hex.as_deref() {
            decode_hex_arg("ckb-hash", hex, None)?
        } else if let Some(path) = args.file.as_ref() {
            std::fs::read(path).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to read CKB hash input '{}': {}", path.display(), error))
            })?
        } else {
            args.input.unwrap_or_default().into_bytes()
        };
        let hash = crate::ckb_blake2b256(&bytes);
        let hash_hex = crate::hex_encode(&hash);
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "algorithm": "blake2b-256",
                "personalization": std::str::from_utf8(crate::CKB_DEFAULT_HASH_PERSONALIZATION).unwrap_or("ckb-default-hash"),
                "input_bytes": bytes.len(),
                "hash": hash_hex,
            }),
            human_lines: vec![hash_hex],
        }
        .emit(args.json)
    }

    pub(super) fn ckb_std_compat(args: CkbStdCompatArgs) -> Result<()> {
        let report = serde_json::json!({
            "status": "ok",
            "schema": "cellscript-ckb-std-compat-report-v0.19",
            "runtime_policy": "inline",
            "compiler_core_dependency": "no-ckb-std",
            "compatibility_dependency_scope": "dev-test-and-adapter-contract",
            "abi_source": "src/ckb_abi.rs",
            "test_evidence": {
                "compat_tests": "tests/ckb_std_compat.rs",
                "constant_parity": true,
                "source_view_decoding": true,
                "witness_args_layout": true,
                "type_id_contract": true,
                "since_epoch_contract": true,
                "occupied_capacity_field": true,
                "packed_transaction_materialization": true,
                "script_construction_api": true,
            },
            "ckb_std_refs": {
                "constants": "ckb_std::ckb_constants",
                "witness_args": "ckb_types::packed::WitnessArgs",
                "type_id": "ckb_std::type_id",
                "since": "ckb_std::since",
                "occupied_capacity": "ckb_std::high_level::load_cell_occupied_capacity",
            },
            "inline_abi": {
                "syscalls": {
                    "load_cell_by_field": crate::ckb_abi::syscall::LOAD_CELL_BY_FIELD,
                    "load_witness": crate::ckb_abi::syscall::LOAD_WITNESS,
                    "load_input_by_field": crate::ckb_abi::syscall::LOAD_INPUT_BY_FIELD,
                    "spawn": crate::ckb_abi::syscall::SPAWN,
                },
                "sources": {
                    "input": crate::ckb_abi::source::INPUT,
                    "output": crate::ckb_abi::source::OUTPUT,
                    "group_input": crate::ckb_abi::source::GROUP_INPUT,
                    "group_output": crate::ckb_abi::source::GROUP_OUTPUT,
                },
                "fields": {
                    "cell_occupied_capacity": crate::ckb_abi::cell_field::OCCUPIED_CAPACITY,
                    "input_since": crate::ckb_abi::input_field::SINCE,
                },
            },
            "adapter_boundary": {
                "transaction_realizer": "ckb-sdk-rust-or-CCC-adapter",
                "compiler_core_uses_ckb_sdk_rust": false,
                "action_build_contract": "cellscript-ckb-adapter-contract-v0.19",
                "requires_node_acceptance_for_production": true,
                "script_construction": {
                    "owner": "adapter",
                    "packed_type": "ckb_types::packed::Script",
                    "evidence_schema": "cellscript-ckb-script-evidence-v0.19",
                    "supports": [
                        "arbitrary_code_hash",
                        "hash_type",
                        "args",
                        "script_hash",
                        "script_ref_readback",
                        "explicit_cell_dep_binding",
                        "args_exact_prefix_suffix",
                        "owner_mode_args"
                    ],
                },
            },
            "witness_args_policy": {
                "entry_payload_abi": ENTRY_WITNESS_ABI,
                "placement_abi": ENTRY_WITNESS_PLACEMENT_ABI,
                "entry_payload_owner": "compiler",
                "final_witness_args_owner": "adapter",
                "default_action_payload_field": ENTRY_WITNESS_PLACEMENT_FIELD,
                "runtime_source": ENTRY_WITNESS_PLACEMENT_SOURCE,
                "raw_v1_compatible": false,
                "lock_signature_policy": "explicit-adapter-owned-do-not-overwrite",
                "placement_requires_deployment_role": true,
                "ckb_reference": "ckb_types::packed::WitnessArgs",
            },
            "non_goals": [
                "does-not-execute-ckb-vm",
                "does-not-query-live-cells",
                "does-not-resolve-celldeps",
                "does-not-sign-or-submit"
            ],
        });

        CommandOutcome {
            human_lines: vec![
                format!("CKB std compatibility: {}", report["status"].as_str().unwrap_or("unknown")),
                format!("  Schema: {}", report["schema"].as_str().unwrap_or("unknown")),
                format!("  Runtime policy: {}", report["runtime_policy"].as_str().unwrap_or("unknown")),
                format!("  ABI source: {}", report["abi_source"].as_str().unwrap_or("unknown")),
                format!("  Test evidence: {}", report["test_evidence"]["compat_tests"].as_str().unwrap_or("unknown")),
            ],
            machine: report,
        }
        .emit(args.json)
    }

    pub(super) fn verify_ckb_fixtures(args: VerifyCkbFixturesArgs) -> Result<()> {
        let manifest_bytes = std::fs::read(&args.manifest).map_err(|error| {
            crate::error::CompileError::without_span(format!(
                "failed to read fixture manifest '{}': {}",
                args.manifest.display(),
                error
            ))
        })?;
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).map_err(|error| {
            crate::error::CompileError::without_span(format!(
                "failed to parse fixture manifest '{}': {}",
                args.manifest.display(),
                error
            ))
        })?;
        let base_dir = args.manifest.parent().unwrap_or_else(|| Path::new("."));
        let report = ckb_fixture_manifest_report(&manifest, base_dir, &manifest_bytes);
        let issue_count = report["issue_count"].as_u64().unwrap_or(0);
        let mut human_lines = vec![
            format!("CKB fixture manifest verification: {}", report["status"].as_str().unwrap_or("unknown")),
            format!("  Manifest schema: {}", report["manifest_schema"].as_str().unwrap_or("unknown")),
            format!("  Execution level: {}", report["execution_level"].as_str().unwrap_or("unknown")),
            format!("  Suites: {}", report["suite_count"].as_u64().unwrap_or(0)),
            format!("  Fixtures: {}", report["fixture_count"].as_u64().unwrap_or(0)),
            format!("  Issues: {issue_count}"),
        ];
        if let Some(issues) = report["issues"].as_array() {
            human_lines.extend(issues.iter().map(|issue| format!("  - {}", issue.as_str().unwrap_or("<invalid issue>"))));
        }
        let outcome = CommandOutcome { machine: report, human_lines };
        if issue_count == 0 {
            outcome.emit(args.json)
        } else {
            Err(outcome.failure(format!("CKB fixture manifest failed verification: {issue_count} issue(s)")))
        }
    }
}
