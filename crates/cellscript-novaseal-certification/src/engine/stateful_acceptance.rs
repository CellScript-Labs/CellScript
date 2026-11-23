use super::endpoint_evidence::{external_endpoint_coverage_summary, fiber_node_execution_summary, live_planned_profile_summary};
use super::live_reports::{agreement_live_summary_passed, core_live_summary_passed, live_agreement_summary, live_core_summary};
use super::package_sources::read_cell_sources;
use super::report_files::json_load_path_optional;
use super::{
    blocker, find_actions, has_agreement_origination_surface, has_core_bootstrap_surface, has_dispatcher_surface, json_pointer_bool,
    json_pointer_str, object_values_all_true, summary_from_report, AGREEMENT_LIVE, AGREEMENT_ROOT, BTC_TX_COMMITMENT_LIVE,
    BTC_TX_COMMITMENT_MANIFEST, BTC_UTXO_SEAL_LIVE, BTC_UTXO_SEAL_MANIFEST, CORE_LIVE, CORE_ROOT, DUAL_SEAL_LIVE, DUAL_SEAL_MANIFEST,
    FIBER_CANDIDATE_LIVE, FIBER_CANDIDATE_MANIFEST, FIBER_NODE_EXPERIMENTS, FUNGIBLE_XUDT_LIVE, FUNGIBLE_XUDT_MANIFEST,
    RWA_RECEIPT_LIVE, RWA_RECEIPT_MANIFEST, VERIFIER_ROOT,
};
use crate::error::Result;
use crate::IMPLEMENTATION_ID;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn build_stateful_acceptance_report(
    repo_root: &Path,
    agreement_conformance: &Value,
    btc_spv_evidence: &Value,
) -> Result<Value> {
    let core_source = read_cell_sources(&repo_root.join(CORE_ROOT).join("src"))?;
    let agreement_source = read_cell_sources(&repo_root.join(AGREEMENT_ROOT).join("src"))?;
    let core_actions = find_actions(&core_source);
    let agreement_actions = find_actions(&agreement_source);
    let core_combined =
        json_load_path_optional(repo_root, &repo_root.join(CORE_ROOT).join("target/novaseal-combined-tx-report.json"))?;
    let agreement_tx =
        json_load_path_optional(repo_root, &repo_root.join(AGREEMENT_ROOT).join("target/nova-agreement-ckb-tx-report.json"))?;
    let live_core_report = json_load_path_optional(repo_root, &repo_root.join(CORE_LIVE))?;
    let live_agreement_report = json_load_path_optional(repo_root, &repo_root.join(AGREEMENT_LIVE))?;
    let live_fungible_xudt_report = json_load_path_optional(repo_root, &repo_root.join(FUNGIBLE_XUDT_LIVE))?;
    let live_rwa_receipt_report = json_load_path_optional(repo_root, &repo_root.join(RWA_RECEIPT_LIVE))?;
    let live_btc_tx_commitment_report = json_load_path_optional(repo_root, &repo_root.join(BTC_TX_COMMITMENT_LIVE))?;
    let live_btc_utxo_seal_report = json_load_path_optional(repo_root, &repo_root.join(BTC_UTXO_SEAL_LIVE))?;
    let live_dual_seal_report = json_load_path_optional(repo_root, &repo_root.join(DUAL_SEAL_LIVE))?;
    let live_fiber_candidate_report = json_load_path_optional(repo_root, &repo_root.join(FIBER_CANDIDATE_LIVE))?;
    let fiber_node_experiments_report = json_load_path_optional(repo_root, &repo_root.join(FIBER_NODE_EXPERIMENTS))?;
    let live_core = live_core_summary(repo_root, live_core_report.as_ref())?;
    let live_agreement = live_agreement_summary(repo_root, live_agreement_report.as_ref())?;
    let live_fungible_xudt = live_planned_profile_summary(
        repo_root,
        live_fungible_xudt_report.as_ref(),
        &[
            FUNGIBLE_XUDT_MANIFEST,
            "proposals/novaseal/fungible-xudt-profile-v0/src",
            "proposals/novaseal/fungible-xudt-profile-v0/schemas",
            VERIFIER_ROOT,
            "crates/cellscript-tools/src/novaseal_planned_fungible.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ],
        &[("issue", "/issue/commit/tx_hash"), ("transfer", "/transfer/commit/tx_hash"), ("settle", "/settle/commit/tx_hash")],
        &[
            ("issue_balance_live", "/issue/balance_live"),
            ("issue_receipt_live", "/issue/receipt_live"),
            ("transfer_old_balance_not_live", "/transfer/old_balance_not_live"),
            ("transfer_sender_balance_live", "/transfer/sender_balance_live"),
            ("transfer_receiver_balance_live", "/transfer/receiver_balance_live"),
            ("transfer_receipt_live", "/transfer/receipt_live"),
            ("transfer_amount_conserved", "/transfer/amount_conserved"),
            ("settle_old_balance_not_live", "/settle/old_balance_not_live"),
            ("settlement_receipt_live", "/settle/settlement_receipt_live"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ],
        &[
            ("wrong_holder_signature_rejected", "wrong_holder_signature_dry_run"),
            ("transfer_amount_mismatch_rejected", "transfer_amount_mismatch_dry_run"),
            ("settle_wrong_holder_signature_rejected", "settle_wrong_holder_signature_dry_run"),
        ],
    )?;
    let live_rwa_receipt = live_planned_profile_summary(
        repo_root,
        live_rwa_receipt_report.as_ref(),
        &[
            RWA_RECEIPT_MANIFEST,
            "proposals/novaseal/rwa-receipt-profile-v0/src",
            "proposals/novaseal/rwa-receipt-profile-v0/schemas",
            VERIFIER_ROOT,
            "crates/cellscript-tools/src/novaseal_planned_rwa.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ],
        &[("materialize", "/materialize/commit/tx_hash"), ("claim", "/claim/commit/tx_hash"), ("settle", "/settle/commit/tx_hash")],
        &[
            ("materialized_receipt_live", "/materialize/receipt_live"),
            ("materialized_audit_event_live", "/materialize/audit_event_live"),
            ("claim_old_receipt_not_live", "/claim/old_receipt_not_live"),
            ("claimed_receipt_live", "/claim/claimed_receipt_live"),
            ("claim_event_live", "/claim/claim_event_live"),
            ("settle_old_claim_not_live", "/settle/old_claim_not_live"),
            ("settlement_receipt_live", "/settle/settlement_receipt_live"),
            ("settlement_event_live", "/settle/settlement_event_live"),
            ("amount_conserved", "/settle/amount_conserved"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ],
        &[
            ("wrong_holder_claim_rejected", "wrong_holder_claim_dry_run"),
            ("wrong_issuer_settlement_rejected", "wrong_issuer_settlement_dry_run"),
            ("amount_mutation_rejected", "amount_mutation_dry_run"),
        ],
    )?;
    let live_btc_tx_commitment = live_planned_profile_summary(
        repo_root,
        live_btc_tx_commitment_report.as_ref(),
        &[
            BTC_TX_COMMITMENT_MANIFEST,
            "proposals/novaseal/btc-transaction-commitment-profile-v0/src",
            "proposals/novaseal/btc-transaction-commitment-profile-v0/schemas",
            VERIFIER_ROOT,
            "crates/cellscript-tools/src/novaseal_planned_btc_tx.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ],
        &[("commit_transaction", "/commit_transaction/commit/tx_hash")],
        &[
            ("old_state_not_live", "/commit_transaction/old_state_not_live"),
            ("new_state_live", "/commit_transaction/new_state_live"),
            ("receipt_live", "/commit_transaction/receipt_live"),
            ("btc_tx_tuple_bound", "/commit_transaction/btc_tx_tuple_bound"),
            ("transition_commitment_bound", "/commit_transaction/transition_commitment_bound"),
            ("public_btc_verification_executed", "/commit_transaction/public_btc_verification_executed"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ],
        &[
            ("wrong_committer_signature_rejected", "wrong_committer_signature_dry_run"),
            ("zero_btc_txid_rejected", "zero_btc_txid_dry_run"),
            ("transition_hash_mismatch_rejected", "transition_hash_mismatch_dry_run"),
        ],
    )?;
    let live_btc_utxo_seal = live_planned_profile_summary(
        repo_root,
        live_btc_utxo_seal_report.as_ref(),
        &[
            BTC_UTXO_SEAL_MANIFEST,
            "proposals/novaseal/btc-utxo-seal-profile-v0/src",
            "proposals/novaseal/btc-utxo-seal-profile-v0/schemas",
            VERIFIER_ROOT,
            "crates/cellscript-tools/src/novaseal_planned_btc_utxo.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ],
        &[("close_utxo_seal", "/close_utxo_seal/commit/tx_hash")],
        &[
            ("old_state_not_live", "/close_utxo_seal/old_state_not_live"),
            ("new_state_live", "/close_utxo_seal/new_state_live"),
            ("receipt_live", "/close_utxo_seal/receipt_live"),
            ("sealed_utxo_tuple_bound", "/close_utxo_seal/sealed_utxo_tuple_bound"),
            ("spend_tuple_bound", "/close_utxo_seal/spend_tuple_bound"),
            ("public_btc_spend_verification_executed", "/close_utxo_seal/public_btc_spend_verification_executed"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ],
        &[
            ("wrong_owner_signature_rejected", "wrong_owner_signature_dry_run"),
            ("utxo_commitment_mismatch_rejected", "utxo_commitment_mismatch_dry_run"),
            ("zero_spend_txid_rejected", "zero_spend_txid_dry_run"),
        ],
    )?;
    let live_dual_seal = live_planned_profile_summary(
        repo_root,
        live_dual_seal_report.as_ref(),
        &[
            DUAL_SEAL_MANIFEST,
            "proposals/novaseal/dual-seal-profile-v0/src",
            "proposals/novaseal/dual-seal-profile-v0/schemas",
            VERIFIER_ROOT,
            "crates/cellscript-tools/src/novaseal_planned_dual.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ],
        &[("finalize_dual_seal", "/finalize_dual_seal/commit/tx_hash")],
        &[
            ("old_state_not_live", "/finalize_dual_seal/old_state_not_live"),
            ("receipt_live", "/finalize_dual_seal/receipt_live"),
            ("btc_closure_bound", "/finalize_dual_seal/btc_closure_bound"),
            ("ckb_maturity_executed", "/finalize_dual_seal/ckb_maturity_executed"),
            ("dual_authority_executed", "/finalize_dual_seal/dual_authority_executed"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ],
        &[
            ("wrong_btc_owner_signature_rejected", "wrong_btc_owner_signature_dry_run"),
            ("wrong_ckb_authority_signature_rejected", "wrong_ckb_authority_signature_dry_run"),
            ("btc_closure_commitment_missing_rejected", "btc_closure_commitment_missing_dry_run"),
        ],
    )?;
    let live_fiber_candidate = live_planned_profile_summary(
        repo_root,
        live_fiber_candidate_report.as_ref(),
        &[
            FIBER_CANDIDATE_MANIFEST,
            "proposals/novaseal/fiber-candidate-profile-v0/src",
            "proposals/novaseal/fiber-candidate-profile-v0/schemas",
            VERIFIER_ROOT,
            "crates/cellscript-tools/src/novaseal_planned_fiber.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ],
        &[("settle_fiber_candidate", "/settle_fiber_candidate/commit/tx_hash")],
        &[
            ("old_candidate_not_live", "/settle_fiber_candidate/old_candidate_not_live"),
            ("new_candidate_live", "/settle_fiber_candidate/new_candidate_live"),
            ("receipt_live", "/settle_fiber_candidate/receipt_live"),
            ("balance_commitment_progressed", "/settle_fiber_candidate/balance_commitment_progressed"),
            ("fiber_execution_executed", "/settle_fiber_candidate/fiber_execution_executed"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ],
        &[
            ("wrong_operator_signature_rejected", "wrong_operator_signature_dry_run"),
            ("balance_commitment_replay_rejected", "balance_commitment_replay_dry_run"),
        ],
    )?;
    let fiber_node_experiments = fiber_node_execution_summary(repo_root, fiber_node_experiments_report.as_ref());
    let external_endpoint_coverage = external_endpoint_coverage_summary(
        btc_spv_evidence,
        &fiber_node_experiments,
        &live_btc_tx_commitment,
        &live_btc_utxo_seal,
        &live_dual_seal,
        &live_fiber_candidate,
    );

    let core_live_passed = core_live_summary_passed(&live_core);
    let agreement_live_passed = agreement_live_summary_passed(&live_agreement, agreement_conformance);
    let fungible_xudt_live_passed = json_pointer_bool(&live_fungible_xudt, "/required_live_checks_passed");
    let rwa_receipt_live_passed = json_pointer_bool(&live_rwa_receipt, "/required_live_checks_passed");
    let btc_tx_commitment_live_passed = json_pointer_bool(&live_btc_tx_commitment, "/required_live_checks_passed");
    let btc_utxo_seal_live_passed = json_pointer_bool(&live_btc_utxo_seal, "/required_live_checks_passed");
    let dual_seal_live_passed = json_pointer_bool(&live_dual_seal, "/required_live_checks_passed");
    let fiber_candidate_live_passed = json_pointer_bool(&live_fiber_candidate, "/required_live_checks_passed");
    let agreement_profile_actions_present = ["originate_agreement", "repay_before_expiry", "claim_after_expiry"]
        .iter()
        .all(|expected| agreement_actions.iter().any(|action| action.name == *expected));
    let agreement_originate_live = ["origin_active_live", "origin_principal_payout_live", "origin_receipt_live"]
        .iter()
        .all(|key| json_pointer_bool(&live_agreement, &format!("/{key}")));
    let agreement_repay_live = [
        "repay_old_active_not_live",
        "repay_closed_live",
        "repay_lender_repayment_live",
        "repay_borrower_collateral_return_live",
        "repay_receipt_live",
    ]
    .iter()
    .all(|key| json_pointer_bool(&live_agreement, &format!("/{key}")));
    let agreement_claim_live =
        ["claim_old_active_not_live", "claim_closed_live", "claim_lender_default_claim_live", "claim_receipt_live"]
            .iter()
            .all(|key| json_pointer_bool(&live_agreement, &format!("/{key}")));
    let agreement_negative_business_cases_preserve_live_state = [
        "wrong_lender_signature_rejected",
        "non_ckb_asset_kind_rejected",
        "wrong_borrower_signature_rejected",
        "repay_payout_capacity_short_rejected",
        "repay_payout_lock_args_mismatch_rejected",
        "repay_wrong_payout_amount_rejected",
        "early_claim_rejected",
        "wrong_lender_claim_signature_rejected",
        "post_negative_active_still_live",
        "post_claim_negative_active_still_live",
    ]
    .iter()
    .all(|key| json_pointer_bool(&live_agreement, &format!("/{key}")));
    let profile_coverage_checks = json!({
        "core_profile_live_stateful": core_live_passed,
        "agreement_profile_live_stateful": agreement_live_passed,
        "fungible_xudt_profile_live_stateful": fungible_xudt_live_passed,
        "rwa_receipt_profile_live_stateful": rwa_receipt_live_passed,
        "btc_transaction_commitment_live_stateful": btc_tx_commitment_live_passed,
        "btc_utxo_seal_live_stateful": btc_utxo_seal_live_passed,
        "dual_seal_live_stateful": dual_seal_live_passed,
        "fiber_candidate_live_stateful": fiber_candidate_live_passed,
        "core_profile_actions_present": !core_actions.is_empty(),
        "agreement_profile_actions_present": agreement_profile_actions_present,
        "distinct_profiles_covered": core_live_passed
            && agreement_live_passed
            && fungible_xudt_live_passed
            && rwa_receipt_live_passed
            && btc_tx_commitment_live_passed
            && btc_utxo_seal_live_passed
            && dual_seal_live_passed
            && fiber_candidate_live_passed,
    });
    let profile_coverage_passed = object_values_all_true(Some(&profile_coverage_checks));
    let business_scenario_checks = json!({
        "core_bootstrap_transition_live": core_live_passed,
        "agreement_originate_live": agreement_originate_live,
        "agreement_repay_live": agreement_repay_live,
        "agreement_claim_live": agreement_claim_live,
        "agreement_negative_business_cases_preserve_live_state": agreement_negative_business_cases_preserve_live_state,
        "fungible_xudt_value_flow_live": fungible_xudt_live_passed,
        "rwa_receipt_lifecycle_live": rwa_receipt_live_passed,
        "btc_transaction_commitment_transition_live": btc_tx_commitment_live_passed,
        "btc_utxo_seal_closure_live": btc_utxo_seal_live_passed,
        "dual_seal_finality_live": dual_seal_live_passed,
        "fiber_candidate_path_live": fiber_candidate_live_passed,
    });
    let business_scenario_coverage_passed = object_values_all_true(Some(&business_scenario_checks));

    let mut core_blockers = Vec::new();
    if !core_live_passed && !has_core_bootstrap_surface(&core_source) {
        core_blockers.push(blocker(
            "NovaSeal core has key_auth_transition but no bootstrap/genesis/seed action that can create the first live NovaSealCellV0.",
            "creating an initial live state cell on devnet before the first transition",
        ));
    }
    if !core_live_passed && !has_dispatcher_surface(&core_source, &repo_root.join(CORE_ROOT)) {
        core_blockers.push(blocker(
            "NovaSeal core is still compiled as a single entry action/lock surface, not a stable lifecycle dispatcher type script.",
            "preserving one script identity across create, transition, and future terminal paths",
        ));
    }

    let mut agreement_blockers = Vec::new();
    let agreement_action_names = agreement_actions.iter().map(|action| action.name.as_str()).collect::<BTreeSet<_>>();
    let expected_agreement_actions =
        ["originate_agreement", "repay_before_expiry", "claim_after_expiry"].into_iter().collect::<BTreeSet<_>>();
    if expected_agreement_actions.is_subset(&agreement_action_names)
        && !has_dispatcher_surface(&agreement_source, &repo_root.join(AGREEMENT_ROOT))
    {
        agreement_blockers.push(blocker(
            "Agreement Profile compiles originate/repay/claim as separate entry-action ELFs; a live CKB Cell cannot move from originate ELF identity to repay/claim ELF identity.",
            "originate -> repay or originate -> claim live-cell lifecycle",
        ));
    }
    if !has_agreement_origination_surface(&agreement_source) {
        agreement_blockers.push(blocker(
            "Agreement Profile has no output-only origination action suitable for creating the initial agreement cell.",
            "first live agreement cell creation",
        ));
    }
    if json_pointer_str(agreement_conformance, "/status") != Some("passed") {
        let failed = agreement_conformance
            .get("checks")
            .and_then(Value::as_object)
            .map(|checks| {
                checks.iter().filter(|(_, value)| value.as_bool() != Some(true)).map(|(name, _)| name.clone()).collect::<Vec<_>>()
            })
            .unwrap_or_default();
        agreement_blockers.push(blocker(
            &format!("Agreement Profile does not satisfy NovaSealCanonicalV0 conformance: {}.", failed.join(", ")),
            "claiming Agreement Profile as a NovaSeal profile",
        ));
    }

    let scenarios = vec![
        json!({
            "name": "novaseal_core_key_auth_transition",
            "status": if !core_blockers.is_empty() { "blocked" } else if core_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": core_live_passed,
            "stateful_lifecycle_executed": core_live_passed,
            "actions": core_actions.iter().map(|action| action.name.clone()).collect::<Vec<_>>(),
            "blockers": core_blockers,
            "live_devnet_evidence": live_core,
            "existing_local_evidence": summary_from_report(core_combined.as_ref(), &[
                "combined_full_transaction_executed",
                "ckb_node_verification_stack_executed",
                "total_cases",
                "matched_expected",
                "node_stack_matched_expected",
                "lock_and_type_script_groups_present",
            ]),
        }),
        json!({
            "name": "agreement_profile_originate_to_terminal",
            "status": if !agreement_blockers.is_empty() { "blocked" } else if agreement_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": agreement_live_passed,
            "stateful_lifecycle_executed": agreement_live_passed,
            "actions": agreement_actions.iter().map(|action| action.name.clone()).collect::<Vec<_>>(),
            "blockers": agreement_blockers,
            "live_devnet_evidence": live_agreement,
            "conformance_evidence": agreement_conformance,
            "existing_local_evidence": summary_from_report(agreement_tx.as_ref(), &[
                "resolved_transaction_harness_executed",
                "ckb_node_verification_stack_executed",
                "total_cases",
                "script_matched_expected",
                "node_matched_expected",
                "fixture_files_not_executed_by_tx_harness",
            ]),
        }),
        json!({
            "name": "fungible_xudt_issue_transfer_settle",
            "status": if fungible_xudt_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": fungible_xudt_live_passed,
            "stateful_lifecycle_executed": fungible_xudt_live_passed,
            "actions": ["issue_xudt", "transfer_xudt", "settle_xudt"],
            "blockers": [],
            "live_devnet_evidence": live_fungible_xudt,
        }),
        json!({
            "name": "rwa_receipt_materialize_claim_settle",
            "status": if rwa_receipt_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": rwa_receipt_live_passed,
            "stateful_lifecycle_executed": rwa_receipt_live_passed,
            "actions": ["materialize_rwa_receipt", "claim_rwa_receipt", "settle_rwa_receipt"],
            "blockers": [],
            "live_devnet_evidence": live_rwa_receipt,
        }),
        json!({
            "name": "btc_transaction_commitment_transition",
            "status": if btc_tx_commitment_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": btc_tx_commitment_live_passed,
            "stateful_lifecycle_executed": btc_tx_commitment_live_passed,
            "actions": ["commit_btc_transaction_transition"],
            "blockers": [],
            "live_devnet_evidence": live_btc_tx_commitment,
        }),
        json!({
            "name": "btc_utxo_seal_closure",
            "status": if btc_utxo_seal_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": btc_utxo_seal_live_passed,
            "stateful_lifecycle_executed": btc_utxo_seal_live_passed,
            "actions": ["close_btc_utxo_seal"],
            "blockers": [],
            "live_devnet_evidence": live_btc_utxo_seal,
        }),
        json!({
            "name": "dual_seal_finality",
            "status": if dual_seal_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": dual_seal_live_passed,
            "stateful_lifecycle_executed": dual_seal_live_passed,
            "actions": ["finalize_dual_seal"],
            "blockers": [],
            "live_devnet_evidence": live_dual_seal,
        }),
        json!({
            "name": "fiber_candidate_settlement",
            "status": if fiber_candidate_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": fiber_candidate_live_passed,
            "stateful_lifecycle_executed": fiber_candidate_live_passed,
            "actions": ["settle_fiber_candidate"],
            "blockers": [],
            "live_devnet_evidence": live_fiber_candidate,
            "external_fiber_node_evidence": fiber_node_experiments.clone(),
        }),
    ];
    let profile_coverage = json!({
        "status": if profile_coverage_passed { "passed" } else { "failed" },
        "required_profiles": [
            "novaseal-core-v0",
            "agreement-profile-v0",
            "fungible-xudt-profile-v0",
            "rwa-receipt-profile-v0",
            "btc-transaction-commitment-profile-v0",
            "btc-utxo-seal-profile-v0",
            "dual-seal-profile-v0",
            "fiber-candidate-profile-v0",
        ],
        "covered_profiles": [
            {
                "profile": "novaseal-core-v0",
                "scenario": "novaseal_core_key_auth_transition",
                "status": if core_live_passed { "passed" } else { "failed" },
                "actions": core_actions.iter().map(|action| action.name.clone()).collect::<Vec<_>>(),
            },
            {
                "profile": "agreement-profile-v0",
                "scenario": "agreement_profile_originate_to_terminal",
                "status": if agreement_live_passed { "passed" } else { "failed" },
                "actions": agreement_actions.iter().map(|action| action.name.clone()).collect::<Vec<_>>(),
            },
            {
                "profile": "fungible-xudt-profile-v0",
                "scenario": "fungible_xudt_issue_transfer_settle",
                "status": if fungible_xudt_live_passed { "passed" } else { "failed" },
                "actions": ["issue_xudt", "transfer_xudt", "settle_xudt"],
            },
            {
                "profile": "rwa-receipt-profile-v0",
                "scenario": "rwa_receipt_materialize_claim_settle",
                "status": if rwa_receipt_live_passed { "passed" } else { "failed" },
                "actions": ["materialize_rwa_receipt", "claim_rwa_receipt", "settle_rwa_receipt"],
            },
            {
                "profile": "btc-transaction-commitment-profile-v0",
                "scenario": "btc_transaction_commitment_transition",
                "status": if btc_tx_commitment_live_passed { "passed" } else { "failed" },
                "actions": ["commit_btc_transaction_transition"],
            },
            {
                "profile": "btc-utxo-seal-profile-v0",
                "scenario": "btc_utxo_seal_closure",
                "status": if btc_utxo_seal_live_passed { "passed" } else { "failed" },
                "actions": ["close_btc_utxo_seal"],
            },
            {
                "profile": "dual-seal-profile-v0",
                "scenario": "dual_seal_finality",
                "status": if dual_seal_live_passed { "passed" } else { "failed" },
                "actions": ["finalize_dual_seal"],
            },
            {
                "profile": "fiber-candidate-profile-v0",
                "scenario": "fiber_candidate_settlement",
                "status": if fiber_candidate_live_passed { "passed" } else { "failed" },
                "actions": ["settle_fiber_candidate"],
            },
        ],
        "checks": profile_coverage_checks,
    });
    let business_scenario_coverage = json!({
        "status": if business_scenario_coverage_passed { "passed" } else { "failed" },
        "required_business_scenarios": [
            "core bootstrap -> key-authorised transition",
            "agreement originate -> active agreement plus principal payout plus receipt",
            "agreement active -> repaid terminal plus lender repayment plus borrower collateral return plus receipt",
            "agreement active -> defaulted terminal plus lender collateral claim plus receipt",
            "negative business/security dry-runs reject without mutating live state",
            "fungible/xUDT issue -> transfer -> settlement with negative accounting dry-runs",
            "RWA receipt materialise -> claim -> settlement with immutable audit event evidence",
            "public BTC transaction commitment authorised transition",
            "BTC UTXO single-use seal closure over a CKB transition",
            "dual-seal terminal finality after CKB maturity and declared BTC closure",
            "Fiber-compatible candidate settlement with balance commitment progress",
        ],
        "checks": business_scenario_checks,
    });
    let local_blockers = scenarios
        .iter()
        .flat_map(|scenario| scenario.get("blockers").and_then(Value::as_array).into_iter().flatten().cloned())
        .collect::<Vec<_>>();
    let acceptance_blockers = stateful_live_acceptance_blockers(
        &scenarios,
        profile_coverage_passed,
        business_scenario_coverage_passed,
        &external_endpoint_coverage,
    );
    let all_blockers = local_blockers.iter().cloned().chain(acceptance_blockers.iter().cloned()).collect::<Vec<_>>();
    let local_live_acceptance_passed = scenarios.iter().all(|scenario| json_pointer_str(scenario, "/status") == Some("passed"))
        && profile_coverage_passed
        && business_scenario_coverage_passed;
    let status = stateful_acceptance_status(
        local_blockers.len(),
        acceptance_blockers.len(),
        local_live_acceptance_passed,
        core_live_passed,
        agreement_live_passed,
        &external_endpoint_coverage,
    );

    Ok(json!({
        "schema": "novaseal-devnet-stateful-acceptance-v0.1",
        "classification": "live_devnet_stateful_release_gate",
        "status": status,
        "production_ready": false,
        "live_devnet_rpc_executed": scenarios.iter().all(|scenario| json_pointer_bool(scenario, "/live_devnet_rpc_executed")),
        "stateful_lifecycle_executed": scenarios.iter().all(|scenario| json_pointer_bool(scenario, "/stateful_lifecycle_executed")),
        "repo_root": repo_root.display().to_string(),
        "requirements": [
            "deploy runtime verifier and protocol artifacts as live CellDeps",
            "submit transactions through CKB RPC, not only in-memory ResolvedTransaction",
            "commit each valid step and verify old inputs are dead plus new state/receipt/payout outputs are live",
            "verify live output capacity/lock/type/data and reject stale source/artifact provenance",
            "prove negative dry-runs fail from the expected lifecycle script and artifact hash",
            "use one stable type-script identity for a lifecycle, or an explicitly audited dispatcher/bootstrap surface",
            "run negative cases as dry-run/send-test rejections without mutating live state",
            "require every NovaSeal profile to pass conforms_to = NovaSealCanonicalV0 conformance",
            "cover every planned NovaSeal V1 profile in the live stateful gate",
            "cover bootstrap, origination, repayment, default claim, payout, xUDT value-flow, RWA receipt, BTC commitment, BTC UTXO closure, Fiber candidate, receipt, and negative business/security paths",
        ],
        "profile_coverage": profile_coverage,
        "business_scenario_coverage": business_scenario_coverage,
        "external_experiment_coverage": {
            "status": if json_pointer_bool(&fiber_node_experiments, "/all_required_workflows_executed_passed") {
                "passed"
            } else if json_pointer_bool(&fiber_node_experiments, "/partial_execution_passed") {
                "partial_execution_passed"
            } else if json_pointer_bool(&fiber_node_experiments, "/discovery_ready") {
                "discovery_ready_live_not_run"
            } else {
                "missing"
            },
            "required_after_novaseal_local_v1": true,
            "fiber_node_execution": fiber_node_experiments,
            "boundary": "External Fiber-node workflow coverage is separate from NovaSeal's own CKB stateful profile acceptance. It must pass before claiming Fiber production execution coverage.",
        },
        "external_endpoint_coverage": external_endpoint_coverage,
        "scenarios": scenarios,
        "local_blocker_count": local_blockers.len(),
        "local_blockers": local_blockers,
        "acceptance_blocker_count": acceptance_blockers.len(),
        "acceptance_blockers": acceptance_blockers,
        "blocker_count": all_blockers.len(),
        "blockers": all_blockers,
        "next_engineering_step": if status == "passed" {
            "Stateful live-devnet acceptance is complete; production readiness is now governed by public CellDep pinning, wallet/Molecule vectors, and external verifier TCB attestation."
        } else {
            "Run the live devnet runners for core, Agreement, and every planned V1 profile after source or artifact changes; this gate fails closed until all reports have fresh provenance, strict output checks, and matched negative dry-run errors."
        },
        "generated_by": {
            "implementation": IMPLEMENTATION_ID,
            "language": "rust",
        },
    }))
}

pub(super) fn stateful_acceptance_status(
    local_blocker_count: usize,
    acceptance_blocker_count: usize,
    local_live_acceptance_passed: bool,
    core_live_passed: bool,
    agreement_live_passed: bool,
    external_endpoint_coverage: &Value,
) -> &'static str {
    if local_blocker_count > 0 {
        "blocked"
    } else if local_live_acceptance_passed && json_pointer_str(external_endpoint_coverage, "/status") == Some("external_required") {
        "local_devnet_passed_external_endpoint_required"
    } else if local_live_acceptance_passed && json_pointer_str(external_endpoint_coverage, "/status") != Some("passed") {
        "local_devnet_passed_acceptance_blockers"
    } else if local_live_acceptance_passed && acceptance_blocker_count == 0 {
        "passed"
    } else if local_live_acceptance_passed {
        "local_devnet_passed_acceptance_blockers"
    } else if core_live_passed && !agreement_live_passed {
        "core_live_devnet_passed_agreement_pending"
    } else if agreement_live_passed && !core_live_passed {
        "agreement_live_devnet_passed_core_pending"
    } else {
        "ready_to_run_live_devnet"
    }
}

pub(super) fn stateful_live_acceptance_blockers(
    scenarios: &[Value],
    profile_coverage_passed: bool,
    business_scenario_coverage_passed: bool,
    external_endpoint_coverage: &Value,
) -> Vec<Value> {
    let mut blockers = Vec::new();
    for scenario in scenarios {
        if json_pointer_str(scenario, "/status") == Some("passed") {
            continue;
        }
        let name = json_pointer_str(scenario, "/name").unwrap_or("unknown");
        let status = json_pointer_str(scenario, "/status").unwrap_or("missing");
        blockers.push(json!({
            "blocker": format!("NovaSeal live devnet scenario `{name}` has not passed ({status})."),
            "scenario": name,
            "status": status,
            "live_devnet_rpc_executed": json_pointer_bool(scenario, "/live_devnet_rpc_executed"),
            "stateful_lifecycle_executed": json_pointer_bool(scenario, "/stateful_lifecycle_executed"),
            "required_for": "full NovaSeal stateful devnet acceptance",
        }));
    }
    if !profile_coverage_passed {
        blockers.push(json!({
            "blocker": "NovaSeal required profile coverage has not passed.",
            "dimension": "profile_coverage",
            "required_for": "multi-profile NovaSeal V1 acceptance",
        }));
    }
    if !business_scenario_coverage_passed {
        blockers.push(json!({
            "blocker": "NovaSeal required business scenario coverage has not passed.",
            "dimension": "business_scenario_coverage",
            "required_for": "multi-scenario NovaSeal V1 acceptance",
        }));
    }
    if json_pointer_str(external_endpoint_coverage, "/status") != Some("passed") {
        blockers.push(json!({
            "blocker": "NovaSeal BTC/Fiber external endpoint coverage has not passed.",
            "dimension": "external_endpoint_coverage",
            "status": json_pointer_str(external_endpoint_coverage, "/status").unwrap_or("missing"),
            "btc_status": json_pointer_str(external_endpoint_coverage, "/btc/status").unwrap_or("missing"),
            "fiber_status": json_pointer_str(external_endpoint_coverage, "/fiber/status").unwrap_or("missing"),
            "required_for": "real BTC SPV and Fiber endpoint production acceptance",
        }));
    }
    blockers
}
