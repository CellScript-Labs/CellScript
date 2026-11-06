//! Opt-in financial-operation measurements; existing compiler baselines stay
//! unchanged. Inputs are strict archived transactions, never reconstructed DSL.
#![cfg(all(feature = "cli", not(feature = "wasm")))]
#[allow(dead_code)]
#[path = "support/cost_measurement.rs"]
mod cost_measurement;
#[allow(dead_code)]
#[path = "support/cost_trace.rs"]
mod cost_trace;
#[path = "support/finance_archive.rs"]
#[allow(dead_code)]
mod finance_archive;

use ckb_testtool::{
    ckb_chain_spec::consensus::ConsensusBuilder,
    ckb_script::{ScriptGroupType, TransactionScriptsVerifier, TxVerifyEnv},
    ckb_types::{
        core::{
            hardfork::{HardForks, CKB2021, CKB2023},
            Capacity, HeaderBuilder,
        },
        prelude::*,
    },
};
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc, time::Instant};

const MAX_CYCLES: u64 = 100_000_000;

fn selected(namespace: &str, name: &str) -> bool {
    match namespace {
        "pool" => matches!(
            name,
            "initialise-authentic-pool-and-lp"
                | "second-provider-proportional-deposit-and-refunds"
                | "swap-a-for-b"
                | "swap-b-for-a"
                | "withdraw-222"
                | "existing-lp-input-cannot-hide-deposit-supply-increase"
        ),
        "c1-layout" => name.starts_with("c1-"),
        "pool-admission" | "pool-math-direct" | "pool-math-relation" => {
            !name.starts_with("issue-")
                && !name.contains("bootstrap")
                && name != "pool-creation-rejects-a-different-real-funding-anchor"
        }
        "c1" => {
            name.starts_with("c1-")
                || matches!(name, "direct-order-pool-settlement-with-no-user-b-input" | "authorizer-unwraps-all-residual-intent-funds")
        }
        "c3" => name.starts_with("pure-two-pool-") || name.starts_with("route-"),
        "c3-mode-boundaries" => matches!(
            name,
            "exact-input-route-rejects-two-valid-exact-output-legs-donating-quote-surplus"
                | "exact-output-route-rejects-valid-exact-input-plateau-overpayment"
        ),
        // The order/allocation pilots include every economically relevant
        // operation; issuance and bootstrap remain in their replay archives.
        "orders" | "c2" => {
            !name.starts_with("issue-") && !name.contains("bootstrap") && !name.starts_with("fund-") && !name.starts_with("create-")
        }
        "pool-boundaries" => name != "create-pool-bootstrap" && name != "pool-creation-rejects-a-different-real-funding-anchor",
        _ => false,
    }
}

#[test]
#[ignore = "requires freshly generated v2 pool/order/C1/C2/C3/boundary archives; explicit financial pilot"]
fn full_financial_operation_resource_pilot() {
    measure(false);
}

#[test]
#[ignore = "requires fresh v2 archives and explicitly frozen financial VM case ceilings"]
fn full_financial_operation_vm_resource_acceptance() {
    measure(true);
}

fn verify_limits(report: &Value, limits: &Value, extra: Option<&Value>) {
    assert_eq!(limits["schema"], "cellscript-finance-operation-vm-limits-v1");
    if let Some(extra) = extra {
        assert_eq!(extra["schema"], "cellscript-finance-operation-vm-extra-limits-v1");
    }
    for namespace in report["requested_namespaces"].as_array().unwrap() {
        let namespace = namespace.as_str().unwrap();
        let original = &limits["namespaces"][namespace];
        let extension = extra.map(|limits| &limits["namespaces"][namespace]);
        let mut original_counts = std::collections::BTreeMap::<String, u64>::new();
        let mut extension_counts = std::collections::BTreeMap::<String, u64>::new();
        for case in report["cases"].as_array().unwrap().iter().filter(|case| case["namespace"] == namespace) {
            let name = case["case"].as_str().unwrap();
            let bound = if original["case_occurrences"].get(name).is_some() {
                assert!(
                    extension.is_none_or(|extra| extra["case_occurrences"].get(name).is_none()),
                    "extension may not replace an original case ceiling"
                );
                *original_counts.entry(name.into()).or_default() += 1;
                original
            } else {
                let extension = extension.expect("new case requires an explicitly frozen additive ceiling");
                assert!(extension["case_occurrences"].get(name).is_some(), "unfrozen case: {namespace}/{name}");
                *extension_counts.entry(name.into()).or_default() += 1;
                extension
            };
            assert!(case["guest_vm_generation_count"].as_u64().unwrap() > 0, "selected case needs actual guest stack observations");
            for (metric, ceiling) in [
                ("all_groups_separate_replay_cycles", "maximum_all_group_cycles"),
                ("maximum_observed_stack_per_vm_bytes", "maximum_observed_stack_bytes"),
                ("transaction_bytes", "maximum_transaction_bytes"),
                ("witness_bytes", "maximum_witness_bytes"),
                ("occupied_output_capacity", "maximum_occupied_output_capacity"),
                ("fee", "maximum_fee"),
            ] {
                let actual = case[metric].as_u64().unwrap_or_else(|| panic!("{namespace}/{name}: {metric} unavailable"));
                assert!(
                    actual <= bound[ceiling].as_u64().unwrap(),
                    "{namespace}/{name}: {metric}={actual} exceeds {ceiling}={}",
                    bound[ceiling]
                );
            }
            for group in case["groups"].as_array().unwrap() {
                if let Some(bytes) = group["executable_bytes"].as_u64() {
                    assert!(bytes <= bound["maximum_executable_bytes"].as_u64().unwrap());
                }
            }
        }
        for (counts, bound) in [(original_counts, Some(original)), (extension_counts, extension)] {
            let frozen = bound.and_then(|bound| bound["case_occurrences"].as_object()).cloned().unwrap_or_default();
            assert_eq!(serde_json::to_value(counts).unwrap(), Value::Object(frozen), "{namespace}: frozen case coverage changed");
            if let Some(bound) = bound.filter(|bound| !bound.is_null()) {
                for program in report["programs"].as_array().unwrap().iter().filter(|program| program["namespace"] == namespace) {
                    assert!(program["elf_bytes"].as_u64().unwrap() <= bound["maximum_executable_bytes"].as_u64().unwrap());
                    assert!(
                        program["maximum_declared_frame_bytes"].as_u64().unwrap()
                            <= bound["maximum_declared_frame_bytes"].as_u64().unwrap()
                    );
                }
            }
        }
    }
}

#[test]
fn additive_vm_budgets_preserve_original_ceilings_and_exact_case_coverage() {
    let bound = |name: &str, maximum: u64| {
        json!({"case_occurrences":{name:1},
        "maximum_all_group_cycles":maximum,"maximum_observed_stack_bytes":maximum,
        "maximum_transaction_bytes":maximum,"maximum_witness_bytes":maximum,
        "maximum_occupied_output_capacity":maximum,"maximum_fee":maximum,
        "maximum_executable_bytes":maximum,"maximum_declared_frame_bytes":maximum})
    };
    let case = |name: &str, actual: u64| {
        json!({"namespace":"orders","case":name,"guest_vm_generation_count":1,
        "all_groups_separate_replay_cycles":actual,"maximum_observed_stack_per_vm_bytes":actual,
        "transaction_bytes":actual,"witness_bytes":actual,"occupied_output_capacity":actual,"fee":actual,"groups":[]})
    };
    let limits = json!({"schema":"cellscript-finance-operation-vm-limits-v1","namespaces":{"orders":bound("original",100)}});
    let extra = json!({"schema":"cellscript-finance-operation-vm-extra-limits-v1","namespaces":{"orders":bound("additional",300)}});
    let report = json!({"requested_namespaces":["orders"],"cases":[case("original",99),case("additional",200)],"programs":[]});
    verify_limits(&report, &limits, Some(&extra));
    let mut exceeded = report.clone();
    exceeded["cases"][0] = case("original", 101);
    assert!(
        std::panic::catch_unwind(|| verify_limits(&exceeded, &limits, Some(&extra))).is_err(),
        "new headroom cannot widen an old case"
    );
    let mut override_old = extra.clone();
    override_old["namespaces"]["orders"]["case_occurrences"]["original"] = json!(1);
    assert!(
        std::panic::catch_unwind(|| verify_limits(&report, &limits, Some(&override_old))).is_err(),
        "duplicate ownership cannot replace the old freeze"
    );
    let mut omitted = report.clone();
    omitted["cases"] = json!([case("additional", 200)]);
    assert!(
        std::panic::catch_unwind(|| verify_limits(&omitted, &limits, Some(&extra))).is_err(),
        "an extension cannot hide a missing old case"
    );
    let mut duplicate = report.clone();
    duplicate["cases"].as_array_mut().unwrap().push(case("additional", 200));
    assert!(std::panic::catch_unwind(|| verify_limits(&duplicate, &limits, Some(&extra))).is_err(), "case multiplicity is frozen");
}

fn measure(acceptance: bool) {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scope = std::env::var("CELLSCRIPT_FINANCE_COST_SCOPE").unwrap_or_else(|_| "pool,orders,c1,c2,c3,pool-boundaries".into());
    let namespaces = scope.split(',').collect::<Vec<_>>();
    assert!(
        !namespaces.is_empty()
            && namespaces.iter().all(|name| matches!(
                *name,
                "pool"
                    | "orders"
                    | "c1"
                    | "c2"
                    | "c3"
                    | "pool-boundaries"
                    | "c1-layout"
                    | "pool-admission"
                    | "pool-math-direct"
                    | "pool-math-relation"
                    | "c3-mode-boundaries"
            ))
    );
    let mut unique = namespaces.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), namespaces.len());
    let directory = repository
        .join("target/finance-operation-costs")
        .join(if acceptance { "vm-acceptance" } else { "pilot" })
        .join(namespaces.join("-"));
    fs::create_dir_all(&directory).unwrap();
    let mut cases = vec![];
    let mut programs = vec![];
    let mut input_archives = vec![];
    for namespace in namespaces.iter().copied() {
        let archive = finance_archive::load_archive(&repository.join("target/finance-protocol-evidence").join(namespace)).unwrap();
        input_archives.push(json!({"namespace":namespace,"archive_id":archive.identity(),
            "local_observations_not_measured":archive.local_observations().len()}));
        for program in archive.programs() {
            programs.push(json!({"namespace":namespace,"program":program.name,"elf_bytes":program.elf.len(),
                "elf_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(&program.elf)),
                "maximum_declared_frame_bytes":program.lowering.entries.iter().map(|entry|entry.frame_size_bytes).max(),
                "static_call_chain_bound":null,"static_bound_unavailable_reason":"not computed by this observed-VM pilot"}));
        }
        let mut count = 0;
        for (index, case) in archive.cases().iter().enumerate() {
            let name = case["case"].as_str().unwrap();
            if !selected(namespace, name) {
                continue;
            }
            count += 1;
            let started = Instant::now();
            let (context, tx, _) = archive.load_case(index).unwrap();
            let accepted = case["outcome"] == "accepted";
            let ordinary = context.verify_tx(&tx, MAX_CYCLES);
            assert_eq!(ordinary.is_ok(), accepted, "{namespace}/{index}/{name}");
            if accepted {
                assert_eq!(
                    ordinary.as_ref().unwrap(),
                    &case["transaction_cycles"].as_u64().unwrap(),
                    "fresh ordinary cycles must match the archive"
                );
            } else {
                let error = format!("{:?}", ordinary.as_ref().unwrap_err());
                assert!(error.contains("ValidationFailure"), "a VM/tool/cycle-limit failure is not a financial rejection: {error}");
                assert!(
                    case["expected_groups"].as_array().unwrap().iter().filter_map(|g| g.as_str()).any(|g| error.contains(g)),
                    "unexpected rejecting Script group: {error}"
                );
            }
            let resolved = cost_measurement::resolve_transaction(&context, &tx).unwrap();
            let consensus = ConsensusBuilder::default()
                .hardfork_switch(HardForks { ckb2021: CKB2021::new_dev_default(), ckb2023: CKB2023::new_dev_default() })
                .build();
            let verifier = TransactionScriptsVerifier::new(
                Arc::new(resolved),
                context.clone(),
                Arc::new(consensus),
                Arc::new(TxVerifyEnv::new_submit(&HeaderBuilder::default().number(0).build())),
            );
            let mut groups = vec![];
            let mut sum = Some(0u64);
            let mut prefix = Some(0u64);
            let mut failed = false;
            let mut maximum_stack = None;
            let mut guest_generations = 0usize;
            for (ordinal, (role, hash, group)) in verifier.groups_with_type().enumerate() {
                let native = group.script.code_hash() == cellscript::ckb_abi::type_id::CODE_HASH.into()
                    && u8::from(group.script.hash_type()) == 1;
                let (cycles, exit, stack, trace_file, native_error) = if native {
                    // Built-in Type ID has no guest VM or guest stack. Use
                    // its actual native verifier, not detailed_run of an ELF.
                    match verifier.verify_single(role, hash, MAX_CYCLES) {
                        Ok(cycles) => (Some(cycles), Some(0), None, None, None),
                        Err(error) => (None, None, None, None, Some(error.to_string())),
                    }
                } else {
                    let trace = cost_trace::measure(&context, &tx, &group.script, role, MAX_CYCLES);
                    assert_eq!(trace.status, "measured", "{namespace}/{index}/{ordinal}: {:?}", trace.unavailable_reason);
                    let stack = trace.vms.iter().map(|vm| vm.observed_stack_bytes).max().unwrap();
                    maximum_stack = Some(maximum_stack.unwrap_or(0u64).max(stack));
                    guest_generations += trace.vms.len();
                    let file = format!("{namespace}-{index}-{ordinal}.trace.json");
                    fs::write(directory.join(&file), serde_json::to_vec(&trace).unwrap()).unwrap();
                    (trace.authoritative_cycles, trace.exit_code, Some(stack), Some(file), None)
                };
                sum = sum.zip(cycles).map(|(total, cycles)| total.checked_add(cycles).unwrap());
                if !failed {
                    prefix = prefix.zip(cycles).map(|(total, cycles)| total.checked_add(cycles).unwrap());
                }
                if exit != Some(0) && !failed {
                    let label = if let Some(input) = group.input_indices.first() {
                        format!("Inputs[{input}]")
                    } else {
                        format!("Outputs[{}]", group.output_indices[0])
                    };
                    let role_name = if role == ScriptGroupType::Lock { "Lock" } else { "Type" };
                    let origin = format!("{label}.{role_name}");
                    assert!(
                        format!("{:?}", ordinary.as_ref().unwrap_err()).contains(&origin),
                        "ordered rejecting-group replay disagrees with ordinary transaction verdict"
                    );
                    failed = true;
                }
                groups.push(json!({"ordinal":ordinal,"role":role.to_string(),"script_hash":hex::encode(hash.as_slice()),
                    "executable_bytes":if native{None}else{Some(verifier.extract_script(&group.script).unwrap().len())},
                    "input_indices":group.input_indices,"output_indices":group.output_indices,"cycles":cycles,"exit_code":exit,
                    "native_error":native_error,
                    "observed_stack_bytes":stack,"stack_scope":if native{"not-applicable-native-system-script"}else{"entry-sp-through-scheduler-vm-generation"},
                    "trace_file":trace_file}));
            }
            assert_eq!(failed, !accepted);
            if accepted {
                assert_eq!(ordinary.as_ref().unwrap(), &sum.unwrap(), "sum of real group costs must equal full transaction cost");
            }
            let inputs = tx.inputs().into_iter().map(|input| context.get_cell(&input.previous_output()).unwrap()).collect::<Vec<_>>();
            let input_capacity = inputs.iter().map(|(output, _)| u64::from(output.capacity())).sum::<u64>();
            let output_capacity = tx.outputs().into_iter().map(|output| u64::from(output.capacity())).sum::<u64>();
            let occupied = tx
                .outputs()
                .into_iter()
                .zip(tx.outputs_data())
                .map(|(output, data)| output.occupied_capacity(Capacity::bytes(data.raw_data().len()).unwrap()).unwrap().as_u64())
                .sum::<u64>();
            cases.push(json!({"namespace":namespace,"case_index":index,"case":name,"case_id":case["case_id"],
                "archive_id":archive.identity(),
                "transaction_hash":case["transaction_hash"],"outcome":case["outcome"],"transaction_cycles":ordinary.ok(),
                "all_groups_separate_replay_cycles":sum,"ordered_group_replay_through_first_failure_cycles":if failed{prefix}else{None},
                "rejection_cycle_scope":"separate group replay in pinned verifier order, not Context::verify_tx return value",
                "maximum_observed_stack_per_vm_bytes":maximum_stack,"transaction_bytes":tx.data().as_slice().len(),
                "guest_vm_generation_count":guest_generations,
                "witness_bytes":tx.witnesses().into_iter().map(|w|w.raw_data().len()).sum::<usize>(),
                "input_capacity":input_capacity,"output_capacity":output_capacity,"occupied_output_capacity":occupied,
                "fee":input_capacity-output_capacity,"diagnostic_host_time_ns":started.elapsed().as_nanos().to_string(),
                "host_time_scope":"fresh case restore + ordinary verifier + separate group replays + trace writes; excludes initial archive validation and is not builder/node latency",
                "groups":groups}));
        }
        assert!(count > 0, "{namespace}: selected financial pilot executed zero cases");
    }
    let complete_original_portfolio = ["c1", "c2", "c3", "orders", "pool", "pool-boundaries"].iter().all(|name| unique.contains(name));
    let mut report = json!({
        "schema":"cellscript-finance-operation-cost-pilot-v1","compiler_version":env!("CARGO_PKG_VERSION"),
        "toolchain_contract":fs::read_to_string(repository.join("rust-toolchain.toml")).unwrap(),
        "cargo_lock_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(&fs::read(repository.join("Cargo.lock")).unwrap())),
        "host_environment":{"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
            "available_parallelism":std::thread::available_parallelism().unwrap().get(),"debug_assertions":cfg!(debug_assertions)},
        "execution_policy":"one fresh Context per archived case; one ordinary transaction oracle and separate observed replay per group; no statistical timing guarantee",
        "frozen_financial_budgets":false,"node_timings":null,"external_reproduction":null,
        "frozen_vm_case_budgets":acceptance,"host_and_node_budget_acceptance":null,
        "stack_aggregation":"maximum per VM generation across independent groups, not simultaneous sum",
        "requested_namespaces":namespaces,"complete_original_portfolio":complete_original_portfolio,
        "input_archives":input_archives,"programs":programs,"cases":cases,
    });
    if acceptance {
        let bytes = fs::read(repository.join("tests/fixtures/finance_operation_vm_limits.json")).unwrap();
        let limits: Value = serde_json::from_slice(&bytes).unwrap();
        let extension = repository.join("tests/fixtures/finance_operation_vm_extra_limits.json");
        let extra_bytes = extension.exists().then(|| fs::read(extension).unwrap());
        let extra: Option<Value> = extra_bytes.as_ref().map(|bytes| serde_json::from_slice(bytes).unwrap());
        if let Some(extra) = &extra {
            assert_eq!(
                extra["original_limits_hash"],
                hex::encode(cellscript_artifact_checker::ckb_blake2b256(&bytes)),
                "additive budget must pin the unchanged original freeze"
            );
        }
        verify_limits(&report, &limits, extra.as_ref());
        report["extra_vm_limits_hash"] = extra_bytes
            .as_ref()
            .map(|bytes| json!(hex::encode(cellscript_artifact_checker::ckb_blake2b256(bytes))))
            .unwrap_or(Value::Null);
        report["vm_limits_hash"] = json!(hex::encode(cellscript_artifact_checker::ckb_blake2b256(&bytes)));
    }
    fs::write(directory.join("report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
