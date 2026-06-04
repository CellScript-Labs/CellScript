use crate::error::{CompileError, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) const IMPLEMENTATION_ID: &str = "cellscript::cli::novaseal_certification";

const AGREEMENT_ROOT: &str = "proposals/novaseal/agreement-profile-v0";
const FUNGIBLE_XUDT_ROOT: &str = "proposals/novaseal/fungible-xudt-profile-v0";
const RWA_RECEIPT_ROOT: &str = "proposals/novaseal/rwa-receipt-profile-v0";
const BTC_TX_COMMITMENT_ROOT: &str = "proposals/novaseal/btc-transaction-commitment-profile-v0";
const BTC_UTXO_SEAL_ROOT: &str = "proposals/novaseal/btc-utxo-seal-profile-v0";
const DUAL_SEAL_ROOT: &str = "proposals/novaseal/dual-seal-profile-v0";
const FIBER_CANDIDATE_ROOT: &str = "proposals/novaseal/fiber-candidate-profile-v0";
const CORE_ROOT: &str = "proposals/novaseal/v0-mvp-skeleton";
const VERIFIER_ROOT: &str = "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier";
const CORE_MANIFEST: &str = "proposals/novaseal/v0-mvp-skeleton/Cell.toml";
const AGREEMENT_MANIFEST: &str = "proposals/novaseal/agreement-profile-v0/Cell.toml";
const FUNGIBLE_XUDT_MANIFEST: &str = "proposals/novaseal/fungible-xudt-profile-v0/Cell.toml";
const RWA_RECEIPT_MANIFEST: &str = "proposals/novaseal/rwa-receipt-profile-v0/Cell.toml";
const BTC_TX_COMMITMENT_MANIFEST: &str = "proposals/novaseal/btc-transaction-commitment-profile-v0/Cell.toml";
const BTC_UTXO_SEAL_MANIFEST: &str = "proposals/novaseal/btc-utxo-seal-profile-v0/Cell.toml";
const DUAL_SEAL_MANIFEST: &str = "proposals/novaseal/dual-seal-profile-v0/Cell.toml";
const FIBER_CANDIDATE_MANIFEST: &str = "proposals/novaseal/fiber-candidate-profile-v0/Cell.toml";
const CANONICAL_SCHEMA: &str = "proposals/novaseal/v0-mvp-skeleton/schemas/nova_seal_canonical_envelope_v0.schema";
const CORE_LIVE: &str = "target/novaseal-devnet-stateful-live.json";
const AGREEMENT_LIVE: &str = "target/novaseal-agreement-devnet-stateful-live.json";
const FUNGIBLE_XUDT_LIVE: &str = "target/novaseal-fungible-xudt-devnet-stateful-live.json";
const RWA_RECEIPT_LIVE: &str = "target/novaseal-rwa-receipt-devnet-stateful-live.json";
const BTC_TX_COMMITMENT_LIVE: &str = "target/novaseal-btc-transaction-commitment-devnet-stateful-live.json";
const BTC_UTXO_SEAL_LIVE: &str = "target/novaseal-btc-utxo-seal-devnet-stateful-live.json";
const FIBER_CANDIDATE_LIVE: &str = "target/novaseal-fiber-candidate-devnet-stateful-live.json";
const STATEFUL_ACCEPTANCE: &str = "target/novaseal-devnet-stateful-acceptance.json";
const WALLET_VECTORS: &str = "target/novaseal-wallet-signing-vectors.json";
const TCB_REVIEW: &str = "target/novaseal-bip340-tcb-review.json";
const PUBLIC_CELLDEP_ATTESTATION: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json";
const EXTERNAL_TCB_ATTESTATION: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json";
const PUBLIC_CELLDEP_ATTESTATION_TEMPLATE: &str =
    "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json";
const EXTERNAL_TCB_ATTESTATION_TEMPLATE: &str =
    "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json";

const EXPECTED_NOVASEAL_CANONICAL_SCHEMA: &str = "NovaSealCanonicalV0";
const EXPECTED_NOVASEAL_CANONICAL_ENVELOPE: &str = "NovaSealCanonicalEnvelopeV0";
const EXPECTED_AGREEMENT_PROFILE: &str = "agreement-profile-v0";
const EXPECTED_FUNGIBLE_XUDT_PROFILE: &str = "fungible-xudt-profile-v0";
const EXPECTED_RWA_RECEIPT_PROFILE: &str = "rwa-receipt-profile-v0";
const EXPECTED_BTC_TX_COMMITMENT_PROFILE: &str = "btc-transaction-commitment-profile-v0";
const EXPECTED_BTC_UTXO_SEAL_PROFILE: &str = "btc-utxo-seal-profile-v0";
const EXPECTED_DUAL_SEAL_PROFILE: &str = "dual-seal-profile-v0";
const EXPECTED_FIBER_CANDIDATE_PROFILE: &str = "fiber-candidate-profile-v0";
const EXPECTED_AGREEMENT_CONFORMANCE_GATE: &str = "cellc certify --plugin novaseal-profile-v0";
const EXPECTED_PROFILE_CERTIFICATION_GATE: &str = "cellc certify --plugin novaseal-profile-v0";
const EXPECTED_CERTIFICATION_PLUGIN: &str = "novaseal-profile-v0";
const EXPECTED_CERTIFICATION_REPORT: &str = "target/cellscript-certification/novaseal-profile-v0.json";

const EXPECTED_VERIFIER: &[(&str, &str)] = &[
    ("name", "cellscript_btc_bip340_verifier_riscv"),
    ("role", "runtime_verifier"),
    ("verifier_id", "btc.bip340.v0"),
    ("ipc_abi", "cellscript-btc-bip340-ipc-v0"),
    ("dep_type", "code"),
    ("hash_type", "data1"),
];

const EXPECTED_CANONICAL_SCHEMA_FIELDS: &[(&str, &str)] = &[
    ("profile_id", "Byte32"),
    ("policy_hash", "Byte32"),
    ("action", "u8"),
    ("terminal_path", "u8"),
    ("subject_id", "Byte32"),
    ("old_state_commitment", "Byte32"),
    ("new_state_commitment", "Byte32"),
    ("old_nonce", "u64"),
    ("new_nonce", "u64"),
    ("expiry", "u64"),
    ("authority_hash", "Byte32"),
    ("profile_body_hash", "Byte32"),
    ("payout_commitment_hash", "Byte32"),
];

const EXPECTED_AGREEMENT_SCHEMA_FILES: &[&str] = &[
    "native_ckb_payout_v0.schema",
    "nova_agreement_cell_v0.schema",
    "nova_agreement_intent_v0.schema",
    "nova_agreement_receipt_v0.schema",
    "nova_agreement_terms_v0.schema",
    "nova_terminal_path_v0.schema",
];

const EXPECTED_FUNGIBLE_XUDT_SCHEMA_FILES: &[&str] =
    &["nova_fungible_xudt_cell_v0.schema", "nova_fungible_xudt_intent_v0.schema", "nova_fungible_xudt_receipt_v0.schema"];

const EXPECTED_RWA_RECEIPT_SCHEMA_FILES: &[&str] =
    &["nova_rwa_receipt_cell_v0.schema", "nova_rwa_receipt_event_v0.schema", "nova_rwa_receipt_intent_v0.schema"];

const EXPECTED_BTC_TX_COMMITMENT_SCHEMA_FILES: &[&str] = &[
    "nova_btc_transaction_commitment_cell_v0.schema",
    "nova_btc_transaction_commitment_intent_v0.schema",
    "nova_btc_transaction_commitment_receipt_v0.schema",
];

const EXPECTED_BTC_UTXO_SEAL_SCHEMA_FILES: &[&str] =
    &["nova_btc_utxo_seal_cell_v0.schema", "nova_btc_utxo_seal_intent_v0.schema", "nova_btc_utxo_seal_receipt_v0.schema"];

const EXPECTED_DUAL_SEAL_SCHEMA_FILES: &[&str] =
    &["nova_dual_seal_cell_v0.schema", "nova_dual_seal_intent_v0.schema", "nova_dual_seal_receipt_v0.schema"];

const EXPECTED_FIBER_CANDIDATE_SCHEMA_FILES: &[&str] =
    &["nova_fiber_candidate_cell_v0.schema", "nova_fiber_candidate_intent_v0.schema", "nova_fiber_candidate_receipt_v0.schema"];

const EXPECTED_CORE_FIXTURES: &[&str] = &[
    "keyauth_transfer_valid.json",
    "expired_intent_reject.json",
    "old_outpoint_index_mismatch_reject.json",
    "old_outpoint_tx_hash_mismatch_reject.json",
    "policy_hash_mismatch_reject.json",
    "receipt_hash_mismatch_reject.json",
    "replay_nonce_reject.json",
    "authority_hash_mapping_mismatch_reject.json",
    "authority_rotation_without_explicit_action_reject.json",
    "wrong_signature_reject.json",
    "wrong_pubkey_valid_signature_reject.json",
];

const EXPECTED_AGREEMENT_FIXTURES: &[&str] = &[
    "originate_valid.json",
    "repay_before_expiry_valid.json",
    "claim_after_expiry_valid.json",
    "wrong_originator_reject.json",
    "wrong_borrower_signature_reject.json",
    "wrong_lender_signature_reject.json",
    "wrong_party_reject.json",
    "non_ckb_asset_kind_reject.json",
    "under_capacity_reject.json",
    "payout_capacity_short_reject.json",
    "payout_lock_args_mismatch_reject.json",
    "wrong_settlement_amount_reject.json",
    "early_claim_reject.json",
    "expired_repay_reject.json",
    "nonce_mismatch_reject.json",
    "latest_receipt_hash_mismatch_reject.json",
    "receipt_hash_mismatch_reject.json",
    "preserved_field_mutation_reject.json",
    "wrong_terms_hash_reject.json",
    "repay_principal_max_fee_1_overflow_reject.json",
    "repay_principal_max_fee_0_accept.json",
    "nonce_max_increment_reject.json",
    "nonce_max_minus_1_increment_accept.json",
];

const EXPECTED_FUNGIBLE_XUDT_FIXTURES: &[&str] = &[
    "issue_valid.json",
    "transfer_valid.json",
    "settle_valid.json",
    "transfer_wrong_holder_signature_reject.json",
    "transfer_amount_mismatch_reject.json",
    "settle_wrong_holder_signature_reject.json",
];

const EXPECTED_FUNGIBLE_XUDT_DOCS: &[&str] = &["AUDIT_STATUS.md", "DEVNET_STATEFUL_ACCEPTANCE.md", "SECURITY.md"];

const EXPECTED_RWA_RECEIPT_FIXTURES: &[&str] = &[
    "materialize_valid.json",
    "claim_valid.json",
    "settle_valid.json",
    "wrong_holder_claim_reject.json",
    "wrong_issuer_settlement_reject.json",
    "amount_mutation_reject.json",
];

const EXPECTED_RWA_RECEIPT_DOCS: &[&str] = &["AUDIT_STATUS.md", "DEVNET_STATEFUL_ACCEPTANCE.md", "SECURITY.md"];

const EXPECTED_BTC_TX_COMMITMENT_FIXTURES: &[&str] = &[
    "commit_transaction_valid.json",
    "wrong_committer_signature_reject.json",
    "zero_btc_txid_reject.json",
    "transition_hash_mismatch_reject.json",
];

const EXPECTED_BTC_TX_COMMITMENT_DOCS: &[&str] = &["AUDIT_STATUS.md", "DEVNET_STATEFUL_ACCEPTANCE.md", "SECURITY.md"];

const EXPECTED_BTC_UTXO_SEAL_FIXTURES: &[&str] = &[
    "close_utxo_seal_valid.json",
    "wrong_owner_signature_reject.json",
    "utxo_commitment_mismatch_reject.json",
    "zero_spend_txid_reject.json",
];

const EXPECTED_BTC_UTXO_SEAL_DOCS: &[&str] = &["AUDIT_STATUS.md", "DEVNET_STATEFUL_ACCEPTANCE.md", "SECURITY.md"];

const EXPECTED_DUAL_SEAL_FIXTURES: &[&str] = &[
    "finalize_dual_seal_valid.json",
    "early_maturity_reject.json",
    "wrong_btc_owner_signature_reject.json",
    "wrong_ckb_authority_signature_reject.json",
];

const EXPECTED_DUAL_SEAL_DOCS: &[&str] = &["AUDIT_STATUS.md", "DEVNET_STATEFUL_ACCEPTANCE.md", "SECURITY.md"];

const EXPECTED_FIBER_CANDIDATE_FIXTURES: &[&str] =
    &["settle_fiber_candidate_valid.json", "wrong_operator_signature_reject.json", "balance_commitment_replay_reject.json"];

const EXPECTED_FIBER_CANDIDATE_DOCS: &[&str] = &["AUDIT_STATUS.md", "DEVNET_STATEFUL_ACCEPTANCE.md", "SECURITY.md"];

const EXPECTED_CERTIFICATION_INVARIANTS: &[&str] = &[
    "profile_separation",
    "ckb_native_only",
    "pre_expiry_repay",
    "post_expiry_claim",
    "party_terminal_rights",
    "receipt_materialized",
    "terms_hash_output_binding",
    "receipt_hash_output_binding",
    "native_capacity_settlement",
    "resolved_transaction_stack",
    "ckb_vm_capacity_settlement",
    "payout_cell_binding",
    "canonical_envelope_binding",
    "checked_financial_arithmetic",
    "authority-binding",
    "u64-overflow-prevention",
    "wallet_signing_vectors",
    "live_devnet_lifecycle",
];

const EXPECTED_FUNGIBLE_XUDT_INVARIANTS: &[&str] = &[
    "profile_separation",
    "canonical_envelope_binding",
    "issuer_only_issue",
    "holder_only_transfer",
    "amount_conservation",
    "settlement_terminal",
    "nonce_monotonicity",
    "live_devnet_lifecycle",
];

const EXPECTED_RWA_RECEIPT_INVARIANTS: &[&str] = &[
    "profile_separation",
    "canonical_envelope_binding",
    "issuer_only_materialization",
    "holder_only_claim",
    "dual_authority_settlement",
    "amount_conservation",
    "immutable_event_audit_trail",
    "nonce_monotonicity",
    "live_devnet_lifecycle",
];

const EXPECTED_BTC_TX_COMMITMENT_INVARIANTS: &[&str] = &[
    "profile_separation",
    "canonical_envelope_binding",
    "btc_public_tuple_binding",
    "non_zero_btc_transaction",
    "transition_commitment_binding",
    "committer_authority",
    "nonce_monotonicity",
    "live_devnet_lifecycle",
    "btc_public_verification",
];

const EXPECTED_BTC_UTXO_SEAL_INVARIANTS: &[&str] = &[
    "profile_separation",
    "canonical_envelope_binding",
    "sealed_utxo_tuple_binding",
    "single_use_closure",
    "spend_tuple_binding",
    "owner_authority",
    "nonce_monotonicity",
    "live_devnet_lifecycle",
    "btc_public_verification",
];

const EXPECTED_DUAL_SEAL_INVARIANTS: &[&str] = &[
    "profile_separation",
    "canonical_envelope_binding",
    "btc_closure_binding",
    "ckb_maturity_gate",
    "dual_authority",
    "single_use_finalization",
    "nonce_monotonicity",
    "live_devnet_lifecycle",
    "btc_public_verification",
    "ckb_finality_verification",
];

const EXPECTED_FIBER_CANDIDATE_INVARIANTS: &[&str] = &[
    "profile_separation",
    "canonical_envelope_binding",
    "candidate_settlement_binding",
    "operator_authority",
    "balance_commitment_progress",
    "nonce_monotonicity",
    "live_devnet_lifecycle",
    "fiber_execution",
];

const EXPECTED_LIVE_NEGATIVE_KEYS: &[&str] = &[
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
];

const REQUIRED_AGREEMENT_CORE_PATTERNS: &[(&str, &str)] = &[
    ("canonical_envelope", "struct NovaSealCanonicalEnvelopeV0"),
    ("canonical_envelope_hash", "canonical_envelope_hash"),
    ("canonical_profile_body_hash", "profile_body_hash"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("signed_typed_intent", "struct NovaAgreementSignedIntentV0"),
    ("expected_receipt_hash", "expected_receipt_hash"),
    ("receipt_commitment", "NovaAgreementReceiptCommitmentV0"),
    ("materialized_receipt", "NovaAgreementReceiptV0"),
    ("latest_receipt_hash", "latest_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("nonce_rule", "new_nonce == active.nonce + 1"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_repayment_sum", "active.fixed_fee_amount <= U64_MAX - active.principal_amount"),
    ("checked_terminal_nonce_increment", "active.nonce < U64_MAX"),
    ("checked_payout_capacity_sum", "repayment_amount <= U64_MAX - NATIVE_CKB_PAYOUT_OCCUPIED_CAPACITY"),
    ("expiry_rule", "expiry_timepoint"),
    ("payout_commitment", "payout_commitment_hash"),
];

const REQUIRED_FUNGIBLE_XUDT_SOURCE_PATTERNS: &[(&str, &str)] = &[
    ("canonical_envelope", "struct NovaSealCanonicalEnvelopeV0"),
    ("signed_typed_intent", "struct NovaFungibleXudtSignedIntentV0"),
    ("state_commitment", "NovaFungibleXudtStateCommitmentV0"),
    ("receipt_commitment", "NovaFungibleXudtReceiptCommitmentV0"),
    ("materialized_receipt", "NovaFungibleXudtReceiptV0"),
    ("issue_action", "action issue_xudt"),
    ("transfer_action", "action transfer_xudt"),
    ("settle_action", "action settle_xudt"),
    ("lifecycle_action", "action nova_fungible_xudt_lifecycle"),
    ("lifecycle_output_check", "source::group_output(0)"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("expected_receipt_hash", "intent.expected_receipt_hash == materialized_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_nonce_increment", "old_cell.nonce < U64_MAX"),
    ("amount_conservation", "intent.core.new_amount == old_cell.amount"),
    ("terminal_settlement", "intent.core.new_amount == 0"),
];

const REQUIRED_RWA_RECEIPT_SOURCE_PATTERNS: &[(&str, &str)] = &[
    ("canonical_envelope", "struct NovaSealCanonicalEnvelopeV0"),
    ("signed_typed_intent", "struct NovaRwaReceiptSignedIntentV0"),
    ("state_commitment", "NovaRwaReceiptStateCommitmentV0"),
    ("event_commitment", "NovaRwaReceiptEventCommitmentV0"),
    ("materialized_event", "NovaRwaReceiptEventV0"),
    ("materialize_action", "action materialize_rwa_receipt"),
    ("claim_action", "action claim_rwa_receipt"),
    ("settle_action", "action settle_rwa_receipt"),
    ("lifecycle_action", "action nova_rwa_receipt_lifecycle"),
    ("lifecycle_output_check", "source::group_output(0)"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("expected_receipt_hash", "intent.expected_receipt_hash == materialized_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_nonce_increment", "old_cell.nonce < U64_MAX"),
    ("amount_conservation", "intent.core.settlement_amount == old_cell.amount"),
    ("dual_authority_settlement", "issuer_sig.pubkey == old_cell.issuer_authority_hash"),
];

const REQUIRED_BTC_TX_COMMITMENT_SOURCE_PATTERNS: &[(&str, &str)] = &[
    ("canonical_envelope", "struct NovaSealCanonicalEnvelopeV0"),
    ("btc_public_tuple", "struct BtcTransactionPublicCommitmentV0"),
    ("signed_typed_intent", "struct NovaBtcTransactionCommitmentSignedIntentV0"),
    ("state_commitment", "NovaBtcTransactionCommitmentStateV0"),
    ("receipt_commitment", "NovaBtcTransactionCommitmentReceiptCommitmentV0"),
    ("materialized_receipt", "NovaBtcTransactionCommitmentReceiptV0"),
    ("commit_action", "action commit_btc_transaction_transition"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("expected_receipt_hash", "intent.expected_receipt_hash == materialized_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_nonce_increment", "old_cell.nonce < U64_MAX"),
    ("non_zero_btc_txid", "intent.core.btc_txid != Hash::zero()"),
    ("non_zero_btc_wtxid", "intent.core.btc_wtxid != Hash::zero()"),
    ("transition_commitment_binding", "intent.core.transition_commitment_hash == hash_blake2b(intent.core.new_state_hash)"),
];

const REQUIRED_BTC_UTXO_SEAL_SOURCE_PATTERNS: &[(&str, &str)] = &[
    ("canonical_envelope", "struct NovaSealCanonicalEnvelopeV0"),
    ("sealed_utxo_tuple", "struct BtcUtxoCommitmentV0"),
    ("closure_tuple", "struct BtcUtxoClosureCommitmentV0"),
    ("signed_typed_intent", "struct NovaBtcUtxoSealSignedIntentV0"),
    ("state_commitment", "NovaBtcUtxoSealStateV0"),
    ("receipt_commitment", "NovaBtcUtxoSealReceiptCommitmentV0"),
    ("materialized_receipt", "NovaBtcUtxoSealReceiptV0"),
    ("close_action", "action close_btc_utxo_seal"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("expected_receipt_hash", "intent.expected_receipt_hash == materialized_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_nonce_increment", "old_cell.nonce < U64_MAX"),
    ("utxo_commitment_binding", "old_cell.sealed_utxo_commitment_hash == sealed_utxo_commitment_hash"),
    ("single_use_consume", "consume old_cell"),
    ("non_zero_spend_txid", "intent.core.spend_txid != Hash::zero()"),
    ("non_zero_spend_wtxid", "intent.core.spend_wtxid != Hash::zero()"),
];

const REQUIRED_DUAL_SEAL_SOURCE_PATTERNS: &[(&str, &str)] = &[
    ("canonical_envelope", "struct NovaSealCanonicalEnvelopeV0"),
    ("finality_commitment", "struct DualSealFinalityCommitmentV0"),
    ("signed_typed_intent", "struct NovaDualSealSignedIntentV0"),
    ("state_commitment", "NovaDualSealStateV0"),
    ("receipt_commitment", "NovaDualSealReceiptCommitmentV0"),
    ("materialized_receipt", "NovaDualSealReceiptV0"),
    ("finalize_action", "action finalize_dual_seal"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("expected_receipt_hash", "intent.expected_receipt_hash == materialized_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_nonce_increment", "old_cell.nonce < U64_MAX"),
    ("ckb_maturity_gate", "now >= old_cell.maturity_timepoint"),
    ("btc_owner_authority", "btc_owner_sig.pubkey == old_cell.btc_owner_authority_hash"),
    ("ckb_authority", "ckb_sig.pubkey == old_cell.ckb_authority_hash"),
    ("single_use_consume", "consume old_cell"),
];

const REQUIRED_FIBER_CANDIDATE_SOURCE_PATTERNS: &[(&str, &str)] = &[
    ("canonical_envelope", "struct NovaSealCanonicalEnvelopeV0"),
    ("settlement_commitment", "struct FiberCandidateSettlementCommitmentV0"),
    ("signed_typed_intent", "struct NovaFiberCandidateSignedIntentV0"),
    ("state_commitment", "NovaFiberCandidateStateV0"),
    ("receipt_commitment", "NovaFiberCandidateReceiptCommitmentV0"),
    ("materialized_receipt", "NovaFiberCandidateReceiptV0"),
    ("settle_action", "action settle_fiber_candidate"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("expected_receipt_hash", "intent.expected_receipt_hash == materialized_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_nonce_increment", "old_cell.nonce < U64_MAX"),
    ("balance_progress", "intent.core.new_balance_commitment_hash != old_cell.balance_commitment_hash"),
    ("operator_authority", "operator_sig.pubkey == old_cell.operator_authority_hash"),
];

#[derive(Clone, Copy)]
struct ExpectedWalletAction {
    signers: &'static [&'static str],
    old_status: i64,
    new_status: i64,
    old_nonce: i64,
    new_nonce: i64,
}

const EXPECTED_AGREEMENT_WALLET_ACTIONS: &[(&str, ExpectedWalletAction)] = &[
    (
        "originate_agreement",
        ExpectedWalletAction { signers: &["borrower", "lender"], old_status: 0, new_status: 1, old_nonce: 0, new_nonce: 0 },
    ),
    ("repay_before_expiry", ExpectedWalletAction { signers: &["borrower"], old_status: 1, new_status: 2, old_nonce: 0, new_nonce: 1 }),
    ("claim_after_expiry", ExpectedWalletAction { signers: &["lender"], old_status: 1, new_status: 3, old_nonce: 0, new_nonce: 1 }),
];

pub(crate) fn build_report(repo_root: &Path) -> Result<Value> {
    let core_live = live_verifier_facts(repo_root, CORE_LIVE)?;
    let agreement_live = live_verifier_facts(repo_root, AGREEMENT_LIVE)?;
    let wallet = json_load(repo_root, WALLET_VECTORS)?;
    let tcb = json_load(repo_root, TCB_REVIEW)?;
    let artifact_hash = normalize_hex(json_pointer_str(&tcb, "/runtime_artifact/artifact_hash"));

    let core_manifest = compare_manifest_dep(repo_root, CORE_MANIFEST, &core_live, artifact_hash.as_deref())?;
    let agreement_manifest = compare_manifest_dep(repo_root, AGREEMENT_MANIFEST, &agreement_live, artifact_hash.as_deref())?;
    let public_attestation = validate_public_attestation(repo_root, PUBLIC_CELLDEP_ATTESTATION, artifact_hash.as_deref())?;
    let external_review = validate_external_review(repo_root, EXTERNAL_TCB_ATTESTATION, artifact_hash.as_deref())?;
    let core_security = validate_core_security_source(repo_root)?;
    let agreement_conformance = validate_agreement_profile_conformance(
        repo_root,
        &repo_root.join(CORE_MANIFEST),
        &repo_root.join(AGREEMENT_MANIFEST),
        &repo_root.join(AGREEMENT_ROOT),
    )?;
    let stateful_acceptance = build_stateful_acceptance_report(repo_root, &agreement_conformance)?;
    write_json_report(&repo_root.join(STATEFUL_ACCEPTANCE), &stateful_acceptance)?;
    let profile_certification = validate_profile_certification(ProfileCertificationInputs {
        repo_root,
        agreement_conformance: &agreement_conformance,
        agreement_manifest: &agreement_manifest,
        core_security: &core_security,
        wallet: &wallet,
        stateful_acceptance: &stateful_acceptance,
        tcb: &tcb,
        public_attestation: &public_attestation,
        external_review: &external_review,
    })?;

    let gates = vec![
        gate(
            "agreement_profile_conforms_to_novaseal_canonical_v0",
            json_pointer_str(&agreement_conformance, "/status").unwrap_or("failed"),
            "proposals/novaseal/v0-mvp-skeleton/Cell.toml + proposals/novaseal/v0-mvp-skeleton/schemas/nova_seal_canonical_envelope_v0.schema + proposals/novaseal/agreement-profile-v0/Cell.toml + proposals/novaseal/agreement-profile-v0/src",
            agreement_conformance.clone(),
        ),
        gate(
            "core_authority_binding_and_checked_arithmetic_source",
            json_pointer_str(&core_security, "/status").unwrap_or("failed"),
            "proposals/novaseal/v0-mvp-skeleton/src + proposals/novaseal/v0-mvp-skeleton/fixtures",
            core_security.clone(),
        ),
        gate(
            "agreement_profile_public_ecosystem_certification_v0",
            json_pointer_str(&profile_certification, "/status").unwrap_or("failed"),
            "proposals/novaseal/agreement-profile-v0/Cell.toml + proposals/novaseal/agreement-profile-v0/schemas + proposals/novaseal/agreement-profile-v0/fixtures + target/novaseal-devnet-stateful-acceptance.json + target/novaseal-wallet-signing-vectors.json",
            profile_certification.clone(),
        ),
        gate(
            "core_manifest_local_devnet_verifier_pin",
            if object_values_all_true(core_manifest.get("checks")) { "passed" } else { "failed" },
            CORE_MANIFEST,
            core_manifest.clone(),
        ),
        gate(
            "agreement_manifest_local_devnet_verifier_pin",
            if object_values_all_true(agreement_manifest.get("checks")) { "passed" } else { "failed" },
            AGREEMENT_MANIFEST,
            agreement_manifest.clone(),
        ),
        gate(
            "wallet_molecule_signing_vectors",
            if wallet_gate_passed(&wallet) { "passed" } else { "failed" },
            WALLET_VECTORS,
            wallet.get("summary").cloned().unwrap_or(Value::Null),
        ),
        gate(
            "bip340_runtime_verifier_local_tcb_review",
            if json_pointer_str(&tcb, "/status").is_some_and(|status| status.starts_with("passed_local_review")) {
                "passed"
            } else {
                "failed"
            },
            TCB_REVIEW,
            json!({
                "status": json_pointer_str(&tcb, "/status"),
                "artifact_hash": artifact_hash,
                "external_review_required": json_pointer_bool_opt(&tcb, "/external_review/required_for_production"),
            }),
        ),
        gate(
            "live_local_devnet_stateful_core_and_agreement",
            if stateful_acceptance_passed(&stateful_acceptance) { "passed" } else { "failed" },
            "target/novaseal-devnet-stateful-acceptance.json + target/novaseal-devnet-stateful-live.json + target/novaseal-agreement-devnet-stateful-live.json",
            json!({
                "acceptance": {
                    "status": json_pointer_str(&stateful_acceptance, "/status"),
                    "blocker_count": stateful_acceptance.get("blocker_count").and_then(Value::as_i64),
                    "live_devnet_rpc_executed": json_pointer_bool_opt(&stateful_acceptance, "/live_devnet_rpc_executed"),
                    "stateful_lifecycle_executed": json_pointer_bool_opt(&stateful_acceptance, "/stateful_lifecycle_executed"),
                    "missing": stateful_acceptance.get("missing"),
                },
                "core": core_live,
                "agreement": agreement_live,
            }),
        ),
        gate(
            "public_shared_cell_dep_pinning_attestation",
            json_pointer_str(&public_attestation, "/status").unwrap_or("failed"),
            PUBLIC_CELLDEP_ATTESTATION,
            public_attestation.clone(),
        ),
        gate(
            "external_bip340_runtime_verifier_tcb_review_attestation",
            json_pointer_str(&external_review, "/status").unwrap_or("failed"),
            EXTERNAL_TCB_ATTESTATION,
            external_review.clone(),
        ),
    ];

    let local_ready = gates
        .iter()
        .filter(|row| json_pointer_str(row, "/status") != Some("external_required"))
        .all(|row| json_pointer_str(row, "/status") == Some("passed"));
    let production_ready = gates.iter().all(|row| json_pointer_str(row, "/status") == Some("passed"));
    let status = if production_ready {
        "production_ready"
    } else if local_ready && gates.iter().any(|row| json_pointer_str(row, "/status") == Some("external_required")) {
        "local_production_prep_ready_external_attestation_required"
    } else {
        "failed"
    };
    let v1_readiness = build_v1_readiness(&profile_certification, &stateful_acceptance, &gates, local_ready, production_ready);

    Ok(json!({
        "schema": "novaseal-production-gates-v0.2",
        "status": status,
        "production_ready": production_ready,
        "local_production_prep_ready": local_ready,
        "runtime_artifact_hash": json_pointer_str(&tcb, "/runtime_artifact/artifact_hash").and_then(|value| normalize_hex(Some(value))),
        "conforms_to": {
            "agreement_profile": json_pointer_str(&agreement_conformance, "/conforms_to"),
            "expected": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
            "canonical_schema_hash": json_pointer_str(&agreement_conformance, "/canonical_schema_hash"),
            "status": json_pointer_str(&agreement_conformance, "/status"),
        },
        "profile_certification": profile_certification,
        "v1_readiness": v1_readiness,
        "gates": gates,
        "policy": {
            "no_placeholder_closure": "production remains false until public/shared CellDep and external TCB attestations are present",
            "attestation_templates": [
                "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json",
                "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json",
            ],
        },
        "generated_by": {
            "implementation": IMPLEMENTATION_ID,
            "language": "rust",
        },
    }))
}

fn build_v1_readiness(
    profile_certification: &Value,
    stateful_acceptance: &Value,
    gates: &[Value],
    local_ready: bool,
    production_ready: bool,
) -> Value {
    let gate_status = |name: &str| {
        gates
            .iter()
            .find(|gate| json_pointer_str(gate, "/name") == Some(name))
            .and_then(|gate| json_pointer_str(gate, "/status"))
            .unwrap_or("missing")
    };
    let planned_matrix = build_planned_profile_matrix(profile_certification, stateful_acceptance);
    let planned_matrix_passed = json_pointer_str(&planned_matrix, "/status") == Some("passed");
    let dimensions = vec![
        readiness_dimension(
            "architecture_and_profile_conformance",
            json_pointer_str(profile_certification, "/status") == Some("passed")
                && json_pointer_bool(profile_certification, "/local_checks/conformance_gate_passed"),
            "profile_certification.status + local_checks.conformance_gate_passed",
            "V1 architecture profile eligibility",
        ),
        readiness_dimension(
            "planned_profiles_and_business_scenarios",
            planned_matrix_passed,
            "v1_readiness.planned_profile_matrix",
            "all planned NovaSeal profiles and business scenarios",
        ),
        readiness_dimension(
            "security_audit_coverage",
            json_pointer_str(profile_certification, "/security_audit_coverage/status") == Some("passed"),
            "profile_certification.security_audit_coverage",
            "complete security-audit consideration",
        ),
        readiness_dimension(
            "devnet_multi_profile_coverage",
            json_pointer_str(stateful_acceptance, "/profile_coverage/status") == Some("passed"),
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage",
            "devnet multi-profile evidence",
        ),
        readiness_dimension(
            "multi_business_scenario_coverage",
            json_pointer_str(stateful_acceptance, "/business_scenario_coverage/status") == Some("passed"),
            "target/novaseal-devnet-stateful-acceptance.json#/business_scenario_coverage",
            "multi-business scenario evidence",
        ),
        readiness_dimension(
            "full_stateful_acceptance",
            stateful_acceptance_passed(stateful_acceptance),
            "target/novaseal-devnet-stateful-acceptance.json",
            "complete stateful acceptance",
        ),
        readiness_dimension(
            "wallet_signing_vectors",
            json_pointer_bool(profile_certification, "/local_checks/wallet_vector_detail_passed"),
            "target/novaseal-wallet-signing-vectors.json",
            "wallet-facing signing safety",
        ),
        readiness_dimension(
            "local_bip340_tcb_review",
            json_pointer_bool(profile_certification, "/local_checks/local_bip340_tcb_review_passed"),
            "target/novaseal-bip340-tcb-review.json",
            "local verifier TCB review",
        ),
        readiness_dimension(
            "local_v1_gate",
            local_ready,
            "all non-external novaseal-production-gates rows",
            "local V1 release readiness",
        ),
        readiness_dimension(
            "public_shared_cell_dep_attestation",
            gate_status("public_shared_cell_dep_pinning_attestation") == "passed",
            PUBLIC_CELLDEP_ATTESTATION,
            "public production deployment",
        ),
        readiness_dimension(
            "external_bip340_tcb_review_attestation",
            gate_status("external_bip340_runtime_verifier_tcb_review_attestation") == "passed",
            EXTERNAL_TCB_ATTESTATION,
            "external production TCB sign-off",
        ),
    ];
    let local_dimension_names = [
        "architecture_and_profile_conformance",
        "planned_profiles_and_business_scenarios",
        "security_audit_coverage",
        "devnet_multi_profile_coverage",
        "multi_business_scenario_coverage",
        "full_stateful_acceptance",
        "wallet_signing_vectors",
        "local_bip340_tcb_review",
        "local_v1_gate",
    ];
    let local_dimensions_passed = dimensions
        .iter()
        .filter(|dimension| json_pointer_str(dimension, "/name").is_some_and(|name| local_dimension_names.contains(&name)))
        .all(|dimension| json_pointer_str(dimension, "/status") == Some("passed"));
    let failed_dimensions = dimensions
        .iter()
        .filter(|dimension| json_pointer_str(dimension, "/status") != Some("passed"))
        .filter_map(|dimension| json_pointer_str(dimension, "/name").map(str::to_string))
        .collect::<Vec<_>>();
    let external_blockers =
        profile_certification.get("production_statement_blockers").cloned().unwrap_or_else(|| Value::Array(Vec::new()));
    let production_statement_eligible = json_pointer_bool(profile_certification, "/production_statement_eligible");
    let status = if production_ready && production_statement_eligible {
        "v1_prod_ready"
    } else if local_dimensions_passed {
        "local_v1_ready_external_attestation_required"
    } else if !planned_matrix_passed {
        "planned_profiles_incomplete"
    } else {
        "failed"
    };

    json!({
        "schema": "novaseal-v1-readiness-v0.1",
        "status": status,
        "local_v1_ready": local_dimensions_passed,
        "production_ready": production_ready,
        "production_statement_eligible": production_statement_eligible,
        "planned_profile_matrix": planned_matrix,
        "dimensions": dimensions,
        "failed_dimensions": failed_dimensions,
        "external_blockers": external_blockers,
        "acceptance_boundary": {
            "local_ready_means": "architecture, audit, wallet, TCB, multi-profile devnet, multi-business scenarios, and full stateful acceptance are machine checked locally",
            "production_ready_requires": [
                "public/shared CellDep pinning attestation",
                "external BIP340 runtime verifier TCB review attestation",
                "cellc certify --plugin novaseal-profile-v0 --require-production passes",
            ],
        },
    })
}

fn build_planned_profile_matrix(profile_certification: &Value, stateful_acceptance: &Value) -> Value {
    let core_passed = json_pointer_str(stateful_acceptance, "/profile_coverage/covered_profiles/0/status") == Some("passed");
    let agreement_passed = json_pointer_str(stateful_acceptance, "/profile_coverage/covered_profiles/1/status") == Some("passed")
        && json_pointer_bool(profile_certification, "/local_checks/conformance_gate_passed");
    let key_signature_passed = json_pointer_bool(profile_certification, "/local_checks/local_bip340_tcb_review_passed")
        && json_pointer_bool(profile_certification, "/local_checks/wallet_vector_detail_passed");
    let btc_tx_commitment_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/btc_tx_commitment/status") == Some("passed");
    let btc_utxo_seal_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/btc_utxo_seal/status") == Some("passed");
    let dual_seal_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/dual_seal/status") == Some("passed");
    let fiber_candidate_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/fiber_candidate/status") == Some("passed");
    let fungible_xudt_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/fungible_xudt/status") == Some("passed");
    let rwa_receipt_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/rwa_receipt/status") == Some("passed");
    let agreement_business_passed = [
        "agreement_originate_live",
        "agreement_repay_live",
        "agreement_claim_live",
        "agreement_negative_business_cases_preserve_live_state",
    ]
    .iter()
    .all(|key| json_pointer_bool(stateful_acceptance, &format!("/business_scenario_coverage/checks/{key}")));
    let btc_tx_commitment_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/btc_transaction_commitment_transition_live");
    let btc_utxo_seal_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/btc_utxo_seal_closure_live");
    let fungible_xudt_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/fungible_xudt_value_flow_live");
    let rwa_receipt_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/rwa_receipt_lifecycle_live");
    let fiber_candidate_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/fiber_candidate_path_live");
    let profiles = vec![
        planned_row(
            "seal_profile_btc_key_signature",
            "Seal profile",
            "BTC key signature authority over a typed CKB transition",
            key_signature_passed,
            "target/novaseal-bip340-tcb-review.json + target/novaseal-wallet-signing-vectors.json",
        ),
        planned_row(
            "seal_profile_btc_transaction_commitment",
            "Seal profile",
            "BTC transaction commitment to a transition",
            btc_tx_commitment_package_passed,
            "proposals/novaseal/btc-transaction-commitment-profile-v0 package, schemas, fixtures, docs, source action, invariant matrix, and explicit public-BTC proof gap",
        ),
        planned_row(
            "seal_profile_btc_utxo_seal",
            "Seal profile",
            "proved BTC UTXO spend as a single-use seal",
            btc_utxo_seal_package_passed,
            "proposals/novaseal/btc-utxo-seal-profile-v0 package, schemas, fixtures, docs, source action, invariant matrix, and explicit public-BTC spend proof gap",
        ),
        planned_row(
            "seal_profile_dual_seal",
            "Seal profile",
            "combined BTC UTXO closure and CKB transition maturity",
            dual_seal_package_passed,
            "proposals/novaseal/dual-seal-profile-v0 package, schemas, fixtures, docs, source action, invariant matrix, and explicit BTC/CKB finality evidence gaps",
        ),
        planned_row(
            "object_profile_key_signed_cell_movement",
            "Object profile",
            "key-signed Cell movement under NovaSealCanonicalV0",
            core_passed,
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage",
        ),
        planned_row(
            "object_profile_agreement",
            "Object profile",
            "CKB-native Agreement profile with deterministic terminal paths",
            agreement_passed,
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage + profile certification",
        ),
        planned_row(
            "object_profile_fungible_xudt",
            "Object profile",
            "Fungible/xUDT balance-bearing NovaSeal profile",
            fungible_xudt_package_passed,
            "proposals/novaseal/fungible-xudt-profile-v0 package, schemas, fixtures, docs, source actions, and invariant matrix",
        ),
        planned_row(
            "object_profile_rwa_receipt",
            "Object profile",
            "RWA/receipt object profile with materialised receipt lifecycle",
            rwa_receipt_package_passed,
            "proposals/novaseal/rwa-receipt-profile-v0 package, schemas, fixtures, docs, source actions, and invariant matrix",
        ),
        planned_row(
            "future_fiber_test_path",
            "Application profile",
            "Fiber-facing candidate test path",
            fiber_candidate_package_passed,
            "proposals/novaseal/fiber-candidate-profile-v0 package, schemas, fixtures, docs, source action, invariant matrix, and explicit live Fiber evidence gap",
        ),
    ];
    let business_scenarios = vec![
        planned_row(
            "core_bootstrap_to_key_authorised_transition",
            "Business scenario",
            "Core bootstrap followed by key-authorised state transition",
            core_passed,
            "target/novaseal-devnet-stateful-acceptance.json#/business_scenario_coverage",
        ),
        planned_row(
            "agreement_originate_repay_claim",
            "Business scenario",
            "Agreement originate, repay-before-expiry, claim-after-expiry, payout, receipt, and negative paths",
            agreement_passed && agreement_business_passed,
            "target/novaseal-devnet-stateful-acceptance.json#/business_scenario_coverage",
        ),
        planned_row(
            "btc_transaction_commitment_transition",
            "Business scenario",
            "Transition authorised by a public BTC transaction commitment",
            btc_tx_commitment_package_passed && btc_tx_commitment_business_passed,
            "target/novaseal-btc-transaction-commitment-devnet-stateful-live.json",
        ),
        planned_row(
            "btc_utxo_seal_closure",
            "Business scenario",
            "Single-use BTC UTXO seal closure over a CKB transition",
            btc_utxo_seal_package_passed && btc_utxo_seal_business_passed,
            "target/novaseal-btc-utxo-seal-devnet-stateful-live.json",
        ),
        planned_row(
            "fungible_xudt_value_flow",
            "Business scenario",
            "Fungible/xUDT issue, transfer, settlement, and negative accounting paths",
            fungible_xudt_package_passed && fungible_xudt_business_passed,
            "target/novaseal-fungible-xudt-devnet-stateful-live.json",
        ),
        planned_row(
            "rwa_receipt_lifecycle",
            "Business scenario",
            "RWA/receipt materialisation, claim, settlement, and negative paths",
            rwa_receipt_package_passed && rwa_receipt_business_passed,
            "target/novaseal-rwa-receipt-devnet-stateful-live.json",
        ),
        planned_row(
            "fiber_candidate_path",
            "Business scenario",
            "Fiber-compatible candidate settlement path",
            fiber_candidate_package_passed && fiber_candidate_business_passed,
            "target/novaseal-fiber-candidate-devnet-stateful-live.json",
        ),
    ];
    let missing_profiles = profiles
        .iter()
        .chain(business_scenarios.iter())
        .filter(|row| json_pointer_str(row, "/status") != Some("passed"))
        .filter_map(|row| json_pointer_str(row, "/id").map(str::to_string))
        .collect::<Vec<_>>();
    let passed = missing_profiles.is_empty();
    json!({
        "schema": "novaseal-planned-profile-matrix-v0.1",
        "status": if passed { "passed" } else { "incomplete" },
        "source": "proposals/novaseal/v0-mvp-skeleton/NOVASEAL_ARCHITECTURE_EXPLAINED.md",
        "profiles": profiles,
        "business_scenarios": business_scenarios,
        "missing": missing_profiles,
        "boundary": {
            "implemented_now": "BTC key-signature authority, planned profile package evidence, key-signed Cell movement, CKB-native Agreement terminal paths, and machine-readable live-report contracts for all remaining V1 business scenarios",
            "not_implemented_yet": "fresh live devnet reports proving BTC transaction commitment, BTC UTXO closure, Fungible/xUDT value-flow, RWA receipt lifecycle, and Fiber candidate execution",
        },
    })
}

fn planned_row(id: &str, category: &str, description: &str, passed: bool, evidence: &str) -> Value {
    json!({
        "id": id,
        "category": category,
        "description": description,
        "status": if passed { "passed" } else { "missing" },
        "evidence": evidence,
    })
}

fn readiness_dimension(name: &str, passed: bool, evidence: &str, required_for: &str) -> Value {
    json!({
        "name": name,
        "status": if passed { "passed" } else { "failed" },
        "evidence": evidence,
        "required_for": required_for,
    })
}

fn build_stateful_acceptance_report(repo_root: &Path, agreement_conformance: &Value) -> Result<Value> {
    let core_source = read_cell_sources(&repo_root.join(CORE_ROOT).join("src"))?;
    let agreement_source = read_cell_sources(&repo_root.join(AGREEMENT_ROOT).join("src"))?;
    let core_actions = find_actions(&core_source);
    let agreement_actions = find_actions(&agreement_source);
    let core_combined = json_load_path_optional(&repo_root.join(CORE_ROOT).join("target/novaseal-combined-tx-report.json"))?;
    let agreement_tx = json_load_path_optional(&repo_root.join(AGREEMENT_ROOT).join("target/nova-agreement-ckb-tx-report.json"))?;
    let live_core_report = json_load_path_optional(&repo_root.join(CORE_LIVE))?;
    let live_agreement_report = json_load_path_optional(&repo_root.join(AGREEMENT_LIVE))?;
    let live_fungible_xudt_report = json_load_path_optional(&repo_root.join(FUNGIBLE_XUDT_LIVE))?;
    let live_rwa_receipt_report = json_load_path_optional(&repo_root.join(RWA_RECEIPT_LIVE))?;
    let live_btc_tx_commitment_report = json_load_path_optional(&repo_root.join(BTC_TX_COMMITMENT_LIVE))?;
    let live_btc_utxo_seal_report = json_load_path_optional(&repo_root.join(BTC_UTXO_SEAL_LIVE))?;
    let live_fiber_candidate_report = json_load_path_optional(&repo_root.join(FIBER_CANDIDATE_LIVE))?;
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
            "scripts/novaseal_planned_profiles_devnet_stateful_live.py",
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
            "scripts/novaseal_planned_profiles_devnet_stateful_live.py",
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
            "scripts/novaseal_planned_profiles_devnet_stateful_live.py",
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
            "scripts/novaseal_planned_profiles_devnet_stateful_live.py",
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
    let live_fiber_candidate = live_planned_profile_summary(
        repo_root,
        live_fiber_candidate_report.as_ref(),
        &[
            FIBER_CANDIDATE_MANIFEST,
            "proposals/novaseal/fiber-candidate-profile-v0/src",
            "proposals/novaseal/fiber-candidate-profile-v0/schemas",
            VERIFIER_ROOT,
            "scripts/novaseal_planned_profiles_devnet_stateful_live.py",
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

    let core_live_passed = json_pointer_str(&live_core, "/status") == Some("passed")
        && json_pointer_bool(&live_core, "/live_devnet_rpc_executed")
        && json_pointer_bool(&live_core, "/stateful_lifecycle_executed")
        && json_pointer_bool(&live_core, "/provenance_freshness_matched")
        && json_pointer_bool(&live_core, "/old_state_not_live")
        && json_pointer_bool(&live_core, "/new_state_live")
        && json_pointer_bool(&live_core, "/receipt_live")
        && json_pointer_bool(&live_core, "/wrong_signature_rejected");
    let agreement_live_passed = json_pointer_str(&live_agreement, "/status") == Some("passed")
        && json_pointer_bool(&live_agreement, "/live_devnet_rpc_executed")
        && json_pointer_bool(&live_agreement, "/stateful_lifecycle_executed")
        && json_pointer_bool(&live_agreement, "/provenance_freshness_matched")
        && [
            "origin_active_live",
            "origin_principal_payout_live",
            "origin_receipt_live",
            "claim_origin_active_live",
            "claim_origin_principal_payout_live",
            "claim_origin_receipt_live",
            "repay_old_active_not_live",
            "repay_closed_live",
            "repay_lender_repayment_live",
            "repay_borrower_collateral_return_live",
            "repay_receipt_live",
            "claim_old_active_not_live",
            "claim_closed_live",
            "claim_lender_default_claim_live",
            "claim_receipt_live",
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
        .all(|key| json_pointer_bool(&live_agreement, &format!("/{key}")))
        && json_pointer_str(agreement_conformance, "/status") == Some("passed");
    let fungible_xudt_live_passed = json_pointer_bool(&live_fungible_xudt, "/required_live_checks_passed");
    let rwa_receipt_live_passed = json_pointer_bool(&live_rwa_receipt, "/required_live_checks_passed");
    let btc_tx_commitment_live_passed = json_pointer_bool(&live_btc_tx_commitment, "/required_live_checks_passed");
    let btc_utxo_seal_live_passed = json_pointer_bool(&live_btc_utxo_seal, "/required_live_checks_passed");
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
        "fiber_candidate_live_stateful": fiber_candidate_live_passed,
        "core_profile_actions_present": !core_actions.is_empty(),
        "agreement_profile_actions_present": agreement_profile_actions_present,
        "distinct_profiles_covered": core_live_passed
            && agreement_live_passed
            && fungible_xudt_live_passed
            && rwa_receipt_live_passed
            && btc_tx_commitment_live_passed
            && btc_utxo_seal_live_passed
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
        "fiber_candidate_path_live": fiber_candidate_live_passed,
    });
    let business_scenario_coverage_passed = object_values_all_true(Some(&business_scenario_checks));

    let mut core_blockers = Vec::new();
    if !has_core_bootstrap_surface(&core_source) {
        core_blockers.push(blocker(
            "NovaSeal core has key_auth_transition but no bootstrap/genesis/seed action that can create the first live NovaSealCellV0.",
            "creating an initial live state cell on devnet before the first transition",
        ));
    }
    if !has_dispatcher_surface(&core_source, &repo_root.join(CORE_ROOT)) {
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
            "name": "fiber_candidate_settlement",
            "status": if fiber_candidate_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": fiber_candidate_live_passed,
            "stateful_lifecycle_executed": fiber_candidate_live_passed,
            "actions": ["settle_fiber_candidate"],
            "blockers": [],
            "live_devnet_evidence": live_fiber_candidate,
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
            "Fiber-compatible candidate settlement with balance commitment progress",
        ],
        "checks": business_scenario_checks,
    });
    let all_blockers = scenarios
        .iter()
        .flat_map(|scenario| scenario.get("blockers").and_then(Value::as_array).into_iter().flatten().cloned())
        .collect::<Vec<_>>();
    let status = if !all_blockers.is_empty() {
        "blocked"
    } else if scenarios.iter().all(|scenario| json_pointer_str(scenario, "/status") == Some("passed"))
        && profile_coverage_passed
        && business_scenario_coverage_passed
    {
        "passed"
    } else if core_live_passed && !agreement_live_passed {
        "core_live_devnet_passed_agreement_pending"
    } else if agreement_live_passed && !core_live_passed {
        "agreement_live_devnet_passed_core_pending"
    } else {
        "ready_to_run_live_devnet"
    };

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
        "scenarios": scenarios,
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

#[derive(Clone)]
struct ActionSurface {
    name: String,
    params: String,
}

impl ActionSurface {
    fn consumes_resource(&self) -> bool {
        self.params.contains("NovaSealCellV0") || self.params.contains("NovaAgreementCellV0")
    }
}

fn find_actions(source: &str) -> Vec<ActionSurface> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let rest = line.strip_prefix("action ")?;
            let name_end = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))?;
            let name = rest[..name_end].to_string();
            let params_start = rest[name_end..].find('(')? + name_end + 1;
            let params_end = rest[params_start..].find(')')? + params_start;
            Some(ActionSurface { name, params: rest[params_start..params_end].to_string() })
        })
        .collect()
}

fn has_dispatcher_surface(source: &str, root: &Path) -> bool {
    let names = find_actions(source).into_iter().map(|action| action.name).collect::<BTreeSet<_>>();
    let manifest = std::fs::read_to_string(root.join("Cell.toml")).unwrap_or_default();
    names.iter().any(|name| ["dispatch", "dispatch_agreement", "novaseal_dispatch", "agreement_dispatch"].contains(&name.as_str()))
        || manifest.contains("stateful_dispatcher")
        || (manifest.contains("dispatcher") && manifest.contains("entry"))
}

fn has_core_bootstrap_surface(source: &str) -> bool {
    let actions = find_actions(source);
    if actions.iter().any(|action| action.name == "novaseal_lifecycle") && source.contains("OP_BOOTSTRAP") {
        return true;
    }
    actions.iter().any(|action| {
        let lowered = action.name.to_ascii_lowercase();
        ["bootstrap", "genesis", "seed", "initialize", "originate"].iter().any(|word| lowered.contains(word))
            && !action.consumes_resource()
    })
}

fn has_agreement_origination_surface(source: &str) -> bool {
    let actions = find_actions(source);
    if actions.iter().any(|action| action.name == "nova_agreement_lifecycle") && source.contains("PATH_ORIGINATE") {
        return true;
    }
    actions.iter().any(|action| action.name == "originate_agreement" && !action.consumes_resource())
}

fn live_planned_profile_summary(
    repo_root: &Path,
    report: Option<&Value>,
    source_paths: &[&str],
    tx_hashes: &[(&str, &str)],
    required_bools: &[(&str, &str)],
    negative_cases: &[(&str, &str)],
) -> Result<Value> {
    let expected_tx_hashes = tx_hashes.iter().map(|(name, pointer)| json!({"name": name, "pointer": pointer})).collect::<Vec<_>>();
    let required_live_checks =
        required_bools.iter().map(|(name, pointer)| json!({"name": name, "pointer": pointer})).collect::<Vec<_>>();
    let required_negative_cases = negative_cases.iter().map(|(name, key)| json!({"name": name, "key": key})).collect::<Vec<_>>();

    let Some(report) = report else {
        return Ok(json!({
            "present": false,
            "expected_tx_hashes": expected_tx_hashes,
            "required_live_checks": required_live_checks,
            "required_negative_cases": required_negative_cases,
            "required_live_checks_passed": false,
        }));
    };
    if report.get("_invalid_json").is_some() {
        return Ok(json!({
            "present": true,
            "valid_json": false,
            "error": report.get("_invalid_json"),
            "expected_tx_hashes": expected_tx_hashes,
            "required_live_checks": required_live_checks,
            "required_negative_cases": required_negative_cases,
            "required_live_checks_passed": false,
        }));
    }

    let provenance = provenance_summary(report, repo_root, source_paths)?;
    let mut tx_hash_summary = Map::new();
    for (name, pointer) in tx_hashes {
        tx_hash_summary.insert((*name).to_string(), json_pointer_str(report, pointer).map(Value::from).unwrap_or(Value::Null));
    }

    let mut live_checks = Map::new();
    for (name, pointer) in required_bools {
        live_checks.insert((*name).to_string(), Value::Bool(json_pointer_bool(report, pointer)));
    }

    let mut negative_checks = Map::new();
    for (name, key) in negative_cases {
        negative_checks.insert((*name).to_string(), negative_case_matched(report, key).map(Value::Bool).unwrap_or(Value::Null));
    }

    let status_passed = json_pointer_str(report, "/status") == Some("passed");
    let rpc_executed = json_pointer_bool(report, "/live_devnet_rpc_executed");
    let lifecycle_executed = json_pointer_bool(report, "/stateful_lifecycle_executed");
    let provenance_freshness_matched = json_pointer_bool(&provenance, "/freshness_matched");
    let tx_hashes_present = tx_hash_summary.values().all(value_is_present);
    let required_bools_passed = live_checks.values().all(|value| value.as_bool() == Some(true));
    let negative_cases_passed = negative_checks.values().all(|value| value.as_bool() == Some(true));
    let required_live_checks_passed = status_passed
        && rpc_executed
        && lifecycle_executed
        && provenance_freshness_matched
        && tx_hashes_present
        && required_bools_passed
        && negative_cases_passed;

    Ok(json!({
        "present": true,
        "valid_json": true,
        "status": json_pointer_str(report, "/status"),
        "live_devnet_rpc_executed": rpc_executed,
        "stateful_lifecycle_executed": lifecycle_executed,
        "provenance": provenance,
        "provenance_freshness_matched": provenance_freshness_matched,
        "expected_tx_hashes": expected_tx_hashes,
        "required_live_checks": required_live_checks,
        "required_negative_cases": required_negative_cases,
        "tx_hashes": tx_hash_summary,
        "live_checks": live_checks,
        "negative_cases": negative_checks,
        "required_live_checks_passed": required_live_checks_passed,
    }))
}

fn live_core_summary(repo_root: &Path, report: Option<&Value>) -> Result<Value> {
    let Some(report) = report else {
        return Ok(json!({"present": false}));
    };
    if report.get("_invalid_json").is_some() {
        return Ok(json!({"present": true, "valid_json": false, "error": report.get("_invalid_json")}));
    }
    let transition = report.get("transition").cloned().unwrap_or(Value::Null);
    let provenance = provenance_summary(
        report,
        repo_root,
        &[
            CORE_MANIFEST,
            "proposals/novaseal/v0-mvp-skeleton/src",
            "proposals/novaseal/v0-mvp-skeleton/schemas",
            VERIFIER_ROOT,
            "scripts/novaseal_devnet_stateful_live.py",
        ],
    )?;
    Ok(json!({
        "present": true,
        "valid_json": true,
        "status": json_pointer_str(report, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool(report, "/live_devnet_rpc_executed"),
        "stateful_lifecycle_executed": json_pointer_bool(report, "/stateful_lifecycle_executed"),
        "provenance": provenance,
        "provenance_freshness_matched": json_pointer_bool(&provenance, "/freshness_matched"),
        "bootstrap_tx_hash": json_pointer_str(report, "/bootstrap/commit/tx_hash"),
        "transition_tx_hash": json_pointer_str(&transition, "/commit/tx_hash"),
        "old_state_not_live": json_pointer_bool_opt(&transition, "/old_state_not_live"),
        "new_state_live": json_pointer_bool_opt(&transition, "/new_state_live"),
        "receipt_live": json_pointer_bool_opt(&transition, "/receipt_live"),
        "wrong_signature_rejected": negative_case_matched(report, "wrong_signature_dry_run"),
    }))
}

fn live_agreement_summary(repo_root: &Path, report: Option<&Value>) -> Result<Value> {
    let Some(report) = report else {
        return Ok(json!({"present": false}));
    };
    if report.get("_invalid_json").is_some() {
        return Ok(json!({"present": true, "valid_json": false, "error": report.get("_invalid_json")}));
    }
    let provenance = provenance_summary(
        report,
        repo_root,
        &[
            AGREEMENT_MANIFEST,
            "proposals/novaseal/agreement-profile-v0/src",
            "proposals/novaseal/agreement-profile-v0/schemas",
            VERIFIER_ROOT,
            "scripts/novaseal_agreement_devnet_stateful_live.py",
            "scripts/novaseal_devnet_stateful_live.py",
        ],
    )?;
    Ok(json!({
        "present": true,
        "valid_json": true,
        "status": json_pointer_str(report, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool(report, "/live_devnet_rpc_executed"),
        "stateful_lifecycle_executed": json_pointer_bool(report, "/stateful_lifecycle_executed"),
        "provenance": provenance,
        "provenance_freshness_matched": json_pointer_bool(&provenance, "/freshness_matched"),
        "originate_tx_hash": json_pointer_str(report, "/originate/commit/tx_hash"),
        "repay_tx_hash": json_pointer_str(report, "/repay/commit/tx_hash"),
        "claim_originate_tx_hash": json_pointer_str(report, "/claim_originate/commit/tx_hash"),
        "claim_tx_hash": json_pointer_str(report, "/claim/commit/tx_hash"),
        "origin_active_live": json_pointer_bool_opt(report, "/originate/active_live"),
        "origin_principal_payout_live": json_pointer_bool_opt(report, "/originate/principal_payout_live"),
        "origin_receipt_live": json_pointer_bool_opt(report, "/originate/receipt_live"),
        "claim_origin_active_live": json_pointer_bool_opt(report, "/claim_originate/active_live"),
        "claim_origin_principal_payout_live": json_pointer_bool_opt(report, "/claim_originate/principal_payout_live"),
        "claim_origin_receipt_live": json_pointer_bool_opt(report, "/claim_originate/receipt_live"),
        "repay_old_active_not_live": json_pointer_bool_opt(report, "/repay/old_active_not_live"),
        "repay_closed_live": json_pointer_bool_opt(report, "/repay/closed_live"),
        "repay_lender_repayment_live": json_pointer_bool_opt(report, "/repay/lender_repayment_live"),
        "repay_borrower_collateral_return_live": json_pointer_bool_opt(report, "/repay/borrower_collateral_return_live"),
        "repay_receipt_live": json_pointer_bool_opt(report, "/repay/receipt_live"),
        "claim_old_active_not_live": json_pointer_bool_opt(report, "/claim/old_active_not_live"),
        "claim_closed_live": json_pointer_bool_opt(report, "/claim/closed_live"),
        "claim_lender_default_claim_live": json_pointer_bool_opt(report, "/claim/lender_default_claim_live"),
        "claim_receipt_live": json_pointer_bool_opt(report, "/claim/receipt_live"),
        "wrong_lender_signature_rejected": negative_case_matched(report, "wrong_lender_signature_dry_run"),
        "non_ckb_asset_kind_rejected": negative_case_matched(report, "non_ckb_asset_kind_dry_run"),
        "wrong_borrower_signature_rejected": negative_case_matched(report, "wrong_borrower_signature_dry_run"),
        "repay_payout_capacity_short_rejected": negative_case_matched(report, "repay_payout_capacity_short_dry_run"),
        "repay_payout_lock_args_mismatch_rejected": negative_case_matched(report, "repay_payout_lock_args_mismatch_dry_run"),
        "repay_wrong_payout_amount_rejected": negative_case_matched(report, "repay_wrong_payout_amount_dry_run"),
        "early_claim_rejected": negative_case_matched(report, "early_claim_dry_run"),
        "wrong_lender_claim_signature_rejected": negative_case_matched(report, "wrong_lender_claim_signature_dry_run"),
        "post_negative_active_still_live": json_pointer_bool_opt(report, "/negative_cases/post_negative_active_still_live"),
        "post_claim_negative_active_still_live": json_pointer_bool_opt(report, "/negative_cases/post_claim_negative_active_still_live"),
    }))
}

fn provenance_summary(report: &Value, repo_root: &Path, source_paths: &[&str]) -> Result<Value> {
    let provenance = report.get("provenance").cloned().unwrap_or(Value::Null);
    let recorded_source = provenance.get("source_tree").cloned().unwrap_or(Value::Null);
    let current_source = source_tree_hash(repo_root, source_paths)?;
    let source_hash_matches = json_pointer_str(&recorded_source, "/sha256") == json_pointer_str(&current_source, "/sha256");
    let mut artifact_checks = Map::new();
    let recorded_artifacts = provenance.get("artifacts").cloned().unwrap_or(Value::Null);
    for name in ["verifier", "lifecycle"] {
        let artifact = recorded_artifacts.get(name).cloned().unwrap_or(Value::Null);
        let raw_path = json_pointer_str(&artifact, "/path");
        let path = raw_path.map(|value| {
            let path = Path::new(value);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                repo_root.join(path)
            }
        });
        let exists = path.as_ref().is_some_and(|path| path.is_file());
        let current_sha = path.as_ref().filter(|path| path.is_file()).map(|path| sha256_file_hex(path)).transpose()?;
        artifact_checks.insert(
            name.to_string(),
            json!({
                "present": artifact.is_object(),
                "path": raw_path,
                "exists": exists,
                "sha256_matches": current_sha.as_deref() == json_pointer_str(&artifact, "/sha256"),
                "recorded_sha256": json_pointer_str(&artifact, "/sha256"),
                "current_sha256": current_sha,
            }),
        );
    }
    let artifact_hashes_match = artifact_checks.values().all(|row| {
        json_pointer_bool(row, "/present") && json_pointer_bool(row, "/exists") && json_pointer_bool(row, "/sha256_matches")
    });
    let current_commit = git_commit(repo_root);
    Ok(json!({
        "present": provenance.is_object(),
        "freshness_matched": source_hash_matches && artifact_hashes_match,
        "repo_commit": json_pointer_str(&provenance, "/repo_commit"),
        "current_repo_commit": current_commit,
        "repo_commit_matches": json_pointer_str(&provenance, "/repo_commit") == current_commit.as_deref(),
        "source_hash_matches": source_hash_matches,
        "recorded_source_hash": json_pointer_str(&recorded_source, "/sha256"),
        "current_source_hash": json_pointer_str(&current_source, "/sha256"),
        "recorded_file_count": recorded_source.get("file_count").and_then(Value::as_u64),
        "current_file_count": current_source.get("file_count").and_then(Value::as_u64),
        "artifact_hashes_match": artifact_hashes_match,
        "artifacts": artifact_checks,
    }))
}

fn source_tree_hash(repo_root: &Path, paths: &[&str]) -> Result<Value> {
    let mut files = BTreeSet::new();
    for raw_path in paths {
        let path = repo_root.join(raw_path);
        if path.is_file() {
            files.insert(path);
        } else if path.is_dir() {
            collect_source_tree_files(&path, &path, &mut files)?;
        }
    }
    let mut hasher = Sha256::new();
    let mut rows = Vec::new();
    for path in files {
        let rel_path = rel(repo_root, &path);
        let digest = Sha256::digest(std::fs::read(&path)?);
        hasher.update(rel_path.as_bytes());
        hasher.update([0]);
        hasher.update(digest);
        rows.push(rel_path);
    }
    Ok(json!({
        "sha256": format!("0x{}", hex::encode(hasher.finalize())),
        "files": rows,
        "file_count": rows.len(),
    }))
}

fn collect_source_tree_files(root: &Path, path: &Path, files: &mut BTreeSet<std::path::PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        let relative_parts = child.strip_prefix(root).unwrap_or(&child).components().map(|part| part.as_os_str().to_string_lossy());
        if relative_parts.clone().any(|part| matches!(part.as_ref(), "target" | "build" | ".git" | "__pycache__")) {
            continue;
        }
        if child.is_dir() {
            collect_source_tree_files(root, &child, files)?;
        } else if child.is_file() && source_tree_file_allowed(&child) {
            files.insert(child);
        }
    }
    Ok(())
}

fn source_tree_file_allowed(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "Cargo.lock")
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "cell" | "schema" | "toml" | "py" | "json" | "rs"))
}

fn git_commit(repo_root: &Path) -> Option<String> {
    let output = std::process::Command::new("git").arg("rev-parse").arg("HEAD").current_dir(repo_root).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn negative_case_matched(report: &Value, key: &str) -> Option<bool> {
    let row = report.pointer(&format!("/negative_cases/{key}"))?;
    Some(json_pointer_str(row, "/status") == Some("rejected") && json_pointer_bool(row, "/matched_expected"))
}

fn summary_from_report(report: Option<&Value>, summary_keys: &[&str]) -> Value {
    let Some(report) = report else {
        return json!({"present": false});
    };
    if let Some(error) = report.get("_invalid_json") {
        return json!({"present": true, "valid_json": false, "error": error});
    }
    let Some(summary) = report.get("summary").and_then(Value::as_object) else {
        return json!({"present": true, "valid_json": true, "summary_present": false});
    };
    let mut out = Map::from_iter([
        ("present".to_string(), Value::Bool(true)),
        ("valid_json".to_string(), Value::Bool(true)),
        ("summary_present".to_string(), Value::Bool(true)),
    ]);
    for key in summary_keys {
        out.insert((*key).to_string(), summary.get(*key).cloned().unwrap_or(Value::Null));
    }
    Value::Object(out)
}

fn blocker(text: &str, required_for: &str) -> Value {
    json!({"blocker": text, "required_for": required_for})
}

fn validate_core_security_source(repo_root: &Path) -> Result<Value> {
    let source = read_cell_sources(&repo_root.join(CORE_ROOT).join("src"))?;
    let fixture_files = expected_files(repo_root, &repo_root.join(CORE_ROOT).join("fixtures"), EXPECTED_CORE_FIXTURES)?;
    let checks = json!({
        "fixture_set_exact": json_pointer_bool(&fixture_files, "/exact"),
        "wrong_pubkey_valid_signature_fixture_present": repo_root
            .join("proposals/novaseal/v0-mvp-skeleton/fixtures/wrong_pubkey_valid_signature_reject.json")
            .is_file(),
        "authority_hash_mapping_mismatch_fixture_present": repo_root
            .join("proposals/novaseal/v0-mvp-skeleton/fixtures/authority_hash_mapping_mismatch_reject.json")
            .is_file(),
        "authority_rotation_without_explicit_action_fixture_present": repo_root
            .join("proposals/novaseal/v0-mvp-skeleton/fixtures/authority_rotation_without_explicit_action_reject.json")
            .is_file(),
        "state_action_binds_sig_pubkey_to_old_cell_authority": source.contains("require sig.pubkey == old_cell.btc_authority_hash"),
        "lifecycle_binds_sig_pubkey_to_old_cell_authority": source.contains("assert(sig.pubkey == old_cell.btc_authority_hash"),
        "lock_binds_sig_pubkey_to_cell_authority_in_both_lock_surfaces": source.matches("require sig.pubkey == cell.btc_authority_hash").count() >= 2,
        "core_nonce_increment_guarded": source.contains("require old_cell.nonce < U64_MAX")
            && source.contains("assert(old_cell.nonce < U64_MAX"),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "fixture_files": fixture_files,
        "security_boundary": "BIP340 verification is only authority-enforcing when the verified x-only pubkey is bound to the Cell-declared authority.",
    }))
}

pub(crate) fn validate_agreement_profile_conformance(
    repo_root: &Path,
    core_manifest_path: &Path,
    agreement_manifest_path: &Path,
    agreement_root: &Path,
) -> Result<Value> {
    let core_metadata = manifest_metadata(core_manifest_path)?;
    let agreement_metadata = manifest_metadata(agreement_manifest_path)?;
    let agreement_source = read_cell_sources(&agreement_root.join("src"))?;
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let schema_checks = canonical_schema_checks(&schema_path)?;
    let source_checks = REQUIRED_AGREEMENT_CORE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(agreement_source.contains(pattern))))
        .collect::<Map<_, _>>();

    let mut checks = schema_checks;
    checks.extend([
        (
            "core_declares_canonical_schema".to_string(),
            Value::Bool(toml_str(&core_metadata, "canonical_schema") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "core_canonical_schema_hash".to_string(),
            Value::Bool(toml_str(&core_metadata, "canonical_schema_hash") == schema_hash.as_deref()),
        ),
        ("core_package_role".to_string(), Value::Bool(toml_str(&core_metadata, "package_role") == Some("canonical-example"))),
        ("core_protocol_family".to_string(), Value::Bool(toml_str(&core_metadata, "protocol_family") == Some("NovaSeal"))),
        ("profile_protocol_family".to_string(), Value::Bool(toml_str(&agreement_metadata, "protocol_family") == Some("NovaSeal"))),
        ("profile_name".to_string(), Value::Bool(toml_str(&agreement_metadata, "profile") == Some(EXPECTED_AGREEMENT_PROFILE))),
        (
            "profile_conforms_to".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "profile_canonical_schema_hash".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "canonical_schema_hash") == schema_hash.as_deref()),
        ),
        (
            "profile_conformance_gate".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "conformance_gate") == Some(EXPECTED_AGREEMENT_CONFORMANCE_GATE)),
        ),
        (
            "profile_certification_plugin".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "profile_certification_report".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "certification_report") == Some(EXPECTED_CERTIFICATION_REPORT)),
        ),
    ]);
    checks.extend(source_checks);

    let source_patterns = REQUIRED_AGREEMENT_CORE_PATTERNS
        .iter()
        .map(|(name, pattern)| ((*name).to_string(), Value::String((*pattern).to_string())))
        .collect::<Map<_, _>>();
    Ok(json!({
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "conforms_to": toml_str(&agreement_metadata, "conforms_to"),
        "expected_conforms_to": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
        "canonical_schema": toml_str(&core_metadata, "canonical_schema"),
        "canonical_schema_file": rel(repo_root, &schema_path),
        "canonical_schema_hash": schema_hash,
        "canonical_schema_hash_algorithm": "sha256(normalized schema lines: comments/blank lines ignored, whitespace collapsed)",
        "canonical_schema_lines": canonical_schema_lines(&schema_path)?,
        "core_manifest": rel(repo_root, core_manifest_path),
        "profile_manifest": rel(repo_root, agreement_manifest_path),
        "checks": checks,
        "manifest": {
            "canonical_schema": toml_str(&core_metadata, "canonical_schema"),
            "canonical_schema_hash": toml_str(&core_metadata, "canonical_schema_hash"),
            "package_role": toml_str(&core_metadata, "package_role"),
            "core_protocol_family": toml_str(&core_metadata, "protocol_family"),
            "profile": toml_str(&agreement_metadata, "profile"),
            "protocol_family": toml_str(&agreement_metadata, "protocol_family"),
            "conforms_to": toml_str(&agreement_metadata, "conforms_to"),
            "profile_canonical_schema_hash": toml_str(&agreement_metadata, "canonical_schema_hash"),
            "conformance_gate": toml_str(&agreement_metadata, "conformance_gate"),
        },
        "source_patterns": source_patterns,
    }))
}

struct ProfileCertificationInputs<'a> {
    repo_root: &'a Path,
    agreement_conformance: &'a Value,
    agreement_manifest: &'a Value,
    core_security: &'a Value,
    wallet: &'a Value,
    stateful_acceptance: &'a Value,
    tcb: &'a Value,
    public_attestation: &'a Value,
    external_review: &'a Value,
}

fn validate_profile_certification(input: ProfileCertificationInputs<'_>) -> Result<Value> {
    let ProfileCertificationInputs {
        repo_root,
        agreement_conformance,
        agreement_manifest,
        core_security,
        wallet,
        stateful_acceptance,
        tcb,
        public_attestation,
        external_review,
    } = input;
    let schema_files = expected_files(repo_root, &repo_root.join(AGREEMENT_ROOT).join("schemas"), EXPECTED_AGREEMENT_SCHEMA_FILES)?;
    let fixture_files = expected_files(repo_root, &repo_root.join(AGREEMENT_ROOT).join("fixtures"), EXPECTED_AGREEMENT_FIXTURES)?;
    let wallet_detail = validate_wallet_vector_detail(wallet);
    let invariant_matrix = validate_invariant_matrix(repo_root, &repo_root.join(AGREEMENT_ROOT).join("proofs/invariant_matrix.json"))?;
    let fungible_xudt_profile = validate_fungible_xudt_profile_package(repo_root)?;
    let rwa_receipt_profile = validate_rwa_receipt_profile_package(repo_root)?;
    let btc_tx_commitment_profile = validate_btc_tx_commitment_profile_package(repo_root)?;
    let btc_utxo_seal_profile = validate_btc_utxo_seal_profile_package(repo_root)?;
    let dual_seal_profile = validate_dual_seal_profile_package(repo_root)?;
    let fiber_candidate_profile = validate_fiber_candidate_profile_package(repo_root)?;
    let live_evidence = agreement_live_evidence(stateful_acceptance);
    let artifact_hash = normalize_hex(json_pointer_str(tcb, "/runtime_artifact/artifact_hash"));
    let source_tree_hash = normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256"));
    let attestation_templates = validate_attestation_templates(repo_root, artifact_hash.as_deref(), source_tree_hash.as_deref())?;
    let security_audit_coverage =
        validate_security_audit_coverage(repo_root, core_security, &invariant_matrix, &live_evidence, tcb, &attestation_templates)?;
    let docs = json!({
        "agreement_profile": repo_root.join(AGREEMENT_ROOT).join("docs/AGREEMENT_PROFILE.md").is_file(),
        "security": repo_root.join(AGREEMENT_ROOT).join("docs/SECURITY.md").is_file(),
        "audit_status": repo_root.join(AGREEMENT_ROOT).join("docs/AUDIT_STATUS.md").is_file(),
        "devnet_acceptance": repo_root.join(AGREEMENT_ROOT).join("docs/DEVNET_STATEFUL_ACCEPTANCE.md").is_file(),
    });
    let external_checks = json!({
        "public_shared_cell_dep_attested": json_pointer_str(public_attestation, "/status") == Some("passed"),
        "external_bip340_tcb_review_attested": json_pointer_str(external_review, "/status") == Some("passed"),
    });
    let local_checks = json!({
        "conformance_gate_passed": json_pointer_str(agreement_conformance, "/status") == Some("passed"),
        "profile_schema_set_exact": json_pointer_bool(&schema_files, "/exact"),
        "profile_fixture_set_exact": json_pointer_bool(&fixture_files, "/exact"),
        "wallet_vector_detail_passed": json_pointer_str(&wallet_detail, "/status") == Some("passed"),
        "invariant_matrix_passed": json_pointer_str(&invariant_matrix, "/status") == Some("passed"),
        "live_devnet_evidence_passed": json_pointer_str(&live_evidence, "/status") == Some("passed"),
        "agreement_runtime_verifier_pin_passed": object_values_all_true(agreement_manifest.get("checks")),
        "local_bip340_tcb_review_passed": json_pointer_str(tcb, "/status").is_some_and(|status| status.starts_with("passed_local_review")),
        "external_attestation_templates_current": json_pointer_str(&attestation_templates, "/status") == Some("passed"),
        "security_audit_coverage_passed": json_pointer_str(&security_audit_coverage, "/status") == Some("passed"),
        "fungible_xudt_profile_package_passed": json_pointer_str(&fungible_xudt_profile, "/status") == Some("passed"),
        "rwa_receipt_profile_package_passed": json_pointer_str(&rwa_receipt_profile, "/status") == Some("passed"),
        "btc_tx_commitment_profile_package_passed": json_pointer_str(&btc_tx_commitment_profile, "/status") == Some("passed"),
        "btc_utxo_seal_profile_package_passed": json_pointer_str(&btc_utxo_seal_profile, "/status") == Some("passed"),
        "dual_seal_profile_package_passed": json_pointer_str(&dual_seal_profile, "/status") == Some("passed"),
        "fiber_candidate_profile_package_passed": json_pointer_str(&fiber_candidate_profile, "/status") == Some("passed"),
        "required_docs_present": object_values_all_true(Some(&docs)),
    });
    let local_passed = object_values_all_true(Some(&local_checks));
    let production_statement_eligible = local_passed && object_values_all_true(Some(&external_checks));
    let production_statement_blockers = external_checks
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(_, passed)| passed.as_bool() != Some(true))
        .map(|(name, _)| Value::String(name.clone()))
        .collect::<Vec<_>>();

    Ok(json!({
        "schema": "novaseal-profile-certification-v0.1",
        "profile": EXPECTED_AGREEMENT_PROFILE,
        "conforms_to": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
        "gate": EXPECTED_PROFILE_CERTIFICATION_GATE,
        "status": if local_passed { "passed" } else { "failed" },
        "certification_level": if local_passed {
            "public_ecosystem_profile_certification_local_ready"
        } else {
            "public_ecosystem_profile_certification_failed"
        },
        "production_statement_eligible": production_statement_eligible,
        "production_statement_blockers": production_statement_blockers,
        "local_checks": local_checks,
        "external_checks": external_checks,
        "schema_files": schema_files,
        "fixture_files": fixture_files,
        "wallet_vectors": wallet_detail,
        "invariant_matrix": invariant_matrix,
        "planned_profile_packages": {
            "btc_tx_commitment": btc_tx_commitment_profile,
            "btc_utxo_seal": btc_utxo_seal_profile,
            "dual_seal": dual_seal_profile,
            "fiber_candidate": fiber_candidate_profile,
            "fungible_xudt": fungible_xudt_profile,
            "rwa_receipt": rwa_receipt_profile,
        },
        "live_devnet": live_evidence,
        "attestation_templates": attestation_templates,
        "security_audit_coverage": security_audit_coverage,
        "docs": docs,
        "design_boundary": {
            "agreement_calls_core_runtime": false,
            "canonical_constraint": "manifest canonical_schema_hash + signed canonical_envelope_hash + runtime recomputation",
            "rgb_code_vendored": false,
            "rgbplusplus_schema_dependency": false,
            "new_runtime_machinery_added": false,
        },
    }))
}

fn validate_wallet_vector_detail(wallet: &Value) -> Value {
    let vectors = wallet.get("vectors").and_then(Value::as_array).cloned().unwrap_or_default();
    let agreement_vectors = vectors
        .iter()
        .filter(|vector| json_pointer_str(vector, "/suite") == Some("novaseal-agreement-profile-v0"))
        .cloned()
        .collect::<Vec<_>>();
    let mut by_action: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for vector in &agreement_vectors {
        if let Some(action) = json_pointer_str(vector, "/action") {
            by_action.entry(action.to_string()).or_default().push(vector.clone());
        }
    }

    let mut action_checks = Map::new();
    for (action, expected) in EXPECTED_AGREEMENT_WALLET_ACTIONS {
        let matches = by_action.get(*action).cloned().unwrap_or_default();
        let vector = matches.first().cloned().unwrap_or(Value::Null);
        let display = vector.get("wallet_display").cloned().unwrap_or(Value::Null);
        let packed = json_pointer_str(&vector, "/signed_intent_packed_hex");
        let byte_len = packed.and_then(|value| is_hex_bytes(value).then_some((value.len() - 2) / 2));
        action_checks.insert(
            (*action).to_string(),
            json!({
                "exactly_one_vector": matches.len() == 1,
                "status_passed": json_pointer_str(&vector, "/status") == Some("passed"),
                "signed_type": json_pointer_str(&vector, "/signed_type") == Some("NovaAgreementSignedIntentV0"),
                "fixed_width_signed_intent_259_bytes": byte_len == Some(259),
                "bip340_message_hash": json_pointer_str(&vector, "/bip340_message_hash").is_some_and(is_hex32),
                "expected_receipt_hash": json_pointer_str(&vector, "/expected_receipt_hash").is_some_and(is_hex32),
                "canonical_envelope_hash_displayed": json_pointer_str(&display, "/canonical_envelope_hash").is_some_and(is_hex32),
                "payout_commitment_hash_displayed": json_pointer_str(&display, "/payout_commitment_hash").is_some_and(is_hex32),
                "agreement_id_displayed": json_pointer_str(&display, "/agreement_id").is_some_and(is_hex32),
                "terms_hash_displayed": json_pointer_str(&display, "/terms_hash").is_some_and(is_hex32),
                "borrower_authority_displayed": json_pointer_str(&display, "/borrower_authority_hash").is_some_and(is_hex32),
                "lender_authority_displayed": json_pointer_str(&display, "/lender_authority_hash").is_some_and(is_hex32),
                "signers_match": json_array_strings(&vector, "/signers") == expected.signers,
                "status_transition_match": json_pointer_i64(&display, "/old_status") == Some(expected.old_status)
                    && json_pointer_i64(&display, "/new_status") == Some(expected.new_status),
                "nonce_transition_match": json_pointer_i64(&display, "/old_nonce") == Some(expected.old_nonce)
                    && json_pointer_i64(&display, "/new_nonce") == Some(expected.new_nonce),
                "terminal_amount_positive": json_pointer_i64(&display, "/terminal_amount_shannons").is_some_and(|amount| amount > 0),
            }),
        );
    }

    let actions_present = by_action.keys().cloned().collect::<BTreeSet<_>>();
    let expected_actions = EXPECTED_AGREEMENT_WALLET_ACTIONS.iter().map(|(name, _)| (*name).to_string()).collect::<BTreeSet<_>>();
    let checks = json!({
        "wallet_report_passed": json_pointer_str(wallet, "/status") == Some("passed"),
        "summary_counts_match": json_pointer_i64(wallet, "/summary/agreement_vectors") == Some(3)
            && json_pointer_i64(wallet, "/summary/core_vectors").unwrap_or_default() >= 6
            && json_pointer_i64(wallet, "/summary/matched") == json_pointer_i64(wallet, "/summary/total"),
        "exact_agreement_actions": actions_present == expected_actions,
        "agreement_action_details": action_checks.values().all(|row| object_values_all_true(Some(row))),
    });
    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "actions": action_checks,
        "expected_actions": expected_actions.into_iter().collect::<Vec<_>>(),
        "agreement_vector_count": agreement_vectors.len(),
    })
}

fn validate_invariant_matrix(repo_root: &Path, path: &Path) -> Result<Value> {
    let payload = json_load_path(repo_root, path)?;
    let invariants = payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required = EXPECTED_CERTIFICATION_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let checks = json!({
        "file_present": payload.get("missing").is_none(),
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-agreement-invariant-matrix-v0.1"),
        "required_invariants_present": required.is_subset(&ids),
        "no_empty_coverage": ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present)),
    });
    let missing = required.difference(&ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "required": required.into_iter().collect::<Vec<_>>(),
        "present": ids.into_iter().collect::<Vec<_>>(),
        "missing": missing,
        "coverage_by_id": coverage_by_id,
    }))
}

fn validate_fungible_xudt_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(FUNGIBLE_XUDT_ROOT);
    let manifest_path = repo_root.join(FUNGIBLE_XUDT_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_FUNGIBLE_XUDT_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["issue_xudt", "transfer_xudt", "settle_xudt", "nova_fungible_xudt_lifecycle"]
        .iter()
        .map(|action| (*action).to_string())
        .collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_FUNGIBLE_XUDT_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_FUNGIBLE_XUDT_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_FUNGIBLE_XUDT_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_FUNGIBLE_XUDT_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        (
            "manifest_protocol_family".to_string(),
            Value::Bool(metadata_str("protocol_family") == Some("NovaSeal")),
        ),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_FUNGIBLE_XUDT_PROFILE))),
        (
            "manifest_conforms_to".to_string(),
            Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "manifest_canonical_schema_hash".to_string(),
            Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref()),
        ),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(
                metadata_str("stateful_dispatcher")
                    == Some("src/nova_fungible_xudt_lifecycle_type.cell:nova_fungible_xudt_lifecycle"),
            ),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some(
                        "src/nova_fungible_xudt_type.cell:issue_xudt;src/nova_fungible_xudt_type.cell:transfer_xudt;src/nova_fungible_xudt_type.cell:settle_xudt;src/nova_fungible_xudt_lifecycle_type.cell:nova_fungible_xudt_lifecycle",
                    ),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-fungible-xudt-invariant-matrix-v0.1")),
        ),
        (
            "required_invariants_present".to_string(),
            Value::Bool(required_invariants.is_subset(&invariant_ids)),
        ),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str) == Some("missing-live-devnet-evidence")),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-fungible-xudt-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-with-compiled-lifecycle-dispatcher-not-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "live devnet issue -> transfer -> settle transactions are still required before fungible_xudt_value_flow can pass",
    }))
}

fn validate_rwa_receipt_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(RWA_RECEIPT_ROOT);
    let manifest_path = repo_root.join(RWA_RECEIPT_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_RWA_RECEIPT_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["materialize_rwa_receipt", "claim_rwa_receipt", "settle_rwa_receipt", "nova_rwa_receipt_lifecycle"]
        .iter()
        .map(|action| (*action).to_string())
        .collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_RWA_RECEIPT_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_RWA_RECEIPT_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_RWA_RECEIPT_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_RWA_RECEIPT_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        (
            "manifest_protocol_family".to_string(),
            Value::Bool(metadata_str("protocol_family") == Some("NovaSeal")),
        ),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_RWA_RECEIPT_PROFILE))),
        (
            "manifest_conforms_to".to_string(),
            Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "manifest_canonical_schema_hash".to_string(),
            Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref()),
        ),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(
                metadata_str("stateful_dispatcher") == Some("src/nova_rwa_receipt_lifecycle_type.cell:nova_rwa_receipt_lifecycle"),
            ),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some(
                        "src/nova_rwa_receipt_type.cell:materialize_rwa_receipt;src/nova_rwa_receipt_type.cell:claim_rwa_receipt;src/nova_rwa_receipt_type.cell:settle_rwa_receipt;src/nova_rwa_receipt_lifecycle_type.cell:nova_rwa_receipt_lifecycle",
                    ),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-rwa-receipt-invariant-matrix-v0.1")),
        ),
        (
            "required_invariants_present".to_string(),
            Value::Bool(required_invariants.is_subset(&invariant_ids)),
        ),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str) == Some("missing-live-devnet-evidence")),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-rwa-receipt-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-with-compiled-lifecycle-dispatcher-not-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "live devnet materialise -> claim -> settle transactions are still required before rwa_receipt_lifecycle can pass",
    }))
}

fn validate_btc_tx_commitment_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(BTC_TX_COMMITMENT_ROOT);
    let manifest_path = repo_root.join(BTC_TX_COMMITMENT_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_BTC_TX_COMMITMENT_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["commit_btc_transaction_transition"].iter().map(|action| (*action).to_string()).collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_BTC_TX_COMMITMENT_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_BTC_TX_COMMITMENT_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_BTC_TX_COMMITMENT_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_BTC_TX_COMMITMENT_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        ("manifest_protocol_family".to_string(), Value::Bool(metadata_str("protocol_family") == Some("NovaSeal"))),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_BTC_TX_COMMITMENT_PROFILE))),
        ("manifest_conforms_to".to_string(), Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA))),
        ("manifest_canonical_schema_hash".to_string(), Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref())),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(metadata_str("stateful_dispatcher") == Some("missing-live-dispatcher")),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(metadata_str("btc_public_verification") == Some("missing-spv-or-indexer-evidence")),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some("src/nova_btc_transaction_commitment_type.cell:commit_btc_transaction_transition"),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(
                json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-btc-transaction-commitment-invariant-matrix-v0.1"),
            ),
        ),
        ("required_invariants_present".to_string(), Value::Bool(required_invariants.is_subset(&invariant_ids))),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str) == Some("missing-live-devnet-evidence")),
        ),
        (
            "btc_public_verification_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("btc_public_verification").and_then(Value::as_str) == Some("missing-spv-or-indexer-evidence"),
            ),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-btc-transaction-commitment-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-evidence-not-btc-finality-or-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "live devnet BTC transaction commitment transition and public BTC verification evidence are still required before btc_transaction_commitment_transition can pass",
    }))
}

fn validate_btc_utxo_seal_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(BTC_UTXO_SEAL_ROOT);
    let manifest_path = repo_root.join(BTC_UTXO_SEAL_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_BTC_UTXO_SEAL_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["close_btc_utxo_seal"].iter().map(|action| (*action).to_string()).collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_BTC_UTXO_SEAL_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_BTC_UTXO_SEAL_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_BTC_UTXO_SEAL_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_BTC_UTXO_SEAL_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        ("manifest_protocol_family".to_string(), Value::Bool(metadata_str("protocol_family") == Some("NovaSeal"))),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_BTC_UTXO_SEAL_PROFILE))),
        ("manifest_conforms_to".to_string(), Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA))),
        ("manifest_canonical_schema_hash".to_string(), Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref())),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(metadata_str("stateful_dispatcher") == Some("missing-live-dispatcher")),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(metadata_str("btc_public_verification") == Some("missing-spv-or-indexer-evidence")),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(metadata_str("source_actions") == Some("src/nova_btc_utxo_seal_type.cell:close_btc_utxo_seal")),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-btc-utxo-seal-invariant-matrix-v0.1")),
        ),
        ("required_invariants_present".to_string(), Value::Bool(required_invariants.is_subset(&invariant_ids))),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str) == Some("missing-live-devnet-evidence")),
        ),
        (
            "btc_public_verification_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("btc_public_verification").and_then(Value::as_str) == Some("missing-spv-or-indexer-evidence"),
            ),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-btc-utxo-seal-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-evidence-not-btc-spend-proof-or-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "live devnet BTC UTXO seal closure and public BTC spend-verification evidence are still required before btc_utxo_seal_closure can pass",
    }))
}

fn validate_dual_seal_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(DUAL_SEAL_ROOT);
    let manifest_path = repo_root.join(DUAL_SEAL_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_DUAL_SEAL_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["finalize_dual_seal"].iter().map(|action| (*action).to_string()).collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_DUAL_SEAL_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_DUAL_SEAL_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_DUAL_SEAL_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_DUAL_SEAL_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        ("manifest_protocol_family".to_string(), Value::Bool(metadata_str("protocol_family") == Some("NovaSeal"))),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_DUAL_SEAL_PROFILE))),
        ("manifest_conforms_to".to_string(), Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA))),
        ("manifest_canonical_schema_hash".to_string(), Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref())),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(metadata_str("stateful_dispatcher") == Some("missing-live-dispatcher")),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(metadata_str("btc_public_verification") == Some("missing-spv-or-indexer-evidence")),
        ),
        (
            "manifest_ckb_finality_gap".to_string(),
            Value::Bool(metadata_str("ckb_finality_verification") == Some("missing-live-maturity-evidence")),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(metadata_str("source_actions") == Some("src/nova_dual_seal_type.cell:finalize_dual_seal")),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-dual-seal-invariant-matrix-v0.1")),
        ),
        ("required_invariants_present".to_string(), Value::Bool(required_invariants.is_subset(&invariant_ids))),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str) == Some("missing-live-devnet-evidence")),
        ),
        (
            "btc_public_verification_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("btc_public_verification").and_then(Value::as_str) == Some("missing-spv-or-indexer-evidence"),
            ),
        ),
        (
            "ckb_finality_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("ckb_finality_verification").and_then(Value::as_str) == Some("missing-live-maturity-evidence"),
            ),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-dual-seal-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-evidence-not-btc-or-ckb-finality-or-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "live devnet dual-seal finality plus public BTC closure and CKB maturity evidence are still required before V1 finality claims",
    }))
}

fn validate_fiber_candidate_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(FIBER_CANDIDATE_ROOT);
    let manifest_path = repo_root.join(FIBER_CANDIDATE_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_FIBER_CANDIDATE_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["settle_fiber_candidate"].iter().map(|action| (*action).to_string()).collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_FIBER_CANDIDATE_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_FIBER_CANDIDATE_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_FIBER_CANDIDATE_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_FIBER_CANDIDATE_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        ("manifest_protocol_family".to_string(), Value::Bool(metadata_str("protocol_family") == Some("NovaSeal"))),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_FIBER_CANDIDATE_PROFILE))),
        ("manifest_conforms_to".to_string(), Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA))),
        ("manifest_canonical_schema_hash".to_string(), Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref())),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(metadata_str("stateful_dispatcher") == Some("missing-live-dispatcher")),
        ),
        (
            "manifest_fiber_execution_gap".to_string(),
            Value::Bool(metadata_str("fiber_execution") == Some("missing-live-fiber-evidence")),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(metadata_str("source_actions") == Some("src/nova_fiber_candidate_type.cell:settle_fiber_candidate")),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-fiber-candidate-invariant-matrix-v0.1")),
        ),
        ("required_invariants_present".to_string(), Value::Bool(required_invariants.is_subset(&invariant_ids))),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str) == Some("missing-live-devnet-evidence")),
        ),
        (
            "fiber_execution_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("fiber_execution").and_then(Value::as_str) == Some("missing-live-fiber-evidence")),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-fiber-candidate-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-evidence-not-live-fiber-or-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "live devnet Fiber candidate path and real Fiber execution evidence are still required before fiber_candidate_path can pass",
    }))
}

fn agreement_live_evidence(stateful_acceptance: &Value) -> Value {
    let agreement = stateful_acceptance
        .get("scenarios")
        .and_then(Value::as_array)
        .and_then(|scenarios| {
            scenarios.iter().find(|scenario| json_pointer_str(scenario, "/name") == Some("agreement_profile_originate_to_terminal"))
        })
        .cloned()
        .unwrap_or(Value::Null);
    let evidence = agreement.get("live_devnet_evidence").cloned().unwrap_or(Value::Null);
    let negative_checks = EXPECTED_LIVE_NEGATIVE_KEYS
        .iter()
        .map(|key| ((*key).to_string(), Value::Bool(json_pointer_bool(&evidence, &format!("/{key}")))))
        .collect::<Map<_, _>>();
    let live_keys = [
        "origin_active_live",
        "origin_principal_payout_live",
        "origin_receipt_live",
        "repay_old_active_not_live",
        "repay_closed_live",
        "repay_lender_repayment_live",
        "repay_borrower_collateral_return_live",
        "repay_receipt_live",
        "claim_old_active_not_live",
        "claim_closed_live",
        "claim_lender_default_claim_live",
        "claim_receipt_live",
    ];
    let checks = json!({
        "acceptance_passed": json_pointer_str(stateful_acceptance, "/status") == Some("passed"),
        "no_blockers": json_pointer_i64(stateful_acceptance, "/blocker_count") == Some(0),
        "live_devnet_rpc_executed": json_pointer_bool(stateful_acceptance, "/live_devnet_rpc_executed"),
        "stateful_lifecycle_executed": json_pointer_bool(stateful_acceptance, "/stateful_lifecycle_executed"),
        "agreement_scenario_passed": json_pointer_str(&agreement, "/status") == Some("passed"),
        "agreement_provenance_fresh": json_pointer_bool(&evidence, "/provenance_freshness_matched"),
        "valid_originate_repay_claim_live": live_keys.iter().all(|key| json_pointer_bool(&evidence, &format!("/{key}"))),
        "negative_cases_rejected": object_values_all_true(Some(&Value::Object(negative_checks.clone()))),
    });
    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "negative_checks": negative_checks,
        "evidence": evidence,
    })
}

fn compare_manifest_dep(repo_root: &Path, manifest_rel: &str, live: &Value, artifact_hash: Option<&str>) -> Result<Value> {
    let manifest_path = repo_root.join(manifest_rel);
    let manifest = toml_value(&manifest_path)?;
    let dep = runtime_dep(&manifest)?;
    let parsed = parse_out_point(toml_str(&dep, "out_point"));
    let expected_metadata = EXPECTED_VERIFIER.iter().all(|(key, value)| toml_str(&dep, key) == Some(*value));
    let production = manifest
        .get("policy")
        .and_then(toml::Value::as_table)
        .and_then(|policy| policy.get("production"))
        .and_then(toml::Value::as_bool);
    let checks = json!({
        "expected_metadata": expected_metadata,
        "out_point_valid": json_pointer_bool(&parsed, "/valid"),
        "out_point_non_placeholder": !placeholder_hash(json_pointer_str(&parsed, "/tx_hash")),
        "data_hash_non_placeholder": !placeholder_hash(normalize_hex(toml_str(&dep, "data_hash")).as_deref()),
        "artifact_hash_non_placeholder": !placeholder_hash(normalize_hex(toml_str(&dep, "artifact_hash")).as_deref()),
        "matches_live_data_hash": normalize_hex(toml_str(&dep, "data_hash")).as_deref() == json_pointer_str(live, "/data_hash"),
        "matches_live_dep_type": toml_str(&dep, "dep_type") == json_pointer_str(live, "/dep_type"),
        "matches_artifact_hash": normalize_hex(toml_str(&dep, "artifact_hash")).as_deref() == artifact_hash,
        "production_false_until_public_attestation": production == Some(false),
    });
    Ok(json!({
        "manifest": manifest_rel,
        "checks": checks,
        "dep": toml_to_json(&dep),
        "live": live,
        "policy": {
            "out_point": "manifest out_point is a pinned deployment descriptor; local live-devnet runs redeploy ephemeral outpoints and are compared by verifier data hash/artifact hash instead",
        },
    }))
}

fn validate_public_attestation(repo_root: &Path, rel_path: &str, artifact_hash: Option<&str>) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({"status": "external_required", "reason": "missing public/shared CellDep attestation"}));
    }
    let payload = json_load_path(repo_root, &path)?;
    let verifier = payload.get("runtime_verifier").cloned().unwrap_or(Value::Null);
    let parsed = parse_out_point(json_pointer_str(&verifier, "/out_point"));
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-public-shared-cell-dep-attestation-v0.1"),
        "status": json_pointer_str(&payload, "/status") == Some("attested"),
        "network_not_local_devnet": json_pointer_str(&payload, "/network").is_some_and(|network| !network.is_empty() && network != "local-devnet"),
        "artifact_hash": normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref() == artifact_hash,
        "data_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&verifier, "/data_hash")).as_deref()),
        "out_point_non_placeholder": !placeholder_hash(json_pointer_str(&parsed, "/tx_hash")),
        "verifier_id": json_pointer_str(&verifier, "/verifier_id") == Some("btc.bip340.v0"),
        "ipc_abi": json_pointer_str(&verifier, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "attestation": payload,
    }))
}

fn validate_external_review(repo_root: &Path, rel_path: &str, artifact_hash: Option<&str>) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({"status": "external_required", "reason": "missing external BIP340 TCB review attestation"}));
    }
    let payload = json_load_path(repo_root, &path)?;
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-bip340-external-tcb-review-attestation-v0.1"),
        "status": json_pointer_str(&payload, "/status") == Some("accepted"),
        "artifact_hash": normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref() == artifact_hash,
        "verifier_id": json_pointer_str(&payload, "/verifier_id") == Some("btc.bip340.v0"),
        "ipc_abi": json_pointer_str(&payload, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
        "reviewer_present": json_pointer_str(&payload, "/reviewer").is_some_and(|value| !value.is_empty()),
        "review_date_present": json_pointer_str(&payload, "/review_date").is_some_and(|value| !value.is_empty()),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "attestation": payload,
    }))
}

fn validate_attestation_templates(repo_root: &Path, artifact_hash: Option<&str>, source_tree_hash: Option<&str>) -> Result<Value> {
    let public_path = repo_root.join(PUBLIC_CELLDEP_ATTESTATION_TEMPLATE);
    let external_path = repo_root.join(EXTERNAL_TCB_ATTESTATION_TEMPLATE);
    let public_payload = if public_path.is_file() { Some(json_load_path(repo_root, &public_path)?) } else { None };
    let external_payload = if external_path.is_file() { Some(json_load_path(repo_root, &external_path)?) } else { None };
    let public = public_payload.as_ref().unwrap_or(&Value::Null);
    let external = external_payload.as_ref().unwrap_or(&Value::Null);
    let public_verifier = public.get("runtime_verifier").unwrap_or(&Value::Null);
    let checks = json!({
        "public_template_present": public_path.is_file(),
        "external_template_present": external_path.is_file(),
        "public_schema": json_pointer_str(public, "/schema") == Some("novaseal-public-shared-cell-dep-attestation-v0.1"),
        "external_schema": json_pointer_str(external, "/schema") == Some("novaseal-bip340-external-tcb-review-attestation-v0.1"),
        "public_template_network_not_local_devnet": json_pointer_str(public, "/network").is_some_and(|network| !network.is_empty() && network != "local-devnet"),
        "public_artifact_hash_matches_current_tcb": normalize_hex(json_pointer_str(public_verifier, "/artifact_hash")).as_deref() == artifact_hash,
        "external_artifact_hash_matches_current_tcb": normalize_hex(json_pointer_str(external, "/artifact_hash")).as_deref() == artifact_hash,
        "external_source_tree_hash_matches_current_tcb": normalize_hex(json_pointer_str(external, "/source_tree_sha256")).as_deref() == source_tree_hash,
        "public_verifier_id": json_pointer_str(public_verifier, "/verifier_id") == Some("btc.bip340.v0"),
        "external_verifier_id": json_pointer_str(external, "/verifier_id") == Some("btc.bip340.v0"),
        "public_ipc_abi": json_pointer_str(public_verifier, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
        "external_ipc_abi": json_pointer_str(external, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "expected_artifact_hash": artifact_hash,
        "expected_source_tree_sha256": source_tree_hash,
        "checks": checks,
        "templates": {
            "public_shared_cell_dep": rel(repo_root, &public_path),
            "external_bip340_tcb_review": rel(repo_root, &external_path),
        },
    }))
}

fn validate_security_audit_coverage(
    repo_root: &Path,
    core_security: &Value,
    invariant_matrix: &Value,
    live_evidence: &Value,
    tcb: &Value,
    attestation_templates: &Value,
) -> Result<Value> {
    let agreement_security = std::fs::read_to_string(repo_root.join(AGREEMENT_ROOT).join("docs/SECURITY.md")).unwrap_or_default();
    let agreement_audit = std::fs::read_to_string(repo_root.join(AGREEMENT_ROOT).join("docs/AUDIT_STATUS.md")).unwrap_or_default();
    let riscv_shell_doc = std::fs::read_to_string(repo_root.join(CORE_ROOT).join("docs/RISCV_VERIFIER_SHELL.md")).unwrap_or_default();
    let riscv_main = std::fs::read_to_string(repo_root.join(VERIFIER_ROOT).join("../novaseal_btc_verifier_riscv/src/main.rs"))
        .or_else(|_| std::fs::read_to_string(repo_root.join(CORE_ROOT).join("verifier/novaseal_btc_verifier_riscv/src/main.rs")))
        .unwrap_or_default();
    let unsafe_hits = tcb.pointer("/source_inventory/unsafe_hits").and_then(Value::as_array).cloned().unwrap_or_default();
    let review_hits = tcb.pointer("/source_inventory/review_hits").and_then(Value::as_array).cloned().unwrap_or_default();
    let unsafe_surface_isolated = unsafe_hits.iter().all(|hit| {
        json_pointer_str(hit, "/path").is_some_and(|path| {
            path.ends_with("Cargo.toml")
                || path == "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_riscv/src/main.rs"
        })
    });
    let unsafe_block_count = riscv_main.matches("unsafe {").count();
    let safety_comment_count = riscv_main.matches("// SAFETY:").count();
    let local_tcb_gates = tcb.get("local_review_gates").and_then(Value::as_array).cloned().unwrap_or_default();
    let local_tcb_gates_passed =
        !local_tcb_gates.is_empty() && local_tcb_gates.iter().all(|gate| json_pointer_str(gate, "/status") == Some("passed"));
    let checks = json!({
        "agreement_security_sections_present": agreement_security.contains("## Implemented Guards")
            && agreement_security.contains("## Not Implemented")
            && agreement_security.contains("## Risk Posture"),
        "agreement_audit_status_sections_present": agreement_audit.contains("## Claim Classification")
            && agreement_audit.contains("## Fixture Honesty")
            && agreement_audit.contains("## Production Statement Boundary"),
        "core_authority_binding_security_passed": json_pointer_str(core_security, "/status") == Some("passed"),
        "agreement_invariant_matrix_passed": json_pointer_str(invariant_matrix, "/status") == Some("passed"),
        "live_negative_cases_rejected": json_pointer_bool(live_evidence, "/checks/negative_cases_rejected"),
        "live_valid_paths_exercised": json_pointer_bool(live_evidence, "/checks/valid_originate_repay_claim_live"),
        "local_bip340_tcb_review_passed": json_pointer_str(tcb, "/status").is_some_and(|status| status.starts_with("passed_local_review")),
        "local_bip340_tcb_gates_passed": local_tcb_gates_passed,
        "tcb_source_inventory_present": json_pointer_str(tcb, "/source_inventory/source_tree_sha256").is_some()
            && json_pointer_i64(tcb, "/source_inventory/total_files").is_some(),
        "tcb_review_hits_empty": review_hits.is_empty(),
        "unsafe_boundary_documented": riscv_shell_doc.contains("## Unsafe Boundary")
            && riscv_shell_doc.contains("syscall register ABI only"),
        "unsafe_surface_isolated": unsafe_surface_isolated,
        "unsafe_blocks_have_safety_comments": unsafe_block_count > 0 && safety_comment_count >= unsafe_block_count,
        "external_attestation_templates_current": json_pointer_str(attestation_templates, "/status") == Some("passed"),
        "production_blockers_explicit": agreement_security.contains("public/shared CellDep")
            && agreement_security.contains("external BIP340")
            && agreement_audit.contains("external production attestations still required"),
    });
    Ok(json!({
        "schema": "novaseal-security-audit-coverage-v0.1",
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "unsafe_inventory": {
            "unsafe_hit_count": unsafe_hits.len(),
            "review_hit_count": review_hits.len(),
            "unsafe_block_count": unsafe_block_count,
            "safety_comment_count": safety_comment_count,
            "boundary": "RISC-V verifier shell syscall ABI only; no raw pointer dereference, transmute, mutable static, or C FFI memory access is accepted by this local audit gate.",
        },
        "residual_production_blockers": [
            "public/shared CellDep pinning attestation",
            "external BIP340 runtime verifier TCB review attestation",
        ],
    }))
}

fn live_verifier_facts(repo_root: &Path, rel_path: &str) -> Result<Value> {
    let payload = json_load(repo_root, rel_path)?;
    let verifier = payload.pointer("/artifacts/verifier").cloned().unwrap_or(Value::Null);
    let out_point = verifier.pointer("/cell_dep/out_point").cloned().unwrap_or(Value::Null);
    let index = json_pointer_str(&out_point, "/index")
        .and_then(|value| value.strip_prefix("0x").and_then(|hex| u64::from_str_radix(hex, 16).ok()))
        .or_else(|| out_point.get("index").and_then(Value::as_u64));
    Ok(json!({
        "status": json_pointer_str(&payload, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool_opt(&payload, "/live_devnet_rpc_executed"),
        "name": json_pointer_str(&verifier, "/name"),
        "tx_hash": normalize_hex(json_pointer_str(&out_point, "/tx_hash")),
        "index": index,
        "dep_type": json_pointer_str(&verifier, "/cell_dep/dep_type"),
        "data_hash": normalize_hex(json_pointer_str(&verifier, "/data_hash")),
        "artifact_size_bytes": verifier.get("artifact_size_bytes").and_then(Value::as_u64),
    }))
}

fn runtime_dep(manifest: &toml::Value) -> Result<toml::Value> {
    let deps = manifest
        .get("deploy")
        .and_then(toml::Value::as_table)
        .and_then(|deploy| deploy.get("ckb"))
        .and_then(toml::Value::as_table)
        .and_then(|ckb| ckb.get("cell_deps"))
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let matches = deps
        .into_iter()
        .filter(|dep| {
            toml_str(dep, "role") == Some("runtime_verifier") || toml_str(dep, "name") == Some("cellscript_btc_bip340_verifier_riscv")
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(CompileError::without_span(format!(
            "expected exactly one NovaSeal runtime verifier dep, found {}",
            matches.len()
        )));
    }
    Ok(matches[0].clone())
}

fn expected_files(repo_root: &Path, root: &Path, names: &[&str]) -> Result<Value> {
    let expected = names.iter().map(|name| (*name).to_string()).collect::<BTreeSet<_>>();
    let found = if root.is_dir() {
        std::fs::read_dir(root)?
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| entry.file_type().ok().filter(|ty| ty.is_file()).map(|_| entry.file_name()))
            .filter_map(|name| name.into_string().ok())
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    let mut hashes = Map::new();
    for name in &expected {
        let path = root.join(name);
        if path.is_file() {
            hashes.insert(name.clone(), Value::String(sha256_file_hex(&path)?));
        }
    }
    Ok(json!({
        "root": rel(repo_root, root),
        "expected": expected.iter().cloned().collect::<Vec<_>>(),
        "present": found.intersection(&expected).cloned().collect::<Vec<_>>(),
        "missing": expected.difference(&found).cloned().collect::<Vec<_>>(),
        "extra": found.difference(&expected).cloned().collect::<Vec<_>>(),
        "hashes": hashes,
        "exact": found == expected,
    }))
}

fn canonical_schema_lines(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let source = std::fs::read_to_string(path)?;
    Ok(source
        .lines()
        .filter_map(|raw| {
            let stripped = raw.split_once('#').map_or(raw, |(before, _)| before).trim();
            if stripped.is_empty() {
                return None;
            }
            if let Some((name, rest)) = stripped.split_once(':') {
                let rest = rest.split_whitespace().collect::<Vec<_>>().join(" ");
                if rest.is_empty() {
                    Some(format!("{}:", name.trim()))
                } else {
                    Some(format!("{}: {rest}", name.trim()))
                }
            } else {
                Some(stripped.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect())
}

fn canonical_schema_hash(path: &Path) -> Result<Option<String>> {
    let lines = canonical_schema_lines(path)?;
    if lines.is_empty() {
        return Ok(None);
    }
    let mut payload = lines.join("\n").into_bytes();
    payload.push(b'\n');
    Ok(Some(format!("0x{}", hex::encode(Sha256::digest(payload)))))
}

fn canonical_schema_checks(path: &Path) -> Result<Map<String, Value>> {
    let lines = canonical_schema_lines(path)?;
    let mut expected_lines = vec![format!("{EXPECTED_NOVASEAL_CANONICAL_ENVELOPE}:")];
    expected_lines.extend(EXPECTED_CANONICAL_SCHEMA_FIELDS.iter().map(|(name, ty)| format!("{name}: {ty}")));
    let expected_set = expected_lines.iter().cloned().collect::<BTreeSet<_>>();
    let lines_set = lines.iter().cloned().collect::<BTreeSet<_>>();
    Ok([
        ("canonical_schema_file_present".to_string(), Value::Bool(path.exists())),
        (
            "canonical_schema_name".to_string(),
            Value::Bool(lines.first().is_some_and(|line| line == &format!("{EXPECTED_NOVASEAL_CANONICAL_ENVELOPE}:"))),
        ),
        ("canonical_schema_exact_field_order".to_string(), Value::Bool(lines == expected_lines)),
        ("canonical_schema_no_extra_fields".to_string(), Value::Bool(lines_set == expected_set)),
        ("canonical_schema_normalized_hash_present".to_string(), Value::Bool(canonical_schema_hash(path)?.is_some())),
    ]
    .into_iter()
    .collect())
}

fn manifest_metadata(path: &Path) -> Result<toml::Value> {
    Ok(toml_value(path)?.get("metadata").cloned().unwrap_or_else(|| toml::Value::Table(Default::default())))
}

fn toml_value(path: &Path) -> Result<toml::Value> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| CompileError::without_span(format!("failed to read TOML '{}': {}", path.display(), error)))?;
    toml::from_str(&source)
        .map_err(|error| CompileError::without_span(format!("failed to parse TOML '{}': {}", path.display(), error)))
}

fn toml_to_json(value: &toml::Value) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn toml_str<'a>(value: &'a toml::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(toml::Value::as_str)
}

fn read_cell_sources(src_root: &Path) -> Result<String> {
    if !src_root.is_dir() {
        return Ok(String::new());
    }
    let mut paths = std::fs::read_dir(src_root)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cell") && path.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    let mut source = String::new();
    for path in paths {
        source.push_str(&std::fs::read_to_string(path)?);
        source.push('\n');
    }
    Ok(source)
}

fn json_load(repo_root: &Path, rel_path: &str) -> Result<Value> {
    json_load_path(repo_root, &repo_root.join(rel_path))
}

fn json_load_path(repo_root: &Path, path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({"missing": true, "path": rel(repo_root, path)}));
    }
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| CompileError::without_span(format!("failed to parse JSON '{}': {}", path.display(), error)))
}

fn json_load_path_optional(path: &Path) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Ok(Some(value)),
        Ok(_) => Ok(Some(json!({"_invalid_json": "top-level value is not an object"}))),
        Err(error) => Ok(Some(json!({"_invalid_json": error.to_string()}))),
    }
}

fn write_json_report(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| CompileError::without_span(format!("failed to serialize JSON report '{}': {}", path.display(), error)))?;
    std::fs::write(path, json + "\n")?;
    Ok(())
}

fn gate(name: &str, status: &str, evidence: &str, detail: Value) -> Value {
    json!({"name": name, "status": status, "evidence": evidence, "detail": detail})
}

fn wallet_gate_passed(wallet: &Value) -> bool {
    json_pointer_str(wallet, "/status") == Some("passed")
        && json_pointer_i64(wallet, "/summary/core_vectors").unwrap_or_default() >= 6
        && json_pointer_i64(wallet, "/summary/agreement_vectors").unwrap_or_default() >= 3
        && json_pointer_i64(wallet, "/summary/matched") == json_pointer_i64(wallet, "/summary/total")
}

fn stateful_acceptance_passed(stateful_acceptance: &Value) -> bool {
    json_pointer_str(stateful_acceptance, "/status") == Some("passed")
        && json_pointer_i64(stateful_acceptance, "/blocker_count") == Some(0)
        && json_pointer_bool(stateful_acceptance, "/live_devnet_rpc_executed")
        && json_pointer_bool(stateful_acceptance, "/stateful_lifecycle_executed")
        && json_pointer_str(stateful_acceptance, "/profile_coverage/status") == Some("passed")
        && json_pointer_str(stateful_acceptance, "/business_scenario_coverage/status") == Some("passed")
}

fn object_values_all_true(value: Option<&Value>) -> bool {
    value.and_then(Value::as_object).is_some_and(|object| object.values().all(|value| value.as_bool() == Some(true)))
}

fn value_is_present(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(_) => true,
    }
}

fn json_pointer_str<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

fn json_pointer_i64(value: &Value, pointer: &str) -> Option<i64> {
    value.pointer(pointer).and_then(Value::as_i64)
}

fn json_pointer_bool(value: &Value, pointer: &str) -> bool {
    value.pointer(pointer).and_then(Value::as_bool).unwrap_or(false)
}

fn json_pointer_bool_opt(value: &Value, pointer: &str) -> Option<bool> {
    value.pointer(pointer).and_then(Value::as_bool)
}

fn json_array_strings(value: &Value, pointer: &str) -> Vec<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn parse_out_point(value: Option<&str>) -> Value {
    let Some(raw) = value else {
        return json!({"valid": false, "raw": Value::Null});
    };
    let Some((tx_hash, index)) = raw.split_once(':') else {
        return json!({"valid": false, "raw": raw});
    };
    json!({
        "valid": is_hex32(tx_hash) && index.parse::<u64>().is_ok(),
        "tx_hash": tx_hash.to_ascii_lowercase(),
        "index": index.parse::<u64>().ok(),
    })
}

fn normalize_hex(value: Option<&str>) -> Option<String> {
    value.map(|raw| {
        let lower = raw.to_ascii_lowercase();
        if lower.starts_with("0x") {
            lower
        } else {
            format!("0x{lower}")
        }
    })
}

fn placeholder_hash(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };
    if !is_hex32(value) {
        return true;
    }
    value[2..].bytes().all(|byte| byte == b'0')
}

fn is_hex32(value: &str) -> bool {
    value.len() == 66 && value.starts_with("0x") && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_hex_bytes(value: &str) -> bool {
    value.len() > 2 && value.len() % 2 == 0 && value.starts_with("0x") && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_file_hex(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(format!("0x{}", hex::encode(Sha256::digest(bytes))))
}

fn rel(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root).unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_schema_normalisation_hashes_comment_free_lines() {
        let temp = tempfile::tempdir().unwrap();
        let schema = temp.path().join("schema");
        std::fs::write(&schema, "# ignored\nNovaSealCanonicalEnvelopeV0:\nprofile_id:   Byte32 # comment\n\npolicy_hash: Byte32\n")
            .unwrap();

        let lines = canonical_schema_lines(&schema).unwrap();

        assert_eq!(
            lines,
            vec!["NovaSealCanonicalEnvelopeV0:".to_string(), "profile_id: Byte32".to_string(), "policy_hash: Byte32".to_string()]
        );
        assert_eq!(
            canonical_schema_hash(&schema).unwrap().unwrap(),
            "0x6b4277f67ee3e47f391d8591f7efccc6e97dcac5436dd22568d72689ac4db130"
        );
    }

    #[test]
    fn out_point_parser_rejects_placeholder_shapes() {
        let parsed = parse_out_point(Some("0x0000000000000000000000000000000000000000000000000000000000000000:0"));

        assert!(json_pointer_bool(&parsed, "/valid"));
        assert!(placeholder_hash(json_pointer_str(&parsed, "/tx_hash")));
    }

    #[test]
    fn attestation_templates_must_match_current_tcb_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let proofs = temp.path().join("proposals/novaseal/v0-mvp-skeleton/proofs");
        std::fs::create_dir_all(&proofs).unwrap();
        let artifact_hash = format!("0x{}", "aa".repeat(32));
        let source_tree_hash = format!("0x{}", "bb".repeat(32));
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
                "status": "attested",
                "network": "testnet",
                "runtime_verifier": {
                    "verifier_id": "btc.bip340.v0",
                    "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                    "artifact_hash": artifact_hash,
                },
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.template.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": "novaseal-bip340-external-tcb-review-attestation-v0.1",
                "status": "accepted",
                "verifier_id": "btc.bip340.v0",
                "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                "artifact_hash": artifact_hash,
                "source_tree_sha256": source_tree_hash,
            }))
            .unwrap(),
        )
        .unwrap();

        let passed = validate_attestation_templates(temp.path(), Some(&artifact_hash), Some(&source_tree_hash)).unwrap();
        let failed =
            validate_attestation_templates(temp.path(), Some(&format!("0x{}", "cc".repeat(32))), Some(&source_tree_hash)).unwrap();

        assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
        assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed, "/checks/public_artifact_hash_matches_current_tcb"));
        assert!(!json_pointer_bool(&failed, "/checks/external_artifact_hash_matches_current_tcb"));
    }

    #[test]
    fn stateful_acceptance_requires_profile_and_business_coverage() {
        let mut report = json!({
            "status": "passed",
            "blocker_count": 0,
            "live_devnet_rpc_executed": true,
            "stateful_lifecycle_executed": true,
            "profile_coverage": { "status": "passed" },
            "business_scenario_coverage": { "status": "passed" },
        });

        assert!(stateful_acceptance_passed(&report));

        report["business_scenario_coverage"]["status"] = Value::String("failed".to_string());
        assert!(!stateful_acceptance_passed(&report));
    }

    #[test]
    fn security_audit_coverage_requires_docs_tcb_and_live_negative_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let agreement_docs = temp.path().join(AGREEMENT_ROOT).join("docs");
        let core_docs = temp.path().join(CORE_ROOT).join("docs");
        let riscv_src = temp.path().join(CORE_ROOT).join("verifier/novaseal_btc_verifier_riscv/src");
        std::fs::create_dir_all(&agreement_docs).unwrap();
        std::fs::create_dir_all(&core_docs).unwrap();
        std::fs::create_dir_all(&riscv_src).unwrap();
        std::fs::write(
            agreement_docs.join("SECURITY.md"),
            "## Implemented Guards\npublic/shared CellDep\nexternal BIP340\n## Not Implemented\n## Risk Posture\n",
        )
        .unwrap();
        std::fs::write(
            agreement_docs.join("AUDIT_STATUS.md"),
            "## Claim Classification\n## Fixture Honesty\nexternal production attestations still required\n## Production Statement Boundary\n",
        )
        .unwrap();
        std::fs::write(core_docs.join("RISCV_VERIFIER_SHELL.md"), "## Unsafe Boundary\nsyscall register ABI only\n").unwrap();
        std::fs::write(
            riscv_src.join("main.rs"),
            "// SAFETY: test syscall boundary\nunsafe {\n}\n// SAFETY: second syscall boundary\nunsafe {\n}\n",
        )
        .unwrap();

        let core_security = json!({ "status": "passed" });
        let invariant_matrix = json!({ "status": "passed" });
        let live_evidence = json!({
            "checks": {
                "negative_cases_rejected": true,
                "valid_originate_repay_claim_live": true,
            }
        });
        let tcb = json!({
            "status": "passed_local_review_external_attestation_required",
            "source_inventory": {
                "source_tree_sha256": format!("0x{}", "11".repeat(32)),
                "total_files": 3,
                "unsafe_hits": [
                    { "path": "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_riscv/src/main.rs" }
                ],
                "review_hits": [],
            },
            "local_review_gates": [
                { "name": "reference_bip340_vectors", "status": "passed" }
            ],
        });
        let attestation_templates = json!({ "status": "passed" });

        let passed = validate_security_audit_coverage(
            temp.path(),
            &core_security,
            &invariant_matrix,
            &live_evidence,
            &tcb,
            &attestation_templates,
        )
        .unwrap();
        let mut failed_tcb = tcb.clone();
        failed_tcb["source_inventory"]["review_hits"] = json!([{ "path": "todo.rs", "line": 1 }]);
        let failed = validate_security_audit_coverage(
            temp.path(),
            &core_security,
            &invariant_matrix,
            &live_evidence,
            &failed_tcb,
            &attestation_templates,
        )
        .unwrap();

        assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
        assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed, "/checks/tcb_review_hits_empty"));
    }

    #[test]
    fn v1_readiness_requires_all_planned_profiles_before_external_only_status() {
        let profile_certification = json!({
            "status": "passed",
            "production_statement_eligible": false,
            "production_statement_blockers": [
                "public_shared_cell_dep_attested",
                "external_bip340_tcb_review_attested",
            ],
            "local_checks": {
                "conformance_gate_passed": true,
                "wallet_vector_detail_passed": true,
                "local_bip340_tcb_review_passed": true,
            },
            "security_audit_coverage": { "status": "passed" },
        });
        let stateful_acceptance = json!({
            "status": "passed",
            "blocker_count": 0,
            "live_devnet_rpc_executed": true,
            "stateful_lifecycle_executed": true,
            "profile_coverage": { "status": "passed" },
            "business_scenario_coverage": { "status": "passed" },
        });
        let local_gates = vec![
            gate("public_shared_cell_dep_pinning_attestation", "external_required", PUBLIC_CELLDEP_ATTESTATION, Value::Null),
            gate(
                "external_bip340_runtime_verifier_tcb_review_attestation",
                "external_required",
                EXTERNAL_TCB_ATTESTATION,
                Value::Null,
            ),
        ];

        let local = build_v1_readiness(&profile_certification, &stateful_acceptance, &local_gates, true, false);
        assert_eq!(json_pointer_str(&local, "/status"), Some("planned_profiles_incomplete"));
        assert!(!json_pointer_bool(&local, "/local_v1_ready"));
        assert!(!json_pointer_bool(&local, "/production_ready"));
        assert_eq!(json_pointer_str(&local, "/dimensions/1/status"), Some("failed"));
        assert_eq!(json_pointer_str(&local, "/planned_profile_matrix/status"), Some("incomplete"));
        let missing = json_array_strings(&local, "/planned_profile_matrix/missing");
        assert!(missing.iter().any(|id| id == "object_profile_fungible_xudt"));
        assert!(missing.iter().any(|id| id == "seal_profile_btc_utxo_seal"));
    }

    #[test]
    fn planned_matrix_counts_fungible_package_but_keeps_value_flow_missing() {
        let profile_certification = json!({
            "status": "passed",
            "production_statement_eligible": false,
            "local_checks": {
                "wallet_vector_detail_passed": true,
                "local_bip340_tcb_review_passed": true,
            },
            "planned_profile_packages": {
                "btc_tx_commitment": { "status": "passed" },
                "btc_utxo_seal": { "status": "passed" },
                "dual_seal": { "status": "passed" },
                "fiber_candidate": { "status": "passed" },
                "fungible_xudt": { "status": "passed" },
                "rwa_receipt": { "status": "passed" }
            },
        });
        let stateful_acceptance = json!({
            "profile_coverage": {
                "covered_profiles": [
                    { "status": "passed" },
                    { "status": "passed" }
                ]
            },
            "business_scenario_coverage": {
                "status": "failed",
                "checks": {
                    "agreement_originate_live": true,
                    "agreement_repay_live": true,
                    "agreement_claim_live": true,
                    "agreement_negative_business_cases_preserve_live_state": true,
                    "btc_transaction_commitment_transition_live": false,
                    "btc_utxo_seal_closure_live": false,
                    "fungible_xudt_value_flow_live": false,
                    "rwa_receipt_lifecycle_live": false,
                    "fiber_candidate_path_live": false
                }
            },
        });

        let matrix = build_planned_profile_matrix(&profile_certification, &stateful_acceptance);
        let fungible_profile_status = matrix
            .pointer("/profiles")
            .and_then(Value::as_array)
            .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("object_profile_fungible_xudt")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let btc_tx_profile_status = matrix
            .pointer("/profiles")
            .and_then(Value::as_array)
            .and_then(|profiles| {
                profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("seal_profile_btc_transaction_commitment"))
            })
            .and_then(|row| json_pointer_str(row, "/status"));
        let btc_tx_flow_status = matrix
            .pointer("/business_scenarios")
            .and_then(Value::as_array)
            .and_then(|scenarios| {
                scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("btc_transaction_commitment_transition"))
            })
            .and_then(|row| json_pointer_str(row, "/status"));
        let btc_utxo_profile_status = matrix
            .pointer("/profiles")
            .and_then(Value::as_array)
            .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("seal_profile_btc_utxo_seal")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let btc_utxo_flow_status = matrix
            .pointer("/business_scenarios")
            .and_then(Value::as_array)
            .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("btc_utxo_seal_closure")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let dual_seal_profile_status = matrix
            .pointer("/profiles")
            .and_then(Value::as_array)
            .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("seal_profile_dual_seal")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let fungible_flow_status = matrix
            .pointer("/business_scenarios")
            .and_then(Value::as_array)
            .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("fungible_xudt_value_flow")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let rwa_profile_status = matrix
            .pointer("/profiles")
            .and_then(Value::as_array)
            .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("object_profile_rwa_receipt")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let rwa_flow_status = matrix
            .pointer("/business_scenarios")
            .and_then(Value::as_array)
            .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("rwa_receipt_lifecycle")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let fiber_profile_status = matrix
            .pointer("/profiles")
            .and_then(Value::as_array)
            .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("future_fiber_test_path")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let fiber_flow_status = matrix
            .pointer("/business_scenarios")
            .and_then(Value::as_array)
            .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("fiber_candidate_path")))
            .and_then(|row| json_pointer_str(row, "/status"));
        let missing = json_array_strings(&matrix, "/missing");

        assert_eq!(json_pointer_str(&matrix, "/status"), Some("incomplete"));
        assert_eq!(btc_tx_profile_status, Some("passed"));
        assert_eq!(btc_tx_flow_status, Some("missing"));
        assert_eq!(btc_utxo_profile_status, Some("passed"));
        assert_eq!(btc_utxo_flow_status, Some("missing"));
        assert_eq!(dual_seal_profile_status, Some("passed"));
        assert_eq!(fungible_profile_status, Some("passed"));
        assert_eq!(fungible_flow_status, Some("missing"));
        assert_eq!(rwa_profile_status, Some("passed"));
        assert_eq!(rwa_flow_status, Some("missing"));
        assert_eq!(fiber_profile_status, Some("passed"));
        assert_eq!(fiber_flow_status, Some("missing"));
        assert!(!missing.iter().any(|id| id == "seal_profile_btc_transaction_commitment"));
        assert!(!missing.iter().any(|id| id == "seal_profile_btc_utxo_seal"));
        assert!(!missing.iter().any(|id| id == "seal_profile_dual_seal"));
        assert!(!missing.iter().any(|id| id == "object_profile_fungible_xudt"));
        assert!(!missing.iter().any(|id| id == "object_profile_rwa_receipt"));
        assert!(!missing.iter().any(|id| id == "future_fiber_test_path"));
        assert!(missing.iter().any(|id| id == "btc_transaction_commitment_transition"));
        assert!(missing.iter().any(|id| id == "btc_utxo_seal_closure"));
        assert!(missing.iter().any(|id| id == "fungible_xudt_value_flow"));
        assert!(missing.iter().any(|id| id == "rwa_receipt_lifecycle"));
        assert!(missing.iter().any(|id| id == "fiber_candidate_path"));
    }
}
