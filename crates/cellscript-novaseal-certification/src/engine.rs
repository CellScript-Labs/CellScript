use crate::error::Result;
use crate::IMPLEMENTATION_ID;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

mod bitcoin_anchors;
mod bitcoin_proofs;
mod endpoint_evidence;
mod evidence_adapters;
mod external_evidence;
mod fiber_provenance;
mod live_reports;
mod package_sources;
mod profile_certification;
mod profile_packages;
mod profile_readiness;
mod provenance;
mod readiness;
mod report_files;
mod report_origins;
mod source_security;
mod stateful_acceptance;
mod timestamps;
mod wallet_and_operator;

#[cfg(test)]
use evidence_adapters::{validate_btc_spv_evidence_adapter_detail, validate_external_attestation_adapter_detail};
use external_evidence::{
    validate_btc_spv_evidence, validate_external_review, validate_public_attestation, validate_rwa_legal_registry_review,
};
use package_sources::{live_verifier_facts, runtime_dep, toml_str, toml_to_json, toml_value};
use profile_certification::{validate_profile_certification, ProfileCertificationInputs};
use profile_readiness::build_profile_production_completeness;
use readiness::{build_v1_readiness, production_gate_status};
use report_files::{json_load, write_json_report};
pub(crate) use source_security::validate_agreement_profile_conformance;
use source_security::validate_core_security_source;
use stateful_acceptance::build_stateful_acceptance_report;
use wallet_and_operator::validate_wallet_lock_alignment_detail;

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
const DUAL_SEAL_LIVE: &str = "target/novaseal-dual-seal-devnet-stateful-live.json";
const FIBER_CANDIDATE_LIVE: &str = "target/novaseal-fiber-candidate-devnet-stateful-live.json";
const FIBER_NODE_EXPERIMENTS: &str = "target/novaseal-fiber-node-experiments.json";
const STATEFUL_ACCEPTANCE: &str = "target/novaseal-devnet-stateful-acceptance.json";
const WALLET_VECTORS: &str = "target/novaseal-wallet-signing-vectors.json";
const WALLET_LOCK_ALIGNMENT: &str = "proposals/novaseal/v0-mvp-skeleton/target/novaseal-wallet-signing-alignment.json";
const PROFILE_OPERATOR_FIXTURES: &str = "target/novaseal-profile-operator-fixtures.json";
const SERVICE_BUILDER_FIXTURES: &str = "target/novaseal-service-builder-fixtures.json";
const BTC_SPV_EVIDENCE_ADAPTER: &str = "target/novaseal-btc-spv-evidence-adapter.json";
const EXTERNAL_ATTESTATION_ADAPTER: &str = "target/novaseal-external-attestation-adapter.json";
const EXTERNAL_EVIDENCE_HANDOFF: &str = "target/novaseal-external-evidence-handoff-bundle.json";
const TCB_REVIEW: &str = "target/novaseal-bip340-tcb-review.json";
const PUBLIC_CELLDEP_ATTESTATION: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json";
const EXTERNAL_TCB_ATTESTATION: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json";
const PUBLIC_BTC_SPV_EVIDENCE: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json";
const RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE: &str =
    "proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.json";
const PUBLIC_CELLDEP_ATTESTATION_TEMPLATE: &str =
    "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json";
const EXTERNAL_TCB_ATTESTATION_TEMPLATE: &str =
    "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json";
const PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json";
const RWA_LEGAL_REGISTRY_REVIEW_TEMPLATE: &str =
    "proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.template.json";

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
const EXPECTED_NOVASEAL_RELEASE_VERSION: &str = "0.0.1-v0-mvp";
const EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE: &str = "code";
const EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE: &str = "data1";
const EXPECTED_BTC_SPV_EVIDENCE_PROFILES: &[&str] =
    &[EXPECTED_BTC_TX_COMMITMENT_PROFILE, EXPECTED_BTC_UTXO_SEAL_PROFILE, EXPECTED_DUAL_SEAL_PROFILE];
const EXPECTED_BTC_SPV_PROFILE_SCENARIOS: &[(&str, &str)] = &[
    (EXPECTED_BTC_TX_COMMITMENT_PROFILE, "btc-transaction-commitment-transition"),
    (EXPECTED_BTC_UTXO_SEAL_PROFILE, "btc-utxo-seal-closure"),
    (EXPECTED_DUAL_SEAL_PROFILE, "dual-seal-finality"),
];

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

const EXPECTED_FIBER_CANDIDATE_DOCS: &[&str] =
    &["AUDIT_STATUS.md", "DEVNET_STATEFUL_ACCEPTANCE.md", "FIBER_NODE_EXPERIMENTS.md", "SECURITY.md"];

const EXPECTED_PUBLIC_CELLDEP_ATTESTATION_FIELDS: &[&str] =
    &["attested_at", "attestor", "network", "notes", "release", "request_handoff", "runtime_verifier", "schema", "status"];
const EXPECTED_PUBLIC_CELLDEP_RELEASE_FIELDS: &[&str] = &["manifest_commit", "package", "version"];
const EXPECTED_PUBLIC_CELLDEP_RUNTIME_VERIFIER_FIELDS: &[&str] =
    &["artifact_hash", "data_hash", "dep_type", "hash_type", "ipc_abi", "out_point", "verifier_id"];
const NOVASEAL_HANDOFF_HASH_ALGORITHM: &str = "blake2b-256(person=NovaExtHandoff)";
const EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS: &[&str] = &["bundle", "bundle_hash", "bundle_hash_algorithm", "group"];
const EXPECTED_EXTERNAL_TCB_REVIEW_ATTESTATION_FIELDS: &[&str] = &[
    "artifact_hash",
    "artifact_hash_algorithm",
    "ipc_abi",
    "notes",
    "report_uri",
    "request_handoff",
    "review_date",
    "review_scope",
    "reviewer",
    "schema",
    "source_tree_sha256",
    "status",
    "verifier_id",
];
const EXPECTED_PUBLIC_BTC_SPV_EVIDENCE_FIELDS: &[&str] =
    &["cases", "evidence_provider", "generated_at", "network", "notes", "request_handoff", "required_profiles", "schema", "status"];
const EXPECTED_PUBLIC_BTC_SPV_CASE_FIELDS: &[&str] = &[
    "btc_block_header",
    "btc_block_hash",
    "btc_merkle_proof",
    "btc_transaction_binding",
    "btc_tx_hex",
    "btc_txid",
    "btc_wtxid",
    "ckb_btc_commitment_hash",
    "ckb_live_tx_hash",
    "confirmations",
    "live_report_hash",
    "minimum_confirmations",
    "profile",
    "scenario",
    "source_service",
    "spv_client_cell_dep",
    "spv_proof_hash",
    "service_builder_case_hash",
    "service_builder_receipt_binding_hash",
    "service_builder_tx_skeleton_hash",
];
const EXPECTED_PUBLIC_BTC_SPV_MERKLE_PROOF_FIELDS: &[&str] =
    &["block_height", "merkle_branch", "merkle_root", "observed_tip_height", "tx_index"];
const EXPECTED_PUBLIC_BTC_SPV_CELLDEP_FIELDS: &[&str] = &["data_hash", "dep_type", "hash_type", "out_point"];
const EXPECTED_PUBLIC_BTC_SPV_SOURCE_SERVICE_FIELDS: &[&str] = &["commit", "name", "report_hash"];
const EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS: &[&str] = &[
    "network",
    "generated_at",
    "evidence_provider",
    "required_profiles",
    "profile",
    "scenario",
    "ckb_live_tx_hash",
    "live_report_hash",
    "service_builder_case_hash",
    "service_builder_tx_skeleton_hash",
    "service_builder_receipt_binding_hash",
    "ckb_btc_commitment_hash",
    "btc_txid",
    "btc_block_hash",
    "btc_block_header",
    "btc_merkle_proof.tx_index",
    "btc_merkle_proof.merkle_branch",
    "btc_merkle_proof.merkle_root",
    "btc_merkle_proof.block_height",
    "btc_merkle_proof.observed_tip_height",
    "btc_tx_hex",
    "btc_wtxid",
    "btc_transaction_binding.kind",
    "btc_transaction_binding.btc_output_index",
    "btc_transaction_binding.btc_amount_sats",
    "btc_transaction_binding.spend_input_index",
    "btc_transaction_binding.sealed_btc_txid",
    "btc_transaction_binding.sealed_btc_vout_index",
    "btc_transaction_binding.sealed_btc_amount_sats",
    "btc_transaction_binding.script_pubkey_hash",
    "btc_transaction_binding.sealed_btc_tx_hex",
    "btc_transaction_binding.sealed_utxo_commitment_hash",
    "spv_proof_hash",
    "minimum_confirmations",
    "confirmations",
    "spv_client_cell_dep.out_point",
    "spv_client_cell_dep.data_hash",
    "spv_client_cell_dep.dep_type",
    "spv_client_cell_dep.hash_type",
    "source_service.name",
    "source_service.commit",
    "source_service.report_hash",
    "request_handoff.bundle",
    "request_handoff.bundle_hash",
    "request_handoff.bundle_hash_algorithm",
    "request_handoff.group",
];
const EXPECTED_PUBLIC_BTC_SPV_HANDOFF_FIELDS: &[&str] = EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS;
const EXPECTED_BTC_SPV_FIELD_CONSTRAINTS: &[(&str, &str)] = &[
    ("network", "explicit public mainnet/testnet name; placeholders and local/devnet/regtest/simnet/private/fake labels are rejected"),
    ("generated_at", "UTC timestamp in YYYY-MM-DDTHH:MM:SSZ form; future timestamps are rejected"),
    (
        "evidence_provider",
        "real external provider identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
    ),
    ("ckb_live_tx_hash", "0x-prefixed 32-byte CKB live transaction hash matching the current NovaSeal service-builder case"),
    ("live_report_hash", "0x-prefixed 32-byte hash of the current NovaSeal live devnet report for this profile"),
    ("service_builder_case_hash", "0x-prefixed 32-byte hash of the current NovaSeal service-builder case for this profile"),
    ("service_builder_tx_skeleton_hash", "0x-prefixed 32-byte service-builder transaction skeleton hash for this profile"),
    ("service_builder_receipt_binding_hash", "0x-prefixed 32-byte service-builder receipt binding hash for this profile"),
    ("ckb_btc_commitment_hash", "0x-prefixed 32-byte CKB-side BTC commitment hash from the current live profile report"),
    ("btc_txid", "0x-prefixed 32-byte non-placeholder Bitcoin transaction id"),
    ("btc_block_hash", "0x-prefixed 32-byte non-placeholder Bitcoin block hash anchoring the SPV proof"),
    ("btc_block_header", "0x-prefixed 80-byte Bitcoin block header whose double-SHA256 hash matches btc_block_hash"),
    ("btc_merkle_proof.tx_index", "zero-based transaction index used to orient the Merkle branch"),
    (
        "btc_merkle_proof.merkle_branch",
        "array of 0x-prefixed 32-byte Bitcoin sibling hashes in display order; empty only for tx_index 0 in a single-transaction block",
    ),
    ("btc_merkle_proof.merkle_root", "0x-prefixed 32-byte Bitcoin Merkle root matching the block header"),
    ("btc_merkle_proof.block_height", "public Bitcoin block height containing btc_txid"),
    ("btc_merkle_proof.observed_tip_height", "public Bitcoin tip height used to compute confirmations"),
    ("btc_tx_hex", "0x-prefixed raw Bitcoin transaction bytes whose txid/wtxid match the public evidence case"),
    ("btc_wtxid", "0x-prefixed 32-byte Bitcoin witness transaction id derived from btc_tx_hex"),
    (
        "btc_transaction_binding.kind",
        "profile-specific binding kind: btc_transaction_output, btc_utxo_spend, or dual_seal_btc_closure",
    ),
    (
        "btc_transaction_binding.btc_output_index",
        "BTC transaction commitment output index; required for btc-transaction-commitment-profile-v0",
    ),
    (
        "btc_transaction_binding.btc_amount_sats",
        "BTC transaction commitment output amount in sats; required for btc-transaction-commitment-profile-v0",
    ),
    ("btc_transaction_binding.spend_input_index", "Bitcoin spend input index; required for UTXO and dual-seal closure profiles"),
    (
        "btc_transaction_binding.sealed_btc_txid",
        "sealed Bitcoin transaction id whose output is spent; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    ),
    (
        "btc_transaction_binding.sealed_btc_vout_index",
        "sealed Bitcoin output index; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    ),
    (
        "btc_transaction_binding.sealed_btc_amount_sats",
        "sealed Bitcoin output amount in sats; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    ),
    (
        "btc_transaction_binding.script_pubkey_hash",
        "0x-prefixed CKB Blake2b-256 hash of the sealed output scriptPubKey bytes; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    ),
    (
        "btc_transaction_binding.sealed_btc_tx_hex",
        "0x-prefixed raw sealed Bitcoin transaction bytes; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    ),
    (
        "btc_transaction_binding.sealed_utxo_commitment_hash",
        "0x-prefixed 32-byte CKB-side sealed UTXO commitment hash; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    ),
    ("spv_proof_hash", "0x-prefixed SHA-256 hash of the canonical BTC SPV proof material carried in this case"),
    ("minimum_confirmations", "integer confirmation floor; at least 6"),
    ("confirmations", "integer observed confirmations meeting minimum_confirmations"),
    ("spv_client_cell_dep.out_point", "0x-prefixed 32-byte CKB transaction hash plus numeric output index"),
    ("spv_client_cell_dep.data_hash", "0x-prefixed 32-byte non-placeholder SPV client data hash"),
    ("spv_client_cell_dep.dep_type", "code"),
    ("spv_client_cell_dep.hash_type", "data, data1, or type CKB script hash type"),
    (
        "source_service.name",
        "real external SPV service identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
    ),
    ("source_service.commit", "40-character hex service source commit"),
    ("source_service.report_hash", "0x-prefixed 32-byte non-placeholder SPV service report hash"),
    ("request_handoff.bundle", "target/novaseal-external-evidence-handoff-bundle.json"),
    ("request_handoff.bundle_hash", "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle"),
    ("request_handoff.bundle_hash_algorithm", "blake2b-256(person=NovaExtHandoff)"),
    ("request_handoff.group", "public_btc_spv_evidence"),
];
const EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS: &[&str] = &[
    "network",
    "attested_at",
    "attestor",
    "release.package",
    "release.version",
    "release.manifest_commit",
    "runtime_verifier.verifier_id",
    "runtime_verifier.ipc_abi",
    "runtime_verifier.out_point",
    "runtime_verifier.data_hash",
    "runtime_verifier.dep_type",
    "runtime_verifier.hash_type",
    "runtime_verifier.artifact_hash",
    "request_handoff.bundle",
    "request_handoff.bundle_hash",
    "request_handoff.bundle_hash_algorithm",
    "request_handoff.group",
];
const EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS: &[(&str, &str)] = &[
    (
        "network",
        "explicit public CKB mainnet/testnet name; placeholders and local/devnet/regtest/simnet/private/fake labels are rejected",
    ),
    ("attested_at", "UTC timestamp in YYYY-MM-DDTHH:MM:SSZ form; future timestamps are rejected"),
    (
        "attestor",
        "real independent release signer or deployer identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
    ),
    ("release.package", "novaseal"),
    ("release.version", "exact NovaSeal release version 0.0.1-v0-mvp"),
    ("release.manifest_commit", "40-character hex source commit matching the reviewed TCB repo_commit"),
    ("runtime_verifier.verifier_id", "btc.bip340.v0"),
    ("runtime_verifier.ipc_abi", "cellscript-btc-bip340-ipc-v0"),
    ("runtime_verifier.out_point", "0x-prefixed 32-byte CKB transaction hash plus numeric output index"),
    ("runtime_verifier.data_hash", "0x-prefixed 32-byte non-placeholder CellDep data hash"),
    ("runtime_verifier.dep_type", EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE),
    ("runtime_verifier.hash_type", "data1"),
    ("runtime_verifier.artifact_hash", "0x-prefixed 32-byte non-placeholder BIP340 runtime verifier artifact hash"),
    ("request_handoff.bundle", "target/novaseal-external-evidence-handoff-bundle.json"),
    ("request_handoff.bundle_hash", "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle"),
    ("request_handoff.bundle_hash_algorithm", "blake2b-256(person=NovaExtHandoff)"),
    ("request_handoff.group", "public_shared_cell_dep_attestation"),
];
const EXPECTED_PUBLIC_CELLDEP_EXPECTED_VALUE_FIELDS: &[&str] = &[
    "artifact_hash",
    "release.manifest_commit",
    "release.package",
    "release.version",
    "runtime_verifier.dep_type",
    "runtime_verifier.hash_type",
    "runtime_verifier.ipc_abi",
    "runtime_verifier.verifier_id",
];
const EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS: &[&str] = &[
    "reviewer",
    "review_date",
    "review_scope",
    "verifier_id",
    "ipc_abi",
    "artifact_hash",
    "artifact_hash_algorithm",
    "source_tree_sha256",
    "report_uri",
    "request_handoff.bundle",
    "request_handoff.bundle_hash",
    "request_handoff.bundle_hash_algorithm",
    "request_handoff.group",
];
const EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS: &[(&str, &str)] = &[
    (
        "reviewer",
        "real external reviewer identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
    ),
    ("review_date", "UTC date in YYYY-MM-DD form; future dates are rejected"),
    ("review_scope", "exact BIP340 verifier, RISC-V shell, IPC envelope, and artifact/CellDep pinning scope"),
    ("verifier_id", "btc.bip340.v0"),
    ("ipc_abi", "cellscript-btc-bip340-ipc-v0"),
    ("artifact_hash", "0x-prefixed 32-byte non-placeholder BIP340 runtime verifier artifact hash"),
    ("artifact_hash_algorithm", "sha256"),
    ("source_tree_sha256", "0x-prefixed 32-byte non-placeholder SHA-256 source tree hash"),
    (
        "report_uri",
        "HTTPS URI for the public review report or source-controlled review commit; example, loopback, private, and reserved hosts are rejected",
    ),
    ("request_handoff.bundle", "target/novaseal-external-evidence-handoff-bundle.json"),
    ("request_handoff.bundle_hash", "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle"),
    ("request_handoff.bundle_hash_algorithm", "blake2b-256(person=NovaExtHandoff)"),
    ("request_handoff.group", "external_bip340_tcb_review_attestation"),
];
const EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE: &[&str] = &[
    "BIP340 verifier core",
    "RISC-V runtime verifier shell",
    "CellScript BIP340 IPC envelope",
    "artifact hash and CellDep pinning requirements",
];
const EXPECTED_EXTERNAL_TCB_EXPECTED_VALUE_FIELDS: &[&str] =
    &["artifact_hash", "artifact_hash_algorithm", "ipc_abi", "review_scope", "source_tree_sha256", "verifier_id"];
const EXPECTED_RWA_LEGAL_REVIEW_EVIDENCE_FIELDS: &[&str] = &[
    "notes",
    "profile",
    "profile_source_tree_sha256",
    "registry",
    "report_uri",
    "request_handoff",
    "review_date",
    "review_scope",
    "reviewer",
    "schema",
    "status",
];
const EXPECTED_RWA_LEGAL_REVIEW_REGISTRY_FIELDS: &[&str] = &["authority", "jurisdiction", "registry_report_hash"];
const EXPECTED_RWA_LEGAL_REVIEW_REQUIRED_FIELDS: &[&str] = &[
    "profile",
    "reviewer",
    "review_date",
    "review_scope",
    "registry.authority",
    "registry.jurisdiction",
    "registry.registry_report_hash",
    "profile_source_tree_sha256",
    "report_uri",
    "request_handoff.bundle",
    "request_handoff.bundle_hash",
    "request_handoff.bundle_hash_algorithm",
    "request_handoff.group",
];
const EXPECTED_RWA_LEGAL_REVIEW_FIELD_CONSTRAINTS: &[(&str, &str)] = &[
    ("profile", "rwa-receipt-profile-v0"),
    (
        "reviewer",
        "real external legal or registry reviewer identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
    ),
    ("review_date", "UTC date in YYYY-MM-DD form; future dates are rejected"),
    ("review_scope", "exact RWA receipt legal-title, custody, registry-state, oracle-fact, and enforceability review scope"),
    (
        "registry.authority",
        "real registry or custodian authority identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
    ),
    (
        "registry.jurisdiction",
        "explicit real-world jurisdiction; placeholder, local/devnet/fake/internal, example, and unknown tokens are rejected",
    ),
    ("registry.registry_report_hash", "0x-prefixed 32-byte non-placeholder hash of the external registry/legal review report"),
    ("profile_source_tree_sha256", "0x-prefixed 32-byte non-placeholder SHA-256 hash of the RWA profile source tree"),
    (
        "report_uri",
        "HTTPS URI for the public legal/registry review report or source-controlled review commit; example, loopback, private, and reserved hosts are rejected",
    ),
    ("request_handoff.bundle", "target/novaseal-external-evidence-handoff-bundle.json"),
    ("request_handoff.bundle_hash", "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle"),
    ("request_handoff.bundle_hash_algorithm", "blake2b-256(person=NovaExtHandoff)"),
    ("request_handoff.group", "rwa_legal_registry_review_evidence"),
];
const EXPECTED_RWA_LEGAL_REVIEW_SCOPE: &[&str] = &[
    "RWA receipt legal title boundary",
    "RWA receipt custody and registry-state provenance",
    "RWA receipt oracle-fact exclusion boundary",
    "RWA receipt enforceability and jurisdiction boundary",
];
const EXPECTED_RWA_LEGAL_REVIEW_EXPECTED_VALUE_FIELDS: &[&str] = &["profile", "profile_source_tree_sha256", "review_scope"];
const RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS: &[&str] = &[
    RWA_RECEIPT_MANIFEST,
    "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_type.cell",
    "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_lifecycle_type.cell",
    "proposals/novaseal/rwa-receipt-profile-v0/schemas",
    "proposals/novaseal/rwa-receipt-profile-v0/fixtures",
    "proposals/novaseal/rwa-receipt-profile-v0/proofs/invariant_matrix.json",
];
const BIP340_TCB_SOURCE_HASH_PATHS: &[&str] = &[
    "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_core",
    "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_riscv",
    "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
];

const EXPECTED_FIBER_NODE_EXECUTION_SCHEMA: &str = "novaseal-fiber-node-execution-v0.4";
const EXPECTED_FIBER_REPO_ORIGIN: &str = "https://github.com/nervosnetwork/fiber.git";
const EXPECTED_FIBER_NODE_PROFILES: &[&str] = &[
    EXPECTED_BTC_TX_COMMITMENT_PROFILE,
    EXPECTED_BTC_UTXO_SEAL_PROFILE,
    EXPECTED_FIBER_CANDIDATE_PROFILE,
    EXPECTED_FUNGIBLE_XUDT_PROFILE,
];
const EXPECTED_FIBER_WORKFLOWS: &[(&str, &[&str])] = &[
    ("open-use-close-a-channel", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("3-nodes-transfer", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("router-pay", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("invoice-ops", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("shutdown-force", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("reestablish", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("external-funding-open", &[EXPECTED_FIBER_CANDIDATE_PROFILE, EXPECTED_BTC_TX_COMMITMENT_PROFILE]),
    ("funding-tx-verification", &[EXPECTED_FIBER_CANDIDATE_PROFILE, EXPECTED_BTC_TX_COMMITMENT_PROFILE]),
    ("udt", &[EXPECTED_FIBER_CANDIDATE_PROFILE, EXPECTED_FUNGIBLE_XUDT_PROFILE]),
    ("udt-router-pay", &[EXPECTED_FIBER_CANDIDATE_PROFILE, EXPECTED_FUNGIBLE_XUDT_PROFILE]),
    ("watchtower/force-close-after-open-channel", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("watchtower/force-close-with-pending-tlcs", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("watchtower/force-close-with-pending-tlcs-and-udt", &[EXPECTED_FIBER_CANDIDATE_PROFILE, EXPECTED_FUNGIBLE_XUDT_PROFILE]),
    ("watchtower/force-close-preimage-multiple", &[EXPECTED_FIBER_CANDIDATE_PROFILE]),
    ("cross-chain-hub", &[EXPECTED_FIBER_CANDIDATE_PROFILE, EXPECTED_BTC_TX_COMMITMENT_PROFILE, EXPECTED_BTC_UTXO_SEAL_PROFILE]),
    (
        "cross-chain-hub-separate",
        &[EXPECTED_FIBER_CANDIDATE_PROFILE, EXPECTED_BTC_TX_COMMITMENT_PROFILE, EXPECTED_BTC_UTXO_SEAL_PROFILE],
    ),
];

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
    ("expected_event_data_hash", "intent.expected_event_data_hash == ckb::hash_data_packed(event)"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_nonce_increment", "old_cell.nonce < U64_MAX"),
    ("amount_conservation", "intent.core.settlement_amount == old_cell.amount"),
    ("dual_authority_settlement", "issuer_sig.pubkey == old_cell.issuer_authority_hash.0"),
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
    ("lifecycle_action", "action nova_dual_seal_lifecycle"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("expected_receipt_hash", "intent.expected_receipt_hash == materialized_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("checked_u64_max", "const U64_MAX: u64 = 18446744073709551615"),
    ("checked_nonce_increment", "old_cell.nonce < U64_MAX"),
    ("ckb_maturity_gate", "now >= old_cell.maturity_timepoint"),
    ("btc_owner_authority", "btc_owner_sig.pubkey == old_cell.btc_owner_authority_hash.0"),
    ("ckb_authority", "ckb_sig.pubkey == old_cell.ckb_authority_hash.0"),
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
    ("operator_authority", "operator_sig.pubkey == old_cell.operator_authority_hash.0"),
];

#[derive(Clone, Copy)]
struct ExpectedWalletAction {
    signers: &'static [&'static str],
    old_status: i64,
    new_status: i64,
    old_nonce: i64,
    new_nonce: i64,
}

#[derive(Clone, Copy)]
struct ExpectedOperatorFixture {
    profile: &'static str,
    action: &'static str,
    fixture: &'static str,
    signers: &'static [&'static str],
    live_report: &'static str,
    live_tx_hash_pointer: &'static str,
    live_required: bool,
    fiber_report: Option<&'static str>,
    fiber_required: bool,
}

const EXPECTED_AGREEMENT_WALLET_ACTIONS: &[(&str, ExpectedWalletAction)] = &[
    (
        "originate_agreement",
        ExpectedWalletAction { signers: &["borrower", "lender"], old_status: 0, new_status: 1, old_nonce: 0, new_nonce: 0 },
    ),
    ("repay_before_expiry", ExpectedWalletAction { signers: &["borrower"], old_status: 1, new_status: 2, old_nonce: 0, new_nonce: 1 }),
    ("claim_after_expiry", ExpectedWalletAction { signers: &["lender"], old_status: 1, new_status: 3, old_nonce: 0, new_nonce: 1 }),
];

const EXPECTED_PROFILE_OPERATOR_FIXTURES: &[ExpectedOperatorFixture] = &[
    ExpectedOperatorFixture {
        profile: EXPECTED_FUNGIBLE_XUDT_PROFILE,
        action: "issue_xudt",
        fixture: "issue_valid.json",
        signers: &["issuer"],
        live_report: FUNGIBLE_XUDT_LIVE,
        live_tx_hash_pointer: "/issue/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_FUNGIBLE_XUDT_PROFILE,
        action: "transfer_xudt",
        fixture: "transfer_valid.json",
        signers: &["holder"],
        live_report: FUNGIBLE_XUDT_LIVE,
        live_tx_hash_pointer: "/transfer/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_FUNGIBLE_XUDT_PROFILE,
        action: "settle_xudt",
        fixture: "settle_valid.json",
        signers: &["holder"],
        live_report: FUNGIBLE_XUDT_LIVE,
        live_tx_hash_pointer: "/settle/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_RWA_RECEIPT_PROFILE,
        action: "materialize_rwa_receipt",
        fixture: "materialize_valid.json",
        signers: &["issuer"],
        live_report: RWA_RECEIPT_LIVE,
        live_tx_hash_pointer: "/materialize/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_RWA_RECEIPT_PROFILE,
        action: "claim_rwa_receipt",
        fixture: "claim_valid.json",
        signers: &["holder"],
        live_report: RWA_RECEIPT_LIVE,
        live_tx_hash_pointer: "/claim/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_RWA_RECEIPT_PROFILE,
        action: "settle_rwa_receipt",
        fixture: "settle_valid.json",
        signers: &["issuer", "holder"],
        live_report: RWA_RECEIPT_LIVE,
        live_tx_hash_pointer: "/settle/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_BTC_TX_COMMITMENT_PROFILE,
        action: "commit_btc_transaction_transition",
        fixture: "commit_transaction_valid.json",
        signers: &["committer"],
        live_report: BTC_TX_COMMITMENT_LIVE,
        live_tx_hash_pointer: "/commit_transaction/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_BTC_UTXO_SEAL_PROFILE,
        action: "close_btc_utxo_seal",
        fixture: "close_utxo_seal_valid.json",
        signers: &["owner"],
        live_report: BTC_UTXO_SEAL_LIVE,
        live_tx_hash_pointer: "/close_utxo_seal/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_DUAL_SEAL_PROFILE,
        action: "finalize_dual_seal",
        fixture: "finalize_dual_seal_valid.json",
        signers: &["btc_owner", "ckb_authority"],
        live_report: DUAL_SEAL_LIVE,
        live_tx_hash_pointer: "/finalize_dual_seal/commit/tx_hash",
        live_required: true,
        fiber_report: None,
        fiber_required: false,
    },
    ExpectedOperatorFixture {
        profile: EXPECTED_FIBER_CANDIDATE_PROFILE,
        action: "settle_fiber_candidate",
        fixture: "settle_fiber_candidate_valid.json",
        signers: &["operator"],
        live_report: FIBER_CANDIDATE_LIVE,
        live_tx_hash_pointer: "/settle_fiber_candidate/commit/tx_hash",
        live_required: true,
        fiber_report: Some(FIBER_NODE_EXPERIMENTS),
        fiber_required: true,
    },
];

pub fn build_report(repo_root: &Path) -> Result<Value> {
    let core_live = live_verifier_facts(repo_root, CORE_LIVE)?;
    let agreement_live = live_verifier_facts(repo_root, AGREEMENT_LIVE)?;
    let wallet = json_load(repo_root, WALLET_VECTORS)?;
    let wallet_alignment = json_load(repo_root, WALLET_LOCK_ALIGNMENT)?;
    let profile_operator_fixtures = json_load(repo_root, PROFILE_OPERATOR_FIXTURES)?;
    let service_builder_fixtures = json_load(repo_root, SERVICE_BUILDER_FIXTURES)?;
    let btc_spv_evidence_adapter = json_load(repo_root, BTC_SPV_EVIDENCE_ADAPTER)?;
    let external_attestation_adapter = json_load(repo_root, EXTERNAL_ATTESTATION_ADAPTER)?;
    let external_evidence_handoff = json_load(repo_root, EXTERNAL_EVIDENCE_HANDOFF)?;
    let tcb = json_load(repo_root, TCB_REVIEW)?;
    let artifact_hash = normalize_hex(json_pointer_str(&tcb, "/runtime_artifact/artifact_hash"));
    let source_tree_hash = normalize_hex(json_pointer_str(&tcb, "/source_inventory/source_tree_sha256"));
    let tcb_repo_commit = json_pointer_str(&tcb, "/repo_commit");

    let core_manifest = compare_manifest_dep(repo_root, CORE_MANIFEST, &core_live, artifact_hash.as_deref())?;
    let agreement_manifest = compare_manifest_dep(repo_root, AGREEMENT_MANIFEST, &agreement_live, artifact_hash.as_deref())?;
    let public_attestation = validate_public_attestation(
        repo_root,
        PUBLIC_CELLDEP_ATTESTATION,
        artifact_hash.as_deref(),
        tcb_repo_commit,
        &external_evidence_handoff,
    )?;
    let external_review = validate_external_review(
        repo_root,
        EXTERNAL_TCB_ATTESTATION,
        artifact_hash.as_deref(),
        source_tree_hash.as_deref(),
        &external_evidence_handoff,
    )?;
    let btc_spv_evidence = validate_btc_spv_evidence(repo_root, PUBLIC_BTC_SPV_EVIDENCE, &external_evidence_handoff)?;
    let rwa_legal_registry_review =
        validate_rwa_legal_registry_review(repo_root, RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &external_evidence_handoff)?;
    let core_security = validate_core_security_source(repo_root)?;
    let agreement_conformance = validate_agreement_profile_conformance(
        repo_root,
        &repo_root.join(CORE_MANIFEST),
        &repo_root.join(AGREEMENT_MANIFEST),
        &repo_root.join(AGREEMENT_ROOT),
    )?;
    let stateful_acceptance = build_stateful_acceptance_report(repo_root, &agreement_conformance, &btc_spv_evidence)?;
    write_json_report(&repo_root.join(STATEFUL_ACCEPTANCE), &stateful_acceptance)?;
    let profile_certification = validate_profile_certification(ProfileCertificationInputs {
        repo_root,
        agreement_conformance: &agreement_conformance,
        agreement_manifest: &agreement_manifest,
        core_security: &core_security,
        wallet: &wallet,
        wallet_alignment: &wallet_alignment,
        profile_operator_fixtures: &profile_operator_fixtures,
        service_builder_fixtures: &service_builder_fixtures,
        btc_spv_evidence_adapter: &btc_spv_evidence_adapter,
        external_attestation_adapter: &external_attestation_adapter,
        external_evidence_handoff: &external_evidence_handoff,
        stateful_acceptance: &stateful_acceptance,
        tcb: &tcb,
        public_attestation: &public_attestation,
        external_review: &external_review,
        btc_spv_evidence: &btc_spv_evidence,
        rwa_legal_registry_review: &rwa_legal_registry_review,
    })?;
    let profile_production_completeness = build_profile_production_completeness(
        &profile_certification,
        &stateful_acceptance,
        &public_attestation,
        &external_review,
        &btc_spv_evidence,
        &rwa_legal_registry_review,
    );
    let profile_operator_fixture_detail = certification_detail(&profile_certification, "/profile_operator_fixtures");
    let service_builder_fixture_detail = certification_detail(&profile_certification, "/service_builder_fixtures");
    let btc_spv_adapter_detail = certification_detail(&profile_certification, "/btc_spv_evidence_adapter");
    let external_attestation_adapter_detail = certification_detail(&profile_certification, "/external_attestation_adapter");
    let external_evidence_handoff_detail = certification_detail(&profile_certification, "/external_evidence_handoff");

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
            "proposals/novaseal/agreement-profile-v0/Cell.toml + proposals/novaseal/agreement-profile-v0/schemas + proposals/novaseal/agreement-profile-v0/fixtures + target/novaseal-devnet-stateful-acceptance.json + target/novaseal-wallet-signing-vectors.json + proposals/novaseal/v0-mvp-skeleton/target/novaseal-wallet-signing-alignment.json + target/novaseal-profile-operator-fixtures.json + target/novaseal-service-builder-fixtures.json + target/novaseal-external-evidence-handoff-bundle.json",
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
            "wallet_lock_digest_alignment",
            if wallet_lock_alignment_gate_passed(&wallet_alignment) {
                "passed"
            } else {
                "failed"
            },
            WALLET_LOCK_ALIGNMENT,
            wallet_alignment.get("summary").cloned().unwrap_or(Value::Null),
        ),
        gate(
            "planned_profile_operator_fixtures",
            certification_detail_status(&profile_certification, "/profile_operator_fixtures/status"),
            PROFILE_OPERATOR_FIXTURES,
            profile_operator_fixture_detail,
        ),
        gate(
            "planned_profile_service_builder_fixtures",
            certification_detail_status(&profile_certification, "/service_builder_fixtures/status"),
            SERVICE_BUILDER_FIXTURES,
            service_builder_fixture_detail,
        ),
        gate(
            "btc_spv_evidence_adapter_request",
            certification_detail_status(&profile_certification, "/btc_spv_evidence_adapter/status"),
            BTC_SPV_EVIDENCE_ADAPTER,
            btc_spv_adapter_detail,
        ),
        gate(
            "external_attestation_adapter_request",
            certification_detail_status(&profile_certification, "/external_attestation_adapter/status"),
            EXTERNAL_ATTESTATION_ADAPTER,
            external_attestation_adapter_detail,
        ),
        gate(
            "external_evidence_handoff_bundle",
            certification_detail_status(&profile_certification, "/external_evidence_handoff/status"),
            EXTERNAL_EVIDENCE_HANDOFF,
            external_evidence_handoff_detail,
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
            if stateful_local_acceptance_passed(&stateful_acceptance) { "passed" } else { "failed" },
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
            "external_btc_fiber_endpoint_acceptance",
            match json_pointer_str(&stateful_acceptance, "/external_endpoint_coverage/status") {
                Some("passed") => "passed",
                Some("external_required") => "external_required",
                _ => "failed",
            },
            "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage + target/novaseal-fiber-node-experiments.json + proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
            stateful_acceptance.get("external_endpoint_coverage").cloned().unwrap_or(Value::Null),
        ),
        gate(
            "all_profiles_production_completeness",
            match json_pointer_str(&profile_production_completeness, "/status") {
                Some("passed") => "passed",
                Some("external_required") => "external_required",
                _ => "failed",
            },
            "target/novaseal-production-gates.json#/profile_production_completeness",
            profile_production_completeness.clone(),
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
        gate(
            "public_btc_spv_evidence",
            json_pointer_str(&btc_spv_evidence, "/status").unwrap_or("failed"),
            PUBLIC_BTC_SPV_EVIDENCE,
            btc_spv_evidence.clone(),
        ),
        gate(
            "rwa_legal_registry_review_evidence",
            json_pointer_str(&rwa_legal_registry_review, "/status").unwrap_or("failed"),
            RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
            rwa_legal_registry_review.clone(),
        ),
    ];

    let local_ready = gates
        .iter()
        .filter(|row| json_pointer_str(row, "/status") != Some("external_required"))
        .all(|row| json_pointer_str(row, "/status") == Some("passed"));
    let production_gates_passed = gates.iter().all(|row| json_pointer_str(row, "/status") == Some("passed"));
    let production_statement_eligible = json_pointer_bool(&profile_certification, "/production_statement_eligible");
    let production_ready = production_gates_passed && production_statement_eligible;
    let external_required = gates.iter().any(|row| json_pointer_str(row, "/status") == Some("external_required"));
    let status = production_gate_status(production_ready, production_gates_passed, local_ready, external_required);
    let v1_readiness = build_v1_readiness(&profile_certification, &stateful_acceptance, &gates, local_ready, production_gates_passed);
    let failed_dimensions = v1_readiness.get("failed_dimensions").cloned().unwrap_or_else(|| Value::Array(Vec::new()));
    let external_blockers = v1_readiness.get("external_blockers").cloned().unwrap_or_else(|| Value::Array(Vec::new()));

    Ok(json!({
        "schema": "novaseal-production-gates-v0.4",
        "status": status,
        "production_ready": production_ready,
        "production_gates_passed": production_gates_passed,
        "local_production_prep_ready": local_ready,
        "production_statement_eligible": production_statement_eligible,
        "failed_dimensions": failed_dimensions,
        "external_blockers": external_blockers,
        "runtime_artifact_hash": json_pointer_str(&tcb, "/runtime_artifact/artifact_hash").and_then(|value| normalize_hex(Some(value))),
        "conforms_to": {
            "agreement_profile": json_pointer_str(&agreement_conformance, "/conforms_to"),
            "expected": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
            "canonical_schema_hash": json_pointer_str(&agreement_conformance, "/canonical_schema_hash"),
            "status": json_pointer_str(&agreement_conformance, "/status"),
        },
        "profile_certification": profile_certification,
        "profile_production_completeness": profile_production_completeness,
        "v1_readiness": v1_readiness,
        "gates": gates,
        "policy": {
            "no_placeholder_closure": "production remains false until public/shared CellDep, public BTC SPV evidence, RWA legal/registry review evidence, and external TCB attestations are present",
            "attestation_templates": [
                "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json",
                "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json",
            ],
            "external_evidence_templates": [
                "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json",
                "proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.template.json",
            ],
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
        "acceptance_passed": stateful_local_acceptance_passed(stateful_acceptance),
        "no_blockers": local_stateful_blocker_count(stateful_acceptance) == Some(0),
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

fn gate(name: &str, status: &str, evidence: &str, detail: Value) -> Value {
    json!({"name": name, "status": status, "evidence": evidence, "detail": detail})
}

fn certification_detail(profile_certification: &Value, pointer: &str) -> Value {
    profile_certification.pointer(pointer).cloned().unwrap_or(Value::Null)
}

fn certification_detail_status<'a>(profile_certification: &'a Value, pointer: &str) -> &'a str {
    json_pointer_str(profile_certification, pointer).unwrap_or("failed")
}

#[cfg(test)]
fn external_evidence_handoff_gate_passed(report: &Value) -> bool {
    json_pointer_str(report, "/status") == Some("passed")
        && json_pointer_str(report, "/handoff_status") == Some("request_bundle_ready_external_evidence_required")
        && normalize_hex(json_pointer_str(report, "/bundle_hash")).as_deref()
            == Some(external_evidence_handoff_reference_hash(report).as_str())
        && json_pointer_str(report, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM)
        && json_pointer_i64(report, "/summary/total") == Some(4)
        && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total")
}

fn wallet_gate_passed(wallet: &Value) -> bool {
    json_pointer_str(wallet, "/status") == Some("passed")
        && json_pointer_i64(wallet, "/summary/core_vectors").unwrap_or_default() >= 6
        && json_pointer_i64(wallet, "/summary/agreement_vectors").unwrap_or_default() >= 3
        && json_pointer_i64(wallet, "/summary/matched") == json_pointer_i64(wallet, "/summary/total")
}

fn wallet_lock_alignment_gate_passed(alignment: &Value) -> bool {
    json_pointer_str(&validate_wallet_lock_alignment_detail(alignment), "/status") == Some("passed")
}

fn novaseal_handoff_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaExtHandoff").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(cellscript_canonical_json::canonical_json_string(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

fn external_evidence_handoff_reference_hash(value: &Value) -> String {
    let mut payload = value.clone();
    if let Some(object) = payload.as_object_mut() {
        object.remove("bundle_hash");
        object.remove("bundle_hash_algorithm");
    }
    novaseal_handoff_report_hash("external_evidence_handoff_bundle", &payload)
}

#[derive(Debug)]
struct BitcoinTxInput {
    prev_txid: String,
    prev_vout: u64,
}

#[derive(Debug)]
struct BitcoinTxOutput {
    amount_sats: u64,
    script_pubkey: Vec<u8>,
}

#[derive(Debug)]
struct BitcoinTxSummary {
    txid: String,
    wtxid: String,
    inputs: Vec<BitcoinTxInput>,
    outputs: Vec<BitcoinTxOutput>,
}

fn novaseal_profile_operator_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaProfileFxV0").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(cellscript_canonical_json::canonical_json_string(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

fn novaseal_service_builder_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaSvcBuildV0").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(cellscript_canonical_json::canonical_json_string(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

fn novaseal_btc_spv_adapter_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaBtcSpvReqV0").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(cellscript_canonical_json::canonical_json_string(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

fn novaseal_external_attestation_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaExtAttReqV0").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(cellscript_canonical_json::canonical_json_string(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

#[cfg(test)]
fn stateful_acceptance_passed(stateful_acceptance: &Value) -> bool {
    json_pointer_str(stateful_acceptance, "/status") == Some("passed")
        && json_pointer_i64(stateful_acceptance, "/blocker_count") == Some(0)
        && json_pointer_bool(stateful_acceptance, "/live_devnet_rpc_executed")
        && json_pointer_bool(stateful_acceptance, "/stateful_lifecycle_executed")
        && json_pointer_str(stateful_acceptance, "/profile_coverage/status") == Some("passed")
        && json_pointer_str(stateful_acceptance, "/business_scenario_coverage/status") == Some("passed")
}

fn stateful_local_acceptance_passed(stateful_acceptance: &Value) -> bool {
    matches!(json_pointer_str(stateful_acceptance, "/status"), Some("passed" | "local_devnet_passed_external_endpoint_required"))
        && local_stateful_blocker_count(stateful_acceptance) == Some(0)
        && json_pointer_bool(stateful_acceptance, "/live_devnet_rpc_executed")
        && json_pointer_bool(stateful_acceptance, "/stateful_lifecycle_executed")
        && json_pointer_str(stateful_acceptance, "/profile_coverage/status") == Some("passed")
        && json_pointer_str(stateful_acceptance, "/business_scenario_coverage/status") == Some("passed")
}

fn local_stateful_blocker_count(stateful_acceptance: &Value) -> Option<i64> {
    json_pointer_i64(stateful_acceptance, "/local_blocker_count").or_else(|| json_pointer_i64(stateful_acceptance, "/blocker_count"))
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

fn json_pointer_u64(value: &Value, pointer: &str) -> Option<u64> {
    value.pointer(pointer).and_then(Value::as_u64)
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

fn json_object_string_map(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|object| object.iter().filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string()))).collect())
        .unwrap_or_default()
}

fn exact_string_set(actual: &[String], expected: &[&str]) -> bool {
    actual.len() == expected.len() && expected.iter().all(|field| actual.iter().any(|actual| actual == field))
}

fn exact_object_keys(value: &Value, expected: &[&str]) -> bool {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .is_some_and(|actual| exact_string_set(&actual, expected))
}

fn exact_string_map(value: &Value, expected: &[(&str, &str)]) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.len() == expected.len()
        && expected.iter().all(|(key, expected_value)| {
            object.get(*key).and_then(Value::as_str).is_some_and(|actual_value| actual_value == *expected_value)
        })
}

fn safe_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute() && path.components().all(|component| matches!(component, Component::Normal(_)))
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

fn tx_hash_value_is_real(value: &Value) -> bool {
    value.as_str().is_some_and(is_real_tx_hash)
}

fn is_real_tx_hash(value: &str) -> bool {
    is_hex32(value) && !placeholder_hash(Some(value))
}

fn is_hex32(value: &str) -> bool {
    value.len() == 66 && value.starts_with("0x") && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_hex_bytes(value: &str) -> bool {
    value.len() > 2
        && value.len().is_multiple_of(2)
        && value.starts_with("0x")
        && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_hex_bytes_len(value: &str, byte_len: usize) -> bool {
    value.len() == 2 + byte_len * 2 && is_hex_bytes(value)
}

fn hex_bytes_exact(value: &str, byte_len: usize) -> Option<Vec<u8>> {
    if !is_hex_bytes_len(value, byte_len) {
        return None;
    }
    hex::decode(&value[2..]).ok().filter(|bytes| bytes.len() == byte_len)
}

fn hex_bytes(value: &str) -> Option<Vec<u8>> {
    if !is_hex_bytes(value) {
        return None;
    }
    hex::decode(&value[2..]).ok()
}

fn is_git_commit_hash(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_file_hex(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(format!("0x{}", hex::encode(Sha256::digest(bytes))))
}

fn rel(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root).unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests;
