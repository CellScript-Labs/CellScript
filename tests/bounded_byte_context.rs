//! Exact AgoraSeal 52-byte encoding, with the pinned real context child.
#![cfg(not(feature = "wasm"))]

use cellscript::{CompileEntryScope, CompileOptions, CompileResult, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::{
    ckb_hash::blake2b_256,
    ckb_script::ScriptGroupType,
    ckb_types::{
        bytes::Bytes,
        core::{EpochNumberWithFraction, HeaderBuilder},
        packed,
        prelude::*,
    },
};
use sha2::{Digest, Sha256};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;
#[path = "support/cost_measurement.rs"]
#[allow(dead_code)]
mod cost_measurement;
#[path = "support/cost_provenance.rs"]
#[allow(dead_code)]
mod cost_provenance;
#[path = "support/cost_trace.rs"]
#[allow(dead_code)]
mod cost_trace;

fn encode(ballot: &[u8], first: u64, second: u64) -> Option<[u8; 52]> {
    if ballot.len() != 53 {
        return None;
    }
    let mut result = [0; 52];
    result[..36].copy_from_slice(&ballot[8..44]);
    result[36..44].copy_from_slice(&first.to_le_bytes());
    result[44..].copy_from_slice(&second.to_le_bytes());
    Some(result)
}

fn child() -> Bytes {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/byte-context-child");
    let manifest: serde_json::Value = serde_json::from_str(include_str!("fixtures/byte-context-child/build-manifest.json")).unwrap();
    for (file, expected) in manifest["source_sha256"].as_object().unwrap() {
        assert_eq!(hex::encode(Sha256::digest(std::fs::read(root.join(file)).unwrap())), expected.as_str().unwrap());
    }
    let elf = hex::decode(include_str!("fixtures/byte-context-child/child.hex").trim()).unwrap();
    assert_eq!(elf.len() as u64, manifest["artifact_bytes"].as_u64().unwrap());
    assert_eq!(hex::encode(Sha256::digest(&elf)), manifest["artifact_sha256"].as_str().unwrap());
    if let Some(path) = std::env::var_os("CELLSCRIPT_BYTE_CONTEXT_CHILD_ELF") {
        assert_eq!(std::fs::read(path).unwrap(), elf, "fresh child build must match the pinned fixture");
    }
    Bytes::from(elf)
}

fn parent(hash: [u8; 32], looped: bool, delegated: bool, mutation: &str, opt_level: u8) -> CompileResult {
    let mut encoding = String::new();
    if looped {
        let range = if mutation == "out_of_range" { "18..54" } else { "8..44" };
        encoding.push_str(&format!("for i in {range} {{ context.push(ckb::cell_data_u8(ballot, i) as u8) }}\n"));
        for name in ["deposit_height", "proposal_height"] {
            encoding.push_str(&format!("for i in 0..8 {{ context.push((({name} >> (i * 8)) & 255) as u8) }}\n"));
        }
    } else {
        for offset in 8..44 {
            encoding.push_str(&format!("context.push(ckb::cell_data_u8(ballot, {offset}) as u8)\n"));
        }
        for name in ["deposit_height", "proposal_height"] {
            for shift in (0..64).step_by(8) {
                encoding.push_str(&format!("context.push((({name} >> {shift}) & 255) as u8)\n"));
            }
        }
    }
    if mutation == "overflow" {
        encoding.push_str("for i in 0..205 { context.push(0 as u8) }\n");
    }
    let first_length = if mutation == "bad_length" { 35 } else { 36 };
    let dependency_index = if mutation == "bad_dependency" { 0 } else { 2 };
    let hash_literal = hash.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
    let invocation = if delegated {
        format!("ckb::trusted_spawn_wait_cell_dep_hex4({dependency_index}, Hash::from_bytes(b\"{hash_literal}\"), context, {first_length}, 8, 8, 0)")
    } else {
        String::new()
    };
    let source = format!(
        r#"module acceptance::bounded_byte_context
action verify(witness deposit_height: u64, witness proposal_height: u64, witness oracle: [u8; 52]) -> u64 {{
    verification
    let ballot = source::group_output(0)
    require ckb::cell_data_size(ballot) == 53
    let mut context: Vec<u8> = Vec::with_capacity(52)
    {encoding}
    require context.len() == 52
    for i in 0..52 {{ require context[i] == oracle[i] }}
    {invocation}
    return 0
}}
"#
    );
    let directory = tempfile::tempdir().unwrap();
    let manager = cellscript::package::PackageManager::new(directory.path());
    manager.init("bounded_byte_context").unwrap();
    let mut manifest = manager.read_manifest().unwrap();
    if delegated {
        manifest.deploy.ckb = Some(cellscript::package::CkbDeployConfig {
            trusted_external_verifiers: vec![cellscript::package::CkbTrustedExternalVerifierConfig {
                schema: "cellscript-trusted-external-verifier-v1".into(),
                name: "agoraseal-fixed-context".into(),
                scope: "action:verify".into(),
                operation: "spawn-wait".into(),
                adapter: "hex4-v1".into(),
                code_hash: hex::encode(hash),
                hash_type: "data".into(),
                source_identity: "AgoraSeal 5b6106a7b86d51b736a7df772ded548fba4c9a4e verifiers/ckb-context".into(),
                applicability: "fixed 52-byte encoding acceptance fixture".into(),
                trust_basis: "exact child ELF hash and real CKB-VM header/CellDep checks".into(),
                guarantees: vec!["validates the direct CellDep prefix outpoint and both committed header heights".into()],
            }],
            ..Default::default()
        });
    }
    manager.write_manifest(&manifest).unwrap();
    std::fs::write(directory.path().join("src/main.cell"), source).unwrap();
    let root = camino::Utf8Path::from_path(directory.path()).unwrap();
    let result = cellscript::compile_path_with_executable_surface_policy(
        root,
        CompileOptions { opt_level, target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
        Some(CompileEntryScope::Action("verify".into())),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    result.validate().unwrap();
    result
}

fn execute(
    compiled: &CompileResult,
    child: &Bytes,
    heights: [u64; 2],
    mutation: &str,
) -> (ckb_script_runner::CkbScriptExecutionResult, cost_trace::ExecutionTrace) {
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::from(vec![0; 53]), 1, 1);
    fixture.cell_deps = [Bytes::from(vec![0; 8]), Bytes::from(vec![0; 188]), child.clone()]
        .into_iter()
        .map(|data| ckb_script_runner::FixtureCell { capacity: 100_000_000_000, type_script: None, data })
        .collect();
    fixture.header_dao_fields = vec![[0; 32]; 2];
    fixture.header_contexts = heights
        .into_iter()
        .map(|number| ckb_script_runner::FixtureHeaderContext {
            number,
            timestamp: 0,
            epoch_number: 0,
            epoch_index: 0,
            epoch_length: 1,
        })
        .collect();
    let original_dependency = std::cell::RefCell::new(None);
    ckb_script_runner::execute_cellscript_script_with_context_setup_and_observer(
        cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
        &fixture,
        |tx, _| {
            *original_dependency.borrow_mut() = Some(tx.cell_deps().get(0).unwrap().out_point());
            let selected = packed::OutPoint::from_slice(&(8..44).collect::<Vec<u8>>()).unwrap();
            let mut dependencies = tx.cell_deps().into_iter().collect::<Vec<_>>();
            dependencies[0] = dependencies[0].clone().as_builder().out_point(selected.clone()).build();
            let mut ballot = (0..53).map(|index| index as u8).collect::<Vec<_>>();
            ballot[8..44].copy_from_slice(selected.as_slice());
            if mutation == "wrong_outpoint" {
                ballot[8] ^= 1;
            }
            let claims = if mutation == "false_height" { [heights[0] ^ 1, heights[1]] } else { heights };
            let mut oracle = encode(&ballot, claims[0], claims[1]).unwrap();
            if mutation == "wrong_oracle" {
                oracle[51] ^= 1;
            }
            let payload = compiled.metadata.actions[0]
                .entry_witness_args(&[
                    EntryWitnessArg::U64(claims[0]),
                    EntryWitnessArg::U64(claims[1]),
                    EntryWitnessArg::Bytes(oracle.to_vec()),
                ])
                .unwrap();
            if mutation == "truncated" {
                ballot.truncate(52);
            }
            let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
            let mut builder = tx
                .as_advanced_builder()
                .set_cell_deps(dependencies)
                .set_outputs_data(vec![Bytes::from(ballot).pack()])
                .set_witnesses(vec![witness.pack()]);
            if mutation == "missing_header" {
                builder = builder.set_header_deps(vec![tx.header_deps().get(1).unwrap()]);
            }
            builder.build()
        },
        |context, tx, _| {
            let (output, data) = context.get_cell(original_dependency.borrow().as_ref().unwrap()).unwrap();
            context.create_cell_with_out_point(tx.cell_deps().get(0).unwrap().out_point(), output, data);
            for (index, number) in heights.into_iter().enumerate() {
                let header =
                    HeaderBuilder::default().number(number).dao([0u8; 32].pack()).epoch(EpochNumberWithFraction::new(0, 0, 1)).build();
                context.link_cell_with_block(tx.cell_deps().get(index).unwrap().out_point(), header.hash(), index);
            }
        },
        |context, tx, script| cost_trace::measure(context, tx, script, ScriptGroupType::Type, 100_000_000),
    )
}

#[test]
fn fixed_context_matches_rust_bytes_and_the_real_parent_child_vm() {
    let child = child();
    let hash = blake2b_256(&child);
    let mut measurements = Vec::new();
    for opt_level in 0..=3 {
        for looped in [false, true] {
            for delegated in [false, true] {
                let compiled = parent(hash, looped, delegated, "", opt_level);
                for heights in [[0, 0], [u64::MAX, u64::MAX], [0x333231302f2e2d2c, 0x3b3a393837363534]] {
                    let (execution, trace) = execute(&compiled, &child, heights, "");
                    assert_eq!(execution.exit_code, 0, "looped={looped}, delegated={delegated}: {:?}", execution.captured_debug);
                    assert_eq!(trace.status, "measured", "{trace:?}");
                    assert_eq!(trace.vms.len(), if delegated { 2 } else { 1 });
                    let budgets: serde_json::Value =
                        serde_json::from_str(include_str!("fixtures/byte-context-child/budgets.json")).unwrap();
                    let ceiling = budgets["ceilings"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|ceiling| {
                            ceiling["opt_level"] == opt_level && ceiling["looped"] == looped && ceiling["delegated"] == delegated
                        })
                        .unwrap();
                    assert!(
                        cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes).len() as u64
                            <= ceiling["elf_bytes"].as_u64().unwrap()
                    );
                    assert!(trace.authoritative_cycles.unwrap() <= ceiling["group_cycles"].as_u64().unwrap());
                    for vm in &trace.vms {
                        let metric = if vm.vm_id == 0 { "parent_stack_bytes" } else { "child_stack_bytes" };
                        assert!(vm.observed_stack_bytes <= ceiling[metric].as_u64().unwrap());
                    }
                    measurements.push(serde_json::json!({"opt_level":opt_level,"looped":looped,"delegated":delegated,"heights_hex":heights.map(|height| format!("{height:016x}")),
                    "lowering_record_hash":compiled.metadata.verified_artifact.lowering_record_hash,
                    "source_map_hash":compiled.metadata.verified_artifact.source_map_hash,
                    "verified_bundle_id":compiled.metadata.verified_artifact.verified_bundle_id,
                    "elf_bytes":cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes).len(),
                    "elf_sha256":hex::encode(Sha256::digest(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes))),
                    "transaction_cycles":execution.cycles,"transaction_hash":execution.serialized_transaction_hash,"trace":trace}));
                }
            }
        }
    }
    let report = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/byte-context/measurements.json");
    std::fs::create_dir_all(report.parent().unwrap()).unwrap();
    let mut configuration = cost_provenance::vm_configuration();
    configuration["group_cycle_limit"] = 100_000_000u64.into();
    configuration["transaction_cycle_limit"] = 10_000_000u64.into();
    configuration["stack_scope"] = "observed entry-SP-relative peak separately for each VM; parity-gated diagnostic replay".into();
    std::fs::write(
        report,
        serde_json::to_vec_pretty(&serde_json::json!({"schema":"cellscript-bounded-byte-context-measurement-v1",
        "source":cost_provenance::capture_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))),
        "vm_configuration":configuration,
        "claim":"matched fixed encoder comparison; not full AgoraSeal Rust/CellScript architecture equivalence",
        "measurements":measurements}))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn agoraseal_vote_template_and_generated_source_share_the_checked_encoding() {
    let code_hash = hex::encode(blake2b_256(child()));
    let literal = code_hash.as_bytes().chunks(2).map(|pair| format!("\\x{}", std::str::from_utf8(pair).unwrap())).collect::<String>();
    let root = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/bounded-byte-context/agoraseal-vote");
    assert_eq!(
        std::fs::read_to_string(root.join("src/main.cell.in")).unwrap().replace("@CONTEXT_HASH_BYTES@", &literal),
        std::fs::read_to_string(root.join("src/main.cell")).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(root.join("Cell.toml.in")).unwrap().replace("@CONTEXT_HASH@", &code_hash),
        std::fs::read_to_string(root.join("Cell.toml")).unwrap()
    );
    for opt_level in 0..=3 {
        let compiled = cellscript::compile_path_with_executable_surface_policy(
            &root,
            CompileOptions { opt_level, target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
            Some(CompileEntryScope::Action("cast".into())),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        compiled.validate().unwrap();
    }
}

#[test]
fn context_and_verifier_negatives_fail_closed() {
    let child = child();
    let hash = blake2b_256(&child);
    for opt_level in 0..=3 {
        let ordinary = parent(hash, true, true, "", opt_level);
        for mutation in ["truncated", "wrong_oracle", "wrong_outpoint", "false_height", "missing_header"] {
            let (execution, trace) = execute(&ordinary, &child, [17, 91], mutation);
            assert_ne!(execution.exit_code, 0, "{mutation}");
            assert_eq!(trace.status, "measured", "{mutation}: {trace:?}");
            assert_ne!(trace.exit_code, Some(0));
        }
        for mutation in ["out_of_range", "overflow", "bad_length", "bad_dependency"] {
            let compiled = parent(hash, true, true, mutation, opt_level);
            let (execution, trace) = execute(&compiled, &child, [17, 91], "");
            assert_ne!(execution.exit_code, 0, "{mutation}");
            assert_eq!(trace.status, "measured", "{mutation}: {trace:?}");
        }
        let wrong_hash = parent([0x42; 32], true, true, "", opt_level);
        assert_ne!(execute(&wrong_hash, &child, [17, 91], "").0.exit_code, 0);
    }
}

#[test]
fn rust_oracle_keeps_every_byte_and_rejects_truncated_ballots() {
    let ballot = (0..53).collect::<Vec<u8>>();
    let context = encode(&ballot, 0x333231302f2e2d2c, 0x3b3a393837363534).unwrap();
    assert_eq!(context, (8..60).collect::<Vec<u8>>().as_slice());
    assert!(encode(&ballot[..52], 0, 0).is_none());
    assert!(encode(&[0; 54], 0, 0).is_none());
}
