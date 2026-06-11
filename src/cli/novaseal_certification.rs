use crate::error::{CompileError, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

pub(crate) fn build_report(repo_root: &Path) -> Result<Value> {
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

fn build_v1_readiness(
    profile_certification: &Value,
    stateful_acceptance: &Value,
    gates: &[Value],
    local_ready: bool,
    production_gates_passed: bool,
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
            stateful_local_acceptance_passed(stateful_acceptance),
            "target/novaseal-devnet-stateful-acceptance.json",
            "complete local stateful acceptance",
        ),
        readiness_dimension(
            "wallet_signing_vectors",
            json_pointer_bool(profile_certification, "/local_checks/wallet_vector_detail_passed"),
            "target/novaseal-wallet-signing-vectors.json",
            "wallet-facing signing vector safety",
        ),
        readiness_dimension(
            "wallet_lock_digest_alignment",
            json_pointer_bool(profile_certification, "/local_checks/wallet_lock_alignment_passed"),
            "proposals/novaseal/v0-mvp-skeleton/target/novaseal-wallet-signing-alignment.json",
            "wallet, lock, and verifier message32 alignment",
        ),
        readiness_dimension(
            "profile_operator_fixtures",
            json_pointer_bool(profile_certification, "/local_checks/profile_operator_fixture_detail_passed"),
            "target/novaseal-profile-operator-fixtures.json",
            "planned-profile wallet and service reproducibility",
        ),
        readiness_dimension(
            "service_builder_fixtures",
            json_pointer_bool(profile_certification, "/local_checks/service_builder_fixture_detail_passed"),
            "target/novaseal-service-builder-fixtures.json",
            "planned-profile service request and response reproducibility",
        ),
        readiness_dimension(
            "btc_spv_evidence_adapter",
            json_pointer_bool(profile_certification, "/local_checks/btc_spv_evidence_adapter_passed"),
            "target/novaseal-btc-spv-evidence-adapter.json",
            "public BTC SPV evidence request readiness",
        ),
        readiness_dimension(
            "external_attestation_adapter",
            json_pointer_bool(profile_certification, "/local_checks/external_attestation_adapter_passed"),
            "target/novaseal-external-attestation-adapter.json",
            "public CellDep and external TCB attestation request readiness",
        ),
        readiness_dimension(
            "external_evidence_handoff",
            json_pointer_bool(profile_certification, "/local_checks/external_evidence_handoff_passed"),
            "target/novaseal-external-evidence-handoff-bundle.json",
            "external production evidence provider handoff",
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
            "external_btc_fiber_endpoint_acceptance",
            gate_status("external_btc_fiber_endpoint_acceptance") == "passed",
            "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage",
            "complete BTC SPV and Fiber external endpoint acceptance",
        ),
        readiness_dimension(
            "all_profiles_production_completeness",
            gate_status("all_profiles_production_completeness") == "passed",
            "target/novaseal-production-gates.json#/profile_production_completeness",
            "every NovaSeal profile has local lifecycle evidence plus required external production evidence",
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
        readiness_dimension(
            "public_btc_spv_evidence",
            gate_status("public_btc_spv_evidence") == "passed",
            PUBLIC_BTC_SPV_EVIDENCE,
            "public BTC inclusion and confirmation proof provenance",
        ),
        readiness_dimension(
            "rwa_legal_registry_review_evidence",
            gate_status("rwa_legal_registry_review_evidence") == "passed",
            RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
            "external legal and registry review for RWA production claims",
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
        "wallet_lock_digest_alignment",
        "profile_operator_fixtures",
        "service_builder_fixtures",
        "btc_spv_evidence_adapter",
        "external_attestation_adapter",
        "external_evidence_handoff",
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
    let production_ready = production_gates_passed && production_statement_eligible;
    let status = if production_ready {
        "v1_prod_ready"
    } else if production_gates_passed {
        "production_statement_ineligible"
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
        "production_gates_passed": production_gates_passed,
        "production_statement_eligible": production_statement_eligible,
        "planned_profile_matrix": planned_matrix,
        "dimensions": dimensions,
        "failed_dimensions": failed_dimensions,
        "external_blockers": external_blockers,
        "acceptance_boundary": {
            "local_ready_means": "architecture, audit, wallet signing vectors, wallet/lock digest alignment, planned-profile operator fixtures, service-builder fixtures, BTC SPV evidence adapter request, external attestation adapter request, external evidence handoff bundle, TCB, multi-profile devnet, multi-business scenarios, and full stateful acceptance are machine checked locally",
            "production_ready_requires": [
                "all NovaSeal profiles pass the profile production-completeness matrix",
                "complete external BTC SPV and Fiber endpoint acceptance",
                "public/shared CellDep pinning attestation",
                "public BTC SPV evidence for BTC-facing profiles",
                "RWA legal/registry review evidence for RWA receipt production claims",
                "external BIP340 runtime verifier TCB review attestation",
                "cellc certify --plugin novaseal-profile-v0 --require-production passes",
            ],
        },
    })
}

fn production_gate_status(
    production_ready: bool,
    production_gates_passed: bool,
    local_ready: bool,
    external_required: bool,
) -> &'static str {
    if production_ready {
        "production_ready"
    } else if production_gates_passed {
        "production_statement_ineligible"
    } else if local_ready && external_required {
        "local_production_prep_ready_external_attestation_required"
    } else {
        "failed"
    }
}

fn build_planned_profile_matrix(profile_certification: &Value, stateful_acceptance: &Value) -> Value {
    let core_passed = json_pointer_str(stateful_acceptance, "/profile_coverage/covered_profiles/0/status") == Some("passed");
    let agreement_passed = json_pointer_str(stateful_acceptance, "/profile_coverage/covered_profiles/1/status") == Some("passed")
        && json_pointer_bool(profile_certification, "/local_checks/conformance_gate_passed");
    let key_signature_passed = json_pointer_bool(profile_certification, "/local_checks/local_bip340_tcb_review_passed")
        && json_pointer_bool(profile_certification, "/local_checks/wallet_vector_detail_passed")
        && json_pointer_bool(profile_certification, "/local_checks/wallet_lock_alignment_passed");
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
    let dual_seal_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/dual_seal_finality_live");
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
            "target/novaseal-bip340-tcb-review.json + target/novaseal-wallet-signing-vectors.json + proposals/novaseal/v0-mvp-skeleton/target/novaseal-wallet-signing-alignment.json",
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
            dual_seal_package_passed && dual_seal_business_passed,
            "proposals/novaseal/dual-seal-profile-v0 package plus target/novaseal-dual-seal-devnet-stateful-live.json and explicit public BTC closure evidence gap",
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
            "dual_seal_finality",
            "Business scenario",
            "Dual-seal terminal finalisation after CKB maturity with declared BTC closure commitment",
            dual_seal_package_passed && dual_seal_business_passed,
            "target/novaseal-dual-seal-devnet-stateful-live.json",
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
    let remaining_items = missing_profiles.clone();
    let not_implemented_yet = if passed {
        "none; all planned NovaSeal V1 profiles and business scenarios have local package, fixture, and stateful evidence".to_string()
    } else {
        format!("remaining local evidence rows: {}", missing_profiles.join(", "))
    };
    json!({
        "schema": "novaseal-planned-profile-matrix-v0.1",
        "status": if passed { "passed" } else { "incomplete" },
        "source": "proposals/novaseal/v0-mvp-skeleton/NOVASEAL_ARCHITECTURE_EXPLAINED.md",
        "profiles": profiles,
        "business_scenarios": business_scenarios,
        "missing": missing_profiles,
        "boundary": {
            "implemented_now": if passed {
                "BTC key-signature authority, all planned profile packages, key-signed Cell movement, CKB-native Agreement terminal paths, and local stateful live-report evidence for every planned V1 business scenario"
            } else {
                "BTC key-signature authority, implemented profile packages, key-signed Cell movement, CKB-native Agreement terminal paths, and any business scenario rows marked passed in this matrix"
            },
            "not_implemented_yet": not_implemented_yet,
            "remaining_items": remaining_items,
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

fn build_stateful_acceptance_report(repo_root: &Path, agreement_conformance: &Value, btc_spv_evidence: &Value) -> Result<Value> {
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
            "scripts/novaseal_planned_profiles_devnet_stateful_live.py",
            "scripts/novaseal_devnet_stateful_live.py",
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
            "scripts/novaseal_devnet_stateful_live.py",
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
            "scripts/novaseal_devnet_stateful_live.py",
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
            "scripts/novaseal_devnet_stateful_live.py",
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
            "scripts/novaseal_planned_profiles_devnet_stateful_live.py",
            "scripts/novaseal_devnet_stateful_live.py",
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
            "scripts/novaseal_planned_profiles_devnet_stateful_live.py",
            "scripts/novaseal_devnet_stateful_live.py",
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

fn stateful_acceptance_status(
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

fn stateful_live_acceptance_blockers(
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
    let tx_hashes_present = tx_hash_summary.values().all(tx_hash_value_is_real);
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

fn fiber_node_execution_summary(repo_root: &Path, report: Option<&Value>) -> Value {
    let Some(report) = report else {
        return json!({
            "present": false,
            "status": "missing",
            "discovery_ready": false,
            "all_required_workflows_executed_passed": false,
            "required_report": FIBER_NODE_EXPERIMENTS,
        });
    };
    if report.get("_invalid_json").is_some() {
        return json!({
            "present": true,
            "valid_json": false,
            "error": report.get("_invalid_json"),
            "discovery_ready": false,
            "all_required_workflows_executed_passed": false,
            "required_report": FIBER_NODE_EXPERIMENTS,
        });
    }

    let workflows = report.get("workflows").and_then(Value::as_array).cloned().unwrap_or_default();
    let workflow_suites =
        workflows.iter().filter_map(|workflow| json_pointer_str(workflow, "/suite").map(str::to_string)).collect::<Vec<_>>();
    let workflow_suites_exact =
        exact_string_set(&workflow_suites, &EXPECTED_FIBER_WORKFLOWS.iter().map(|(suite, _)| *suite).collect::<Vec<_>>());
    let workflows_by_suite = workflows
        .iter()
        .filter_map(|workflow| json_pointer_str(workflow, "/suite").map(|suite| (suite.to_string(), workflow)))
        .collect::<BTreeMap<_, _>>();
    let duplicate_free_workflow_suites = workflows_by_suite.len() == workflow_suites.len();
    let fiber_repo_path = json_pointer_str(report, "/fiber_repo/path").map(Path::new);
    let fiber_repo_exists = fiber_repo_path.is_some_and(Path::is_dir);
    let fiber_repo_git = fiber_repo_git_provenance(fiber_repo_path, report);
    let fiber_repo_git_verified = json_pointer_bool(&fiber_repo_git, "/verified");

    let mut workflow_checks = Map::new();
    let mut failed_workflows = Vec::new();
    for (suite, expected_profiles) in EXPECTED_FIBER_WORKFLOWS {
        let Some(workflow) = workflows_by_suite.get(*suite) else {
            failed_workflows.push(Value::String((*suite).to_string()));
            workflow_checks.insert((*suite).to_string(), json!({ "present": false }));
            continue;
        };
        let mapped_profiles = json_array_strings(workflow, "/mapped_profiles");
        let evidence_files_present = workflow
            .get("evidence_files")
            .and_then(Value::as_array)
            .is_some_and(|files| !files.is_empty() && files.iter().all(|file| file.as_str().is_some_and(|file| !file.is_empty())));
        let evidence_files_exist =
            fiber_repo_path.is_some_and(|fiber_repo| relative_file_array_all_exist(fiber_repo, workflow.get("evidence_files"), true));
        let rpc_methods_present = workflow.get("rpc_methods").and_then(Value::as_array).is_some_and(|methods| {
            !methods.is_empty() && methods.iter().all(|method| method.as_str().is_some_and(|method| !method.is_empty()))
        });
        let execution_logs_present = value_is_present(workflow.pointer("/execution/stdout_log").unwrap_or(&Value::Null))
            && value_is_present(workflow.pointer("/execution/stderr_log").unwrap_or(&Value::Null));
        let execution_logs_exist = relative_file_exists(repo_root, json_pointer_str(workflow, "/execution/stdout_log"), true)
            && relative_file_exists(repo_root, json_pointer_str(workflow, "/execution/stderr_log"), false);
        let expected_command = json!(["npm", "exec", "--", "@usebruno/cli", "run", format!("e2e/{suite}"), "-r", "--env", "test"]);
        let execution_started_node = json_pointer_bool(workflow, "/execution/started_node");
        let execution_command_exact = workflow.pointer("/execution/command") == Some(&expected_command);
        let execution_returncode_zero = json_pointer_i64(workflow, "/execution/returncode") == Some(0);
        let execution_duration_positive =
            workflow.pointer("/execution/duration_seconds").and_then(Value::as_f64).is_some_and(|duration| duration > 0.0);
        let execution_fiber_repo_matches_report = json_pointer_str(workflow, "/execution/fiber_repo/path")
            .is_some_and(|path| json_pointer_str(report, "/fiber_repo/path") == Some(path))
            && json_pointer_str(workflow, "/execution/fiber_repo/origin")
                .is_some_and(|origin| json_pointer_str(report, "/fiber_repo/origin") == Some(origin))
            && json_pointer_str(workflow, "/execution/fiber_repo/branch")
                .is_some_and(|branch| json_pointer_str(report, "/fiber_repo/branch") == Some(branch))
            && json_pointer_str(workflow, "/execution/fiber_repo/commit")
                .is_some_and(|commit| is_git_commit_hash(commit) && json_pointer_str(report, "/fiber_repo/commit") == Some(commit))
            && json_pointer_bool_opt(workflow, "/execution/fiber_repo/dirty")
                .is_some_and(|dirty| json_pointer_bool_opt(report, "/fiber_repo/dirty") == Some(dirty));
        let bruno_compatibility_patch_files_exist = bruno_compatibility_patch_contract(
            repo_root,
            json_pointer_str(workflow, "/execution/bruno_cwd"),
            workflow.pointer("/execution/bruno_compatibility_patches"),
        );
        let checks = json!({
            "present": json_pointer_bool(workflow, "/present"),
            "status_passed": json_pointer_str(workflow, "/status") == Some("passed"),
            "execution_passed": json_pointer_str(workflow, "/execution/status") == Some("passed"),
            "execution_started_node": execution_started_node,
            "execution_command_exact": execution_command_exact,
            "execution_returncode_zero": execution_returncode_zero,
            "execution_duration_positive": execution_duration_positive,
            "execution_fiber_repo_matches_report": execution_fiber_repo_matches_report,
            "mapped_profiles_exact": exact_string_set(&mapped_profiles, expected_profiles),
            "expected_terms_present": object_values_all_true(workflow.get("expected_terms")),
            "evidence_files_present": evidence_files_present,
            "evidence_files_exist": evidence_files_exist,
            "rpc_methods_present": rpc_methods_present,
            "execution_logs_present": execution_logs_present,
            "execution_logs_exist": execution_logs_exist,
            "bruno_compatibility_patch_files_exist": bruno_compatibility_patch_files_exist,
        });
        if !object_values_all_true(Some(&checks)) {
            failed_workflows.push(Value::String((*suite).to_string()));
        }
        workflow_checks.insert((*suite).to_string(), checks);
    }

    let all_present = json_pointer_bool(report, "/workflow_coverage/all_required_workflows_present");
    let runnable_devnet_contract_present = json_pointer_bool(report, "/devnet_contract/runnable_devnet_contract_present");
    let all_executed_passed_reported = json_pointer_bool(report, "/workflow_coverage/all_required_workflows_executed_passed");
    let partial_execution_passed_reported = json_pointer_bool(report, "/workflow_coverage/partial_execution_passed");
    let profiles_covered = json_array_strings(report, "/profiles_covered");
    let schema_ok = json_pointer_str(report, "/schema") == Some(EXPECTED_FIBER_NODE_EXECUTION_SCHEMA);
    let status_passed = json_pointer_str(report, "/status") == Some("passed");
    let clean_expected_repo = fiber_repo_exists
        && fiber_repo_git_verified
        && json_pointer_str(report, "/fiber_repo/origin") == Some(EXPECTED_FIBER_REPO_ORIGIN)
        && json_pointer_str(report, "/fiber_repo/branch").is_some_and(|branch| !branch.is_empty())
        && json_pointer_str(report, "/fiber_repo/commit")
            .is_some_and(|commit| commit.len() == 40 && commit.chars().all(|char| char.is_ascii_hexdigit()))
        && !json_pointer_bool(report, "/fiber_repo/dirty");
    let required_count = json_pointer_i64(report, "/workflow_coverage/required_count");
    let present_count = json_pointer_i64(report, "/workflow_coverage/present_count");
    let executed_count = json_pointer_i64(report, "/workflow_coverage/executed_count");
    let passed_execution_count = json_pointer_i64(report, "/workflow_coverage/passed_execution_count");
    let expected_workflow_count = EXPECTED_FIBER_WORKFLOWS.len() as i64;
    let count_contract_exact = required_count == Some(expected_workflow_count)
        && present_count == Some(expected_workflow_count)
        && executed_count == Some(expected_workflow_count)
        && passed_execution_count == Some(expected_workflow_count);
    let partial_execution_contract_exact = match (required_count, executed_count, passed_execution_count) {
        (Some(required), Some(executed), Some(passed)) => executed > 0 && executed < required && passed == executed,
        _ => false,
    };
    let reported_partial_execution_semantics =
        !partial_execution_passed_reported || (!all_executed_passed_reported && partial_execution_contract_exact);
    let profiles_exact = exact_string_set(&profiles_covered, EXPECTED_FIBER_NODE_PROFILES);
    let workflow_rows_passed = workflow_suites_exact && duplicate_free_workflow_suites && failed_workflows.is_empty();
    let discovery_ready =
        schema_ok && all_present && runnable_devnet_contract_present && workflow_suites_exact && duplicate_free_workflow_suites;
    let all_executed_passed = discovery_ready
        && status_passed
        && clean_expected_repo
        && count_contract_exact
        && reported_partial_execution_semantics
        && profiles_exact
        && all_executed_passed_reported
        && workflow_rows_passed;
    let checks = json!({
        "schema_ok": schema_ok,
        "status_passed": status_passed,
        "clean_expected_fiber_repo": clean_expected_repo,
        "fiber_repo_exists": fiber_repo_exists,
        "fiber_repo_git_provenance_verified": fiber_repo_git_verified,
        "runnable_devnet_contract_present": runnable_devnet_contract_present,
        "coverage_counts_exact": count_contract_exact,
        "reported_partial_execution_semantics": reported_partial_execution_semantics,
        "profiles_covered_exact": profiles_exact,
        "workflow_suites_exact": workflow_suites_exact,
        "duplicate_free_workflow_suites": duplicate_free_workflow_suites,
        "workflow_rows_passed": workflow_rows_passed,
        "reported_all_required_workflows_present": all_present,
        "reported_all_required_workflows_executed_passed": all_executed_passed_reported,
    });
    json!({
        "present": true,
        "valid_json": true,
        "schema": json_pointer_str(report, "/schema"),
        "status": json_pointer_str(report, "/status"),
        "fiber_repo": {
            "path": json_pointer_str(report, "/fiber_repo/path"),
            "origin": json_pointer_str(report, "/fiber_repo/origin"),
            "branch": json_pointer_str(report, "/fiber_repo/branch"),
            "commit": json_pointer_str(report, "/fiber_repo/commit"),
            "dirty": json_pointer_bool(report, "/fiber_repo/dirty"),
        },
        "fiber_repo_git_provenance": fiber_repo_git,
        "workflow_coverage": report.get("workflow_coverage").cloned().unwrap_or(Value::Null),
        "profiles_covered": report.get("profiles_covered").cloned().unwrap_or(Value::Null),
        "tooling": report.get("tooling").cloned().unwrap_or(Value::Null),
        "checks": checks,
        "workflow_checks": workflow_checks,
        "failed_workflows": failed_workflows,
        "expected_workflows": EXPECTED_FIBER_WORKFLOWS.iter().map(|(suite, _)| *suite).collect::<Vec<_>>(),
        "expected_profiles": EXPECTED_FIBER_NODE_PROFILES,
        "discovery_ready": discovery_ready,
        "partial_execution_passed": partial_execution_passed_reported && partial_execution_contract_exact && !all_executed_passed,
        "all_required_workflows_executed_passed": all_executed_passed,
        "execution_boundary": "discovery_ready is not live Fiber devnet evidence; all_required_workflows_executed_passed requires exact suite/profile coverage, clean Nervos Fiber provenance, runnable devnet tooling, and per-workflow started-node execution with the exact Bruno command, returncode 0, positive duration, and persisted logs",
        "required_report": FIBER_NODE_EXPERIMENTS,
    })
}

fn external_endpoint_coverage_summary(
    btc_spv_evidence: &Value,
    fiber_node_experiments: &Value,
    live_btc_tx_commitment: &Value,
    live_btc_utxo_seal: &Value,
    live_dual_seal: &Value,
    live_fiber_candidate: &Value,
) -> Value {
    let btc_ckb_lifecycle_passed = json_pointer_bool(live_btc_tx_commitment, "/required_live_checks_passed")
        && json_pointer_bool(live_btc_utxo_seal, "/required_live_checks_passed")
        && json_pointer_bool(live_dual_seal, "/required_live_checks_passed");
    let public_btc_spv_status = json_pointer_str(btc_spv_evidence, "/status").unwrap_or("missing");
    let public_btc_spv_passed = public_btc_spv_status == "passed";
    let public_btc_spv_external_required = public_btc_spv_status == "external_required";
    let fiber_ckb_lifecycle_passed = json_pointer_bool(live_fiber_candidate, "/required_live_checks_passed");
    let fiber_git_provenance_verified = json_pointer_bool(fiber_node_experiments, "/fiber_repo_git_provenance/verified");
    let fiber_node_workflows_passed =
        json_pointer_bool(fiber_node_experiments, "/all_required_workflows_executed_passed") && fiber_git_provenance_verified;

    let btc_status = if !btc_ckb_lifecycle_passed {
        "failed"
    } else if public_btc_spv_passed {
        "passed"
    } else if public_btc_spv_external_required {
        "external_required"
    } else {
        "failed"
    };
    let fiber_status = if fiber_ckb_lifecycle_passed && fiber_node_workflows_passed { "passed" } else { "failed" };
    let status = if btc_status == "passed" && fiber_status == "passed" {
        "passed"
    } else if btc_status == "external_required" && fiber_status == "passed" {
        "external_required"
    } else {
        "failed"
    };

    json!({
        "schema": "novaseal-external-endpoint-coverage-v0.1",
        "status": status,
        "btc": {
            "status": btc_status,
            "ckb_profile_lifecycle_passed": btc_ckb_lifecycle_passed,
            "public_spv_evidence_passed": public_btc_spv_passed,
            "public_spv_evidence_status": public_btc_spv_status,
            "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
            "evidence": {
                "status": public_btc_spv_status,
                "required_report": json_pointer_str(btc_spv_evidence, "/required_report"),
                "reason": json_pointer_str(btc_spv_evidence, "/reason"),
                "template": json_pointer_str(btc_spv_evidence, "/template"),
            },
            "boundary": "CKB live lifecycle evidence only proves the NovaSeal BTC-facing profile transitions and BIP340 intent verification. Production requires public BTC SPV evidence for inclusion, spend validity, and confirmation depth.",
        },
        "fiber": {
            "status": fiber_status,
            "ckb_profile_lifecycle_passed": fiber_ckb_lifecycle_passed,
            "all_required_workflows_executed_passed": fiber_node_workflows_passed,
            "workflow_coverage": fiber_node_experiments.get("workflow_coverage").cloned().unwrap_or(Value::Null),
            "fiber_repo": fiber_node_experiments.get("fiber_repo").cloned().unwrap_or(Value::Null),
            "fiber_repo_git_provenance": fiber_node_experiments.get("fiber_repo_git_provenance").cloned().unwrap_or(Value::Null),
            "boundary": "Fiber coverage requires the NovaSeal CKB candidate lifecycle and the separate Nervos Fiber devnet Bruno workflow suite to pass.",
        },
        "checks": {
            "btc_ckb_lifecycle_passed": btc_ckb_lifecycle_passed,
            "public_btc_spv_evidence_passed": public_btc_spv_passed,
            "fiber_ckb_lifecycle_passed": fiber_ckb_lifecycle_passed,
            "fiber_git_provenance_verified": fiber_git_provenance_verified,
            "fiber_node_workflows_passed": fiber_node_workflows_passed,
        },
        "production_complete": status == "passed",
        "boundary": "This is stricter than local stateful CKB acceptance: BTC and Fiber external endpoint evidence is reported explicitly and production fails closed while public BTC SPV evidence is missing.",
    })
}

fn build_profile_production_completeness(
    profile_certification: &Value,
    stateful_acceptance: &Value,
    public_attestation: &Value,
    external_review: &Value,
    btc_spv_evidence: &Value,
    rwa_legal_registry_review: &Value,
) -> Value {
    let public_cell_dep_passed = json_pointer_str(public_attestation, "/status") == Some("passed");
    let external_tcb_passed = json_pointer_str(external_review, "/status") == Some("passed");
    let public_btc_spv_passed = json_pointer_str(btc_spv_evidence, "/status") == Some("passed");
    let rwa_legal_passed = json_pointer_str(rwa_legal_registry_review, "/status") == Some("passed");
    let fiber_endpoint_passed = json_pointer_str(stateful_acceptance, "/external_endpoint_coverage/fiber/status") == Some("passed");

    let common_external = [
        ("public_shared_cell_dep_attestation", public_cell_dep_passed),
        ("external_bip340_tcb_review_attestation", external_tcb_passed),
    ];
    let btc_external = [
        ("public_shared_cell_dep_attestation", public_cell_dep_passed),
        ("external_bip340_tcb_review_attestation", external_tcb_passed),
        ("public_btc_spv_evidence", public_btc_spv_passed),
    ];
    let rwa_external = [
        ("public_shared_cell_dep_attestation", public_cell_dep_passed),
        ("external_bip340_tcb_review_attestation", external_tcb_passed),
        ("rwa_legal_registry_review_evidence", rwa_legal_passed),
    ];
    let fiber_external = [
        ("public_shared_cell_dep_attestation", public_cell_dep_passed),
        ("external_bip340_tcb_review_attestation", external_tcb_passed),
        ("fiber_node_workflow_execution", fiber_endpoint_passed),
    ];

    let rows = vec![
        profile_production_row(
            "novaseal-core-v0",
            "core",
            json_pointer_bool(stateful_acceptance, "/profile_coverage/checks/core_profile_live_stateful"),
            &common_external,
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage/checks/core_profile_live_stateful",
        ),
        profile_production_row(
            "agreement-profile-v0",
            "object",
            json_pointer_bool(stateful_acceptance, "/profile_coverage/checks/agreement_profile_live_stateful")
                && json_pointer_bool(profile_certification, "/local_checks/conformance_gate_passed"),
            &common_external,
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage/checks/agreement_profile_live_stateful",
        ),
        profile_production_row(
            "fungible-xudt-profile-v0",
            "object",
            json_pointer_str(profile_certification, "/planned_profile_packages/fungible_xudt/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/fungible_xudt_value_flow_live"),
            &common_external,
            "target/novaseal-fungible-xudt-devnet-stateful-live.json",
        ),
        profile_production_row(
            "rwa-receipt-profile-v0",
            "object",
            json_pointer_str(profile_certification, "/planned_profile_packages/rwa_receipt/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/rwa_receipt_lifecycle_live"),
            &rwa_external,
            "target/novaseal-rwa-receipt-devnet-stateful-live.json + proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.json",
        ),
        profile_production_row(
            "btc-transaction-commitment-profile-v0",
            "seal",
            json_pointer_str(profile_certification, "/planned_profile_packages/btc_tx_commitment/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/btc_transaction_commitment_transition_live"),
            &btc_external,
            "target/novaseal-btc-transaction-commitment-devnet-stateful-live.json + proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
        ),
        profile_production_row(
            "btc-utxo-seal-profile-v0",
            "seal",
            json_pointer_str(profile_certification, "/planned_profile_packages/btc_utxo_seal/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/btc_utxo_seal_closure_live"),
            &btc_external,
            "target/novaseal-btc-utxo-seal-devnet-stateful-live.json + proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
        ),
        profile_production_row(
            "dual-seal-profile-v0",
            "seal",
            json_pointer_str(profile_certification, "/planned_profile_packages/dual_seal/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/dual_seal_finality_live"),
            &btc_external,
            "target/novaseal-dual-seal-devnet-stateful-live.json + proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
        ),
        profile_production_row(
            "fiber-candidate-profile-v0",
            "application",
            json_pointer_str(profile_certification, "/planned_profile_packages/fiber_candidate/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/fiber_candidate_path_live"),
            &fiber_external,
            "target/novaseal-fiber-candidate-devnet-stateful-live.json + target/novaseal-fiber-node-experiments.json",
        ),
    ];

    let local_complete = rows.iter().all(|row| json_pointer_str(row, "/local_status") == Some("passed"));
    let production_complete = rows.iter().all(|row| json_pointer_str(row, "/status") == Some("passed"));
    let external_required = rows.iter().any(|row| json_pointer_str(row, "/status") == Some("external_required"));
    let status = if production_complete {
        "passed"
    } else if local_complete && external_required {
        "external_required"
    } else {
        "failed"
    };
    let missing_external_evidence = rows
        .iter()
        .flat_map(|row| json_array_strings(row, "/missing_external_evidence"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let failed_profiles = rows
        .iter()
        .filter(|row| json_pointer_str(row, "/status") != Some("passed"))
        .filter_map(|row| json_pointer_str(row, "/profile").map(str::to_string))
        .collect::<Vec<_>>();

    json!({
        "schema": "novaseal-all-profiles-production-completeness-v0.1",
        "status": status,
        "local_complete": local_complete,
        "production_complete": production_complete,
        "missing_external_evidence": missing_external_evidence,
        "failed_profiles": failed_profiles,
        "profiles": rows,
        "boundary": "Each NovaSeal profile must have local package/lifecycle evidence and every profile-specific external production proof before production_ready can be true.",
    })
}

fn profile_production_row(
    profile: &str,
    category: &str,
    local_passed: bool,
    external_requirements: &[(&str, bool)],
    evidence: &str,
) -> Value {
    let missing_external_evidence = external_requirements
        .iter()
        .filter(|(_, passed)| !*passed)
        .map(|(name, _)| Value::String((*name).to_string()))
        .collect::<Vec<_>>();
    let external_requirements =
        external_requirements.iter().map(|(name, passed)| json!({"name": name, "passed": passed})).collect::<Vec<_>>();
    let status = if !local_passed {
        "failed"
    } else if missing_external_evidence.is_empty() {
        "passed"
    } else {
        "external_required"
    };

    json!({
        "profile": profile,
        "category": category,
        "status": status,
        "local_status": if local_passed { "passed" } else { "failed" },
        "external_requirements": external_requirements,
        "missing_external_evidence": missing_external_evidence,
        "evidence": evidence,
    })
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
        "bootstrap_state_cell_live": json_pointer_bool_opt(report, "/bootstrap/state_cell_live"),
        "transition_tx_hash": json_pointer_str(&transition, "/commit/tx_hash"),
        "old_state_not_live": json_pointer_bool_opt(&transition, "/old_state_not_live"),
        "new_state_live": json_pointer_bool_opt(&transition, "/new_state_live"),
        "receipt_live": json_pointer_bool_opt(&transition, "/receipt_live"),
        "wrong_signature_rejected": negative_case_matched(report, "wrong_signature_dry_run"),
        "post_negative_state_still_live": json_pointer_bool_opt(report, "/negative_cases/post_negative_state_still_live"),
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

fn summary_tx_hashes_real(summary: &Value, fields: &[&str]) -> bool {
    fields.iter().all(|field| summary.get(*field).is_some_and(tx_hash_value_is_real))
}

fn core_live_summary_passed(live_core: &Value) -> bool {
    json_pointer_str(live_core, "/status") == Some("passed")
        && json_pointer_bool(live_core, "/live_devnet_rpc_executed")
        && json_pointer_bool(live_core, "/stateful_lifecycle_executed")
        && json_pointer_bool(live_core, "/provenance_freshness_matched")
        && summary_tx_hashes_real(live_core, &["bootstrap_tx_hash", "transition_tx_hash"])
        && json_pointer_bool(live_core, "/bootstrap_state_cell_live")
        && json_pointer_bool(live_core, "/old_state_not_live")
        && json_pointer_bool(live_core, "/new_state_live")
        && json_pointer_bool(live_core, "/receipt_live")
        && json_pointer_bool(live_core, "/wrong_signature_rejected")
        && json_pointer_bool(live_core, "/post_negative_state_still_live")
}

fn agreement_live_summary_passed(live_agreement: &Value, agreement_conformance: &Value) -> bool {
    json_pointer_str(live_agreement, "/status") == Some("passed")
        && json_pointer_bool(live_agreement, "/live_devnet_rpc_executed")
        && json_pointer_bool(live_agreement, "/stateful_lifecycle_executed")
        && json_pointer_bool(live_agreement, "/provenance_freshness_matched")
        && summary_tx_hashes_real(live_agreement, &["originate_tx_hash", "repay_tx_hash", "claim_originate_tx_hash", "claim_tx_hash"])
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
        .all(|key| json_pointer_bool(live_agreement, &format!("/{key}")))
        && json_pointer_str(agreement_conformance, "/status") == Some("passed")
}

fn provenance_summary(report: &Value, repo_root: &Path, source_paths: &[&str]) -> Result<Value> {
    let provenance = report.get("provenance").cloned().unwrap_or(Value::Null);
    let recorded_source = provenance.get("source_tree").cloned().unwrap_or(Value::Null);
    let current_source = source_tree_hash(repo_root, source_paths)?;
    let recorded_source_hash = json_pointer_str(&recorded_source, "/sha256");
    let current_source_hash = json_pointer_str(&current_source, "/sha256");
    let recorded_source_valid =
        recorded_source.get("valid").and_then(Value::as_bool).unwrap_or_else(|| recorded_source_hash.is_some());
    let current_source_valid = json_pointer_bool(&current_source, "/valid");
    let source_hash_matches =
        recorded_source_valid && current_source_valid && recorded_source_hash.is_some() && recorded_source_hash == current_source_hash;
    let mut artifact_checks = Map::new();
    let recorded_artifacts = provenance.get("artifacts").cloned().unwrap_or(Value::Null);
    let canonical_repo_root = repo_root.canonicalize()?;
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
        let canonical_path =
            path.as_ref().map(|path| safe_regular_file_within_root(&canonical_repo_root, path)).transpose()?.flatten();
        let current_sha = canonical_path.as_ref().map(|path| sha256_file_hex(path)).transpose()?;
        artifact_checks.insert(
            name.to_string(),
            json!({
                "present": artifact.is_object(),
                "path": raw_path,
                "exists": canonical_path.is_some(),
                "regular_file_within_repo": canonical_path.is_some(),
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
    let repo_commit_matches = json_pointer_str(&provenance, "/repo_commit") == current_commit.as_deref();
    Ok(json!({
        "present": provenance.is_object(),
        "freshness_matched": source_hash_matches && artifact_hashes_match && repo_commit_matches,
        "repo_commit": json_pointer_str(&provenance, "/repo_commit"),
        "current_repo_commit": current_commit,
        "repo_commit_matches": repo_commit_matches,
        "source_hash_matches": source_hash_matches,
        "recorded_source_valid": recorded_source_valid,
        "current_source_valid": current_source_valid,
        "recorded_source_hash": recorded_source_hash,
        "current_source_hash": current_source_hash,
        "recorded_file_count": recorded_source.get("file_count").and_then(Value::as_u64),
        "current_file_count": current_source.get("file_count").and_then(Value::as_u64),
        "current_source_invalid_paths": current_source.get("invalid_paths").cloned().unwrap_or(Value::Null),
        "artifact_hashes_match": artifact_hashes_match,
        "artifacts": artifact_checks,
    }))
}

fn source_tree_hash(repo_root: &Path, paths: &[&str]) -> Result<Value> {
    source_tree_hash_with_options(repo_root, paths, false)
}

fn source_tree_hash_with_options(repo_root: &Path, paths: &[&str], include_markdown: bool) -> Result<Value> {
    let canonical_repo_root = repo_root.canonicalize()?;
    let mut files = BTreeSet::new();
    let mut invalid_paths = BTreeSet::new();
    for raw_path in paths {
        let path = repo_root.join(raw_path);
        let Some(metadata) = symlink_metadata_optional(&path)? else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            invalid_paths.insert(rel(repo_root, &path));
        } else if metadata.is_file() {
            if safe_regular_file_within_root(&canonical_repo_root, &path)?.is_some() {
                files.insert(path);
            } else {
                invalid_paths.insert(rel(repo_root, &path));
            }
        } else if metadata.is_dir() {
            if safe_directory_within_root(&canonical_repo_root, &path)?.is_some() {
                collect_source_tree_files(
                    repo_root,
                    &canonical_repo_root,
                    &path,
                    &path,
                    include_markdown,
                    &mut files,
                    &mut invalid_paths,
                )?;
            } else {
                invalid_paths.insert(rel(repo_root, &path));
            }
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
    let invalid_rows = invalid_paths.into_iter().collect::<Vec<_>>();
    Ok(json!({
        "sha256": if invalid_rows.is_empty() { Value::String(format!("0x{}", hex::encode(hasher.finalize()))) } else { Value::Null },
        "files": rows,
        "file_count": rows.len(),
        "invalid_paths": invalid_rows,
        "valid": invalid_rows.is_empty(),
    }))
}

fn collect_source_tree_files(
    repo_root: &Path,
    canonical_repo_root: &Path,
    root: &Path,
    path: &Path,
    include_markdown: bool,
    files: &mut BTreeSet<PathBuf>,
    invalid_paths: &mut BTreeSet<String>,
) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        let relative_parts = child.strip_prefix(root).unwrap_or(&child).components().map(|part| part.as_os_str().to_string_lossy());
        if relative_parts.clone().any(|part| matches!(part.as_ref(), "target" | "build" | ".git" | "__pycache__")) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&child)?;
        if metadata.file_type().is_symlink() {
            invalid_paths.insert(rel(repo_root, &child));
        } else if metadata.is_dir() {
            if safe_directory_within_root(canonical_repo_root, &child)?.is_some() {
                collect_source_tree_files(repo_root, canonical_repo_root, root, &child, include_markdown, files, invalid_paths)?;
            } else {
                invalid_paths.insert(rel(repo_root, &child));
            }
        } else if metadata.is_file() && source_tree_file_allowed(&child, include_markdown) {
            if safe_regular_file_within_root(canonical_repo_root, &child)?.is_some() {
                files.insert(child);
            } else {
                invalid_paths.insert(rel(repo_root, &child));
            }
        }
    }
    Ok(())
}

fn source_tree_file_allowed(path: &Path, include_markdown: bool) -> bool {
    path.file_name().is_some_and(|name| name == "Cargo.lock")
        || (include_markdown && path.file_name().is_some_and(|name| name == "README.md"))
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
        "state_action_binds_sig_pubkey_to_old_cell_authority": source.contains("require sig.pubkey == old_cell.btc_authority_hash.0"),
        "lifecycle_binds_sig_pubkey_to_old_cell_authority": source.contains("assert(sig.pubkey == old_cell.btc_authority_hash.0"),
        "lock_binds_sig_pubkey_to_cell_authority_in_both_lock_surfaces": source.matches("require sig.pubkey == cell.btc_authority_hash.0").count() >= 2,
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
    wallet_alignment: &'a Value,
    profile_operator_fixtures: &'a Value,
    service_builder_fixtures: &'a Value,
    btc_spv_evidence_adapter: &'a Value,
    external_attestation_adapter: &'a Value,
    external_evidence_handoff: &'a Value,
    stateful_acceptance: &'a Value,
    tcb: &'a Value,
    public_attestation: &'a Value,
    external_review: &'a Value,
    btc_spv_evidence: &'a Value,
    rwa_legal_registry_review: &'a Value,
}

fn validate_profile_certification(input: ProfileCertificationInputs<'_>) -> Result<Value> {
    let ProfileCertificationInputs {
        repo_root,
        agreement_conformance,
        agreement_manifest,
        core_security,
        wallet,
        wallet_alignment,
        profile_operator_fixtures,
        service_builder_fixtures,
        btc_spv_evidence_adapter,
        external_attestation_adapter,
        external_evidence_handoff,
        stateful_acceptance,
        tcb,
        public_attestation,
        external_review,
        btc_spv_evidence,
        rwa_legal_registry_review,
    } = input;
    let schema_files = expected_files(repo_root, &repo_root.join(AGREEMENT_ROOT).join("schemas"), EXPECTED_AGREEMENT_SCHEMA_FILES)?;
    let fixture_files = expected_files(repo_root, &repo_root.join(AGREEMENT_ROOT).join("fixtures"), EXPECTED_AGREEMENT_FIXTURES)?;
    let wallet_detail = validate_wallet_vector_detail(wallet);
    let wallet_alignment_detail = validate_wallet_lock_alignment_detail(wallet_alignment);
    let profile_operator_fixture_detail = validate_profile_operator_fixture_detail(repo_root, profile_operator_fixtures)?;
    let service_builder_fixture_detail = validate_service_builder_fixture_detail(service_builder_fixtures, profile_operator_fixtures);
    let public_btc_spv_template = json_load_path(repo_root, &repo_root.join(PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE))?;
    let public_cell_dep_template = json_load_path(repo_root, &repo_root.join(PUBLIC_CELLDEP_ATTESTATION_TEMPLATE))?;
    let external_tcb_template = json_load_path(repo_root, &repo_root.join(EXTERNAL_TCB_ATTESTATION_TEMPLATE))?;
    let btc_spv_adapter_detail = validate_btc_spv_evidence_adapter_detail_with_sources(
        btc_spv_evidence_adapter,
        Some(service_builder_fixtures),
        Some(&public_btc_spv_template),
    );
    let external_attestation_adapter_detail = validate_external_attestation_adapter_detail_with_sources(
        external_attestation_adapter,
        Some(tcb),
        Some(&public_cell_dep_template),
        Some(&external_tcb_template),
    );
    let external_evidence_handoff_detail = validate_external_evidence_handoff_detail(
        repo_root,
        external_evidence_handoff,
        btc_spv_evidence_adapter,
        external_attestation_adapter,
    );
    let invariant_matrix = validate_invariant_matrix(repo_root, &repo_root.join(AGREEMENT_ROOT).join("proofs/invariant_matrix.json"))?;
    let fungible_xudt_profile = validate_fungible_xudt_profile_package(repo_root)?;
    let rwa_receipt_profile = validate_rwa_receipt_profile_package(repo_root)?;
    let btc_tx_commitment_profile = validate_btc_tx_commitment_profile_package(repo_root)?;
    let btc_utxo_seal_profile = validate_btc_utxo_seal_profile_package(repo_root)?;
    let dual_seal_profile = validate_dual_seal_profile_package(repo_root)?;
    let fiber_candidate_profile = validate_fiber_candidate_profile_package(repo_root)?;
    let live_evidence = agreement_live_evidence(stateful_acceptance);
    let fiber_node_experiments = stateful_acceptance
        .get("external_experiment_coverage")
        .and_then(|coverage| coverage.get("fiber_node_execution"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "present": false,
                "discovery_ready": false,
                "all_required_workflows_executed_passed": false,
                "required_report": FIBER_NODE_EXPERIMENTS,
            })
        });
    let artifact_hash = normalize_hex(json_pointer_str(tcb, "/runtime_artifact/artifact_hash"));
    let artifact_hash_algorithm = json_pointer_str(tcb, "/runtime_artifact/artifact_hash_algorithm");
    let source_tree_hash = normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256"));
    let attestation_templates =
        validate_attestation_templates(repo_root, artifact_hash.as_deref(), artifact_hash_algorithm, source_tree_hash.as_deref())?;
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
        "public_btc_spv_evidence_attested": json_pointer_str(btc_spv_evidence, "/status") == Some("passed"),
        "rwa_legal_registry_review_attested": json_pointer_str(rwa_legal_registry_review, "/status") == Some("passed"),
    });
    let local_checks = json!({
        "conformance_gate_passed": json_pointer_str(agreement_conformance, "/status") == Some("passed"),
        "profile_schema_set_exact": json_pointer_bool(&schema_files, "/exact"),
        "profile_fixture_set_exact": json_pointer_bool(&fixture_files, "/exact"),
        "wallet_vector_detail_passed": json_pointer_str(&wallet_detail, "/status") == Some("passed"),
        "wallet_lock_alignment_passed": json_pointer_str(&wallet_alignment_detail, "/status") == Some("passed"),
        "profile_operator_fixture_detail_passed": json_pointer_str(&profile_operator_fixture_detail, "/status") == Some("passed"),
        "service_builder_fixture_detail_passed": json_pointer_str(&service_builder_fixture_detail, "/status") == Some("passed"),
        "btc_spv_evidence_adapter_passed": json_pointer_str(&btc_spv_adapter_detail, "/status") == Some("passed"),
        "external_attestation_adapter_passed": json_pointer_str(&external_attestation_adapter_detail, "/status") == Some("passed"),
        "external_evidence_handoff_passed": json_pointer_str(&external_evidence_handoff_detail, "/status") == Some("passed"),
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
        "external_fiber_node_experiments_passed": json_pointer_bool(&fiber_node_experiments, "/all_required_workflows_executed_passed"),
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
        "public_btc_spv_evidence": btc_spv_evidence,
        "rwa_legal_registry_review": rwa_legal_registry_review,
        "schema_files": schema_files,
        "fixture_files": fixture_files,
        "wallet_vectors": wallet_detail,
        "wallet_lock_alignment": wallet_alignment_detail,
        "profile_operator_fixtures": profile_operator_fixture_detail,
        "service_builder_fixtures": service_builder_fixture_detail,
        "btc_spv_evidence_adapter": btc_spv_adapter_detail,
        "external_attestation_adapter": external_attestation_adapter_detail,
        "external_evidence_handoff": external_evidence_handoff_detail,
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
        "external_fiber_node_experiments": fiber_node_experiments,
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

fn validate_wallet_lock_alignment_detail(alignment: &Value) -> Value {
    let fixtures = alignment.get("fixtures").and_then(Value::as_array).cloned().unwrap_or_default();
    let fixture_count = json_pointer_i64(alignment, "/summary/fixtures");
    let fixture_details_passed = fixtures.iter().all(|fixture| {
        json_pointer_bool(fixture, "/canonical_vs_current_lock_digest_match")
            && json_pointer_str(fixture, "/current_lock_message_rule")
                == Some("hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })")
            && json_pointer_str(fixture, "/canonical_wallet_message_rule") == Some("signed_intent_hash_after_resolved_receipt")
            && json_pointer_bool(fixture, "/canonical_wallet_positive/self_verified")
            && json_pointer_bool(fixture, "/current_lock_compat_positive/self_verified")
            && json_pointer_bool(fixture, "/cross_check/canonical_signature_accepts_current_lock_digest")
            && json_pointer_bool(fixture, "/cross_check/current_lock_signature_accepts_canonical_digest")
            && json_pointer_i64(fixture, "/resolved_intent_size_bytes") == Some(254)
    });
    let checks = json!({
        "schema_current": json_pointer_str(alignment, "/schema") == Some("novaseal-wallet-signing-alignment-v0.2"),
        "classification_current": json_pointer_str(alignment, "/classification")
            == Some("wallet_signing_vectors_and_lock_digest_alignment_probe"),
        "exact_fixture_count": fixture_count == Some(11) && fixtures.len() == 11,
        "source_model_uses_packed_intent": json_pointer_bool(alignment, "/source_digest_model/all_required_snippets_present")
            && json_pointer_bool(alignment, "/source_digest_model/state_type_uses_packed_signed_intent_hash")
            && json_pointer_bool(alignment, "/source_digest_model/state_type_verifier_uses_signed_intent_hash")
            && json_pointer_bool(alignment, "/source_digest_model/package_lock_uses_packed_digest")
            && json_pointer_bool(alignment, "/source_digest_model/standalone_lock_uses_packed_digest")
            && !json_pointer_bool(alignment, "/source_digest_model/legacy_domain_hash_visible"),
        "summary_alignment_ready": json_pointer_bool(alignment, "/summary/wallet_lock_alignment_ready"),
        "production_wallet_ready": json_pointer_bool(alignment, "/summary/production_wallet_ready"),
        "summary_digest_counts_match": json_pointer_i64(alignment, "/summary/current_lock_digest_matches_canonical") == fixture_count
            && json_pointer_i64(alignment, "/summary/current_lock_digest_mismatches") == Some(0),
        "summary_signature_counts_match": json_pointer_i64(alignment, "/summary/canonical_wallet_vectors_self_verified") == fixture_count
            && json_pointer_i64(alignment, "/summary/current_lock_compat_vectors_self_verified") == fixture_count
            && json_pointer_i64(alignment, "/summary/canonical_wallet_signatures_accepted_by_current_lock_digest") == fixture_count
            && json_pointer_i64(alignment, "/summary/current_lock_signatures_accepted_by_canonical_wallet_digest") == fixture_count,
        "fixture_details_passed": fixture_details_passed,
    });
    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "summary": alignment.get("summary").cloned().unwrap_or(Value::Null),
        "source_digest_model": alignment.get("source_digest_model").cloned().unwrap_or(Value::Null),
    })
}

fn validate_profile_operator_fixture_detail(repo_root: &Path, report: &Value) -> Result<Value> {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_profile_action: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let (Some(profile), Some(action)) = (json_pointer_str(case, "/profile"), json_pointer_str(case, "/action")) {
            by_profile_action.entry((profile.to_string(), action.to_string())).or_default().push(case.clone());
        }
    }

    let expected_profiles =
        EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().map(|fixture| fixture.profile.to_string()).collect::<BTreeSet<_>>();
    let expected_actions = EXPECTED_PROFILE_OPERATOR_FIXTURES
        .iter()
        .map(|fixture| format!("{}:{}", fixture.profile, fixture.action))
        .collect::<BTreeSet<_>>();
    let actual_profiles =
        cases.iter().filter_map(|case| json_pointer_str(case, "/profile").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let actual_actions = cases
        .iter()
        .filter_map(|case| Some(format!("{}:{}", json_pointer_str(case, "/profile")?, json_pointer_str(case, "/action")?)))
        .collect::<BTreeSet<_>>();

    let mut case_checks = Map::new();
    for expected in EXPECTED_PROFILE_OPERATOR_FIXTURES {
        let matches = by_profile_action.get(&(expected.profile.to_string(), expected.action.to_string())).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let display = case.get("wallet_display").cloned().unwrap_or(Value::Null);
        let live_report = json_load_path(repo_root, &repo_root.join(expected.live_report))?;
        let expected_live_report_hash = novaseal_profile_operator_report_hash(expected.live_report, &live_report);
        let expected_live_tx_hash = json_pointer_str(&live_report, expected.live_tx_hash_pointer);
        let public_btc_anchor_pointer = expected_public_btc_anchor_pointer(expected.profile);
        let public_btc_anchor_required = public_btc_anchor_pointer.is_some();
        let expected_public_btc_anchor = public_btc_anchor_pointer.and_then(|pointer| live_report.pointer(pointer));
        let expected_public_btc_commitment_hash =
            expected_public_btc_commitment_hash_pointer(expected.profile).and_then(|pointer| json_pointer_str(&live_report, pointer));
        let case_public_btc_anchor = case.pointer("/public_btc_anchor");
        let display_public_btc_anchor = display.pointer("/public_btc_anchor");
        let case_public_btc_commitment_hash =
            case_public_btc_anchor.and_then(|anchor| json_pointer_str(anchor, "/ckb_btc_commitment_hash"));
        let expected_fiber_report_hash = expected
            .fiber_report
            .map(|path| {
                json_load_path(repo_root, &repo_root.join(path)).map(|report| novaseal_profile_operator_report_hash(path, &report))
            })
            .transpose()?;
        let checks = json!({
            "exactly_one_fixture": matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "fixture_matches": json_pointer_str(&case, "/fixture") == Some(expected.fixture),
            "signers_match": json_array_strings(&case, "/signers") == expected.signers,
            "signed_type_named": json_pointer_str(&case, "/signed_type").is_some_and(|value| value.starts_with("Nova") && value.ends_with("SignedIntentV0")),
            "signed_intent_hash": json_pointer_str(&case, "/signed_intent_hash").is_some_and(is_hex32),
            "bip340_message_hash_matches": json_pointer_str(&case, "/bip340_message_hash") == json_pointer_str(&case, "/signed_intent_hash"),
            "signed_intent_body_hex": json_pointer_str(&case, "/signed_intent_body_hex").is_some_and(is_hex_bytes),
            "signed_intent_preimage_hex": json_pointer_str(&case, "/signed_intent_hash_preimage_hex").is_some_and(is_hex_bytes),
            "witness_shape_hash": json_pointer_str(&case, "/witness_shape_hash").is_some_and(is_hex32),
            "tx_skeleton_hash": json_pointer_str(&case, "/tx_skeleton_hash").is_some_and(is_hex32),
            "fixture_hash": json_pointer_str(&case, "/fixture_hash").is_some_and(is_hex32),
            "source_tree_hash": json_pointer_str(&case, "/source_tree_hash").is_some_and(is_hex32),
            "schema_set_hash": json_pointer_str(&case, "/schema_set_hash").is_some_and(is_hex32),
            "proof_matrix_hash": json_pointer_str(&case, "/proof_matrix_hash").is_some_and(is_hex32),
            "display_profile_matches": json_pointer_str(&display, "/profile") == Some(expected.profile),
            "display_action_matches": json_pointer_str(&display, "/action") == Some(expected.action),
            "live_evidence_present_when_required": !expected.live_required
                || json_pointer_str(&case, "/live_report_hash").is_some_and(is_hex32),
            "live_report_hash_matches_current_report": !expected.live_required
                || json_pointer_str(&case, "/live_report_hash") == Some(expected_live_report_hash.as_str()),
            "live_tx_hash_present_when_required": !expected.live_required
                || json_pointer_str(&case, "/live_devnet_tx_hash").is_some_and(is_real_tx_hash),
            "live_tx_hash_matches_current_report": !expected.live_required
                || json_pointer_str(&case, "/live_devnet_tx_hash") == expected_live_tx_hash,
            "display_live_tx_hash_matches_current_report": !expected.live_required
                || json_pointer_str(&display, "/live_devnet_tx_hash") == expected_live_tx_hash,
            "public_btc_anchor_empty_when_not_required": public_btc_anchor_required
                || case_public_btc_anchor.is_none_or(Value::is_null),
            "public_btc_anchor_present_when_required": !public_btc_anchor_required
                || case_public_btc_anchor.is_some_and(|anchor| anchor.is_object()),
            "public_btc_anchor_shape_matches_profile": !public_btc_anchor_required
                || public_btc_anchor_shape_matches_profile(expected.profile, case_public_btc_anchor),
            "public_btc_anchor_matches_current_report": !public_btc_anchor_required
                || case_public_btc_anchor == expected_public_btc_anchor,
            "public_btc_anchor_commitment_matches_current_report": !public_btc_anchor_required
                || case_public_btc_commitment_hash == expected_public_btc_commitment_hash,
            "display_public_btc_anchor_matches_case": !public_btc_anchor_required
                || display_public_btc_anchor == case_public_btc_anchor,
            "external_boundary_documented_when_not_live": expected.live_required
                || json_pointer_str(&display, "/external_boundary") == Some("package_fixture_only_external_btc_and_ckb_finality_required"),
            "fiber_execution_bound_when_required": !expected.fiber_required
                || json_pointer_str(&case, "/fiber_report_hash").is_some_and(is_hex32),
            "fiber_report_hash_matches_current_report": !expected.fiber_required
                || json_pointer_str(&case, "/fiber_report_hash") == expected_fiber_report_hash.as_deref(),
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert(format!("{}:{}", expected.profile, expected.action), checks);
    }

    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-profile-operator-fixtures-v0.1"),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(EXPECTED_PROFILE_OPERATOR_FIXTURES.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total")
            && json_pointer_i64(report, "/summary/profile_count") == Some(expected_profiles.len() as i64),
        "exact_profiles": actual_profiles == expected_profiles,
        "exact_profile_actions": actual_actions == expected_actions,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_profiles": expected_profiles.into_iter().collect::<Vec<_>>(),
        "expected_actions": expected_actions.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
    }))
}

fn validate_service_builder_fixture_detail(report: &Value, operator_fixtures: &Value) -> Value {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_profile_action: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let (Some(profile), Some(action)) = (json_pointer_str(case, "/profile"), json_pointer_str(case, "/action")) {
            by_profile_action.entry((profile.to_string(), action.to_string())).or_default().push(case.clone());
        }
    }
    let operator_cases = operator_fixtures.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut operator_by_profile_action: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for case in &operator_cases {
        if let (Some(profile), Some(action)) = (json_pointer_str(case, "/profile"), json_pointer_str(case, "/action")) {
            operator_by_profile_action.entry((profile.to_string(), action.to_string())).or_default().push(case.clone());
        }
    }

    let expected_profiles =
        EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().map(|fixture| fixture.profile.to_string()).collect::<BTreeSet<_>>();
    let expected_actions = EXPECTED_PROFILE_OPERATOR_FIXTURES
        .iter()
        .map(|fixture| format!("{}:{}", fixture.profile, fixture.action))
        .collect::<BTreeSet<_>>();
    let actual_profiles =
        cases.iter().filter_map(|case| json_pointer_str(case, "/profile").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let actual_actions = cases
        .iter()
        .filter_map(|case| Some(format!("{}:{}", json_pointer_str(case, "/profile")?, json_pointer_str(case, "/action")?)))
        .collect::<BTreeSet<_>>();
    let expected_operator_report_hash = novaseal_service_builder_report_hash("operator_report", operator_fixtures);

    let mut case_checks = Map::new();
    for expected in EXPECTED_PROFILE_OPERATOR_FIXTURES {
        let matches = by_profile_action.get(&(expected.profile.to_string(), expected.action.to_string())).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let operator_matches =
            operator_by_profile_action.get(&(expected.profile.to_string(), expected.action.to_string())).cloned().unwrap_or_default();
        let operator_case = operator_matches.first().cloned().unwrap_or(Value::Null);
        let expected_operator_case_hash = novaseal_service_builder_report_hash("operator_case", &operator_case);
        let public_btc_anchor_required = expected_public_btc_anchor_pointer(expected.profile).is_some();
        let operator_public_btc_anchor = operator_case.pointer("/public_btc_anchor");
        let request_public_btc_anchor = case.pointer("/request/required_live_inputs/public_btc_anchor");
        let tx_skeleton_public_btc_anchor = case.pointer("/tx_skeleton/public_btc_anchor");
        let checks = json!({
            "exactly_one_fixture": matches.len() == 1,
            "exactly_one_operator_fixture": operator_matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "builder_name": json_pointer_str(&case, "/builder_name") == Some("novaseal-profile-service-builder-v0"),
            "fixture_matches": json_pointer_str(&case, "/fixture") == Some(expected.fixture),
            "signers_match": json_array_strings(&case, "/signers") == expected.signers,
            "operator_fixture_hash": json_pointer_str(&case, "/operator_fixture_hash").is_some_and(is_hex32),
            "operator_fixture_hash_matches_current_operator_case": json_pointer_str(&case, "/operator_fixture_hash")
                == Some(expected_operator_case_hash.as_str()),
            "request_schema": json_pointer_str(&case, "/request/schema") == Some("novaseal-service-builder-request-v0.1"),
            "request_profile_matches": json_pointer_str(&case, "/request/profile") == Some(expected.profile),
            "request_action_matches": json_pointer_str(&case, "/request/action") == Some(expected.action),
            "request_signers_match": json_array_strings(&case, "/request/signers") == expected.signers,
            "request_idempotency_key": json_pointer_str(&case, "/request/idempotency_key").is_some_and(is_hex32),
            "request_operator_hash_matches": json_pointer_str(&case, "/request/operator_fixture_hash") == json_pointer_str(&case, "/operator_fixture_hash"),
            "request_profile_hashes_present": json_pointer_str(&case, "/request/required_profile_inputs/source_tree_hash").is_some_and(is_hex32)
                && json_pointer_str(&case, "/request/required_profile_inputs/schema_set_hash").is_some_and(is_hex32)
                && json_pointer_str(&case, "/request/required_profile_inputs/proof_matrix_hash").is_some_and(is_hex32)
                && json_pointer_str(&case, "/request/required_profile_inputs/fixture_hash").is_some_and(is_hex32),
            "live_inputs_present_when_required": !expected.live_required
                || (json_pointer_str(&case, "/request/required_live_inputs/live_report_hash").is_some_and(is_hex32)
                    && json_pointer_str(&case, "/request/required_live_inputs/live_devnet_tx_hash").is_some_and(is_real_tx_hash)),
            "live_inputs_match_operator_fixture": !expected.live_required
                || (json_pointer_str(&case, "/request/required_live_inputs/live_report_hash")
                    == json_pointer_str(&operator_case, "/live_report_hash")
                    && json_pointer_str(&case, "/request/required_live_inputs/live_devnet_tx_hash")
                        == json_pointer_str(&operator_case, "/live_devnet_tx_hash")),
            "fiber_input_present_when_required": !expected.fiber_required
                || json_pointer_str(&case, "/request/required_live_inputs/fiber_report_hash").is_some_and(is_hex32),
            "fiber_input_matches_operator_fixture": !expected.fiber_required
                || json_pointer_str(&case, "/request/required_live_inputs/fiber_report_hash")
                    == json_pointer_str(&operator_case, "/fiber_report_hash"),
            "public_btc_anchor_input_empty_when_not_required": public_btc_anchor_required
                || request_public_btc_anchor.is_none_or(Value::is_null),
            "public_btc_anchor_input_present_when_required": !public_btc_anchor_required
                || request_public_btc_anchor.is_some_and(|anchor| anchor.is_object()),
            "public_btc_anchor_input_shape_matches_profile": !public_btc_anchor_required
                || public_btc_anchor_shape_matches_profile(expected.profile, request_public_btc_anchor),
            "public_btc_anchor_input_matches_operator_fixture": !public_btc_anchor_required
                || request_public_btc_anchor == operator_public_btc_anchor,
            "external_inputs_named": !json_array_strings(&case, "/request/production_external_inputs").is_empty(),
            "response_schema": json_pointer_str(&case, "/response/schema") == Some("novaseal-service-builder-response-v0.1"),
            "response_profile_matches": json_pointer_str(&case, "/response/profile") == Some(expected.profile),
            "response_action_matches": json_pointer_str(&case, "/response/action") == Some(expected.action),
            "response_service_queue_key": json_pointer_str(&case, "/response/service_queue_key").is_some_and(is_hex32),
            "response_tx_skeleton_hash": json_pointer_str(&case, "/response/tx_skeleton_hash").is_some_and(is_hex32),
            "response_witness_shape_hash": json_pointer_str(&case, "/response/witness_shape_hash").is_some_and(is_hex32),
            "response_signed_intent_hash": json_pointer_str(&case, "/response/signed_intent_hash").is_some_and(is_hex32),
            "response_bip340_message_hash_matches": json_pointer_str(&case, "/response/bip340_message_hash")
                == json_pointer_str(&case, "/response/signed_intent_hash"),
            "response_receipt_binding_hash": json_pointer_str(&case, "/response/receipt_binding_hash").is_some_and(is_hex32),
            "response_builder_trace_hash": json_pointer_str(&case, "/response/builder_trace_hash").is_some_and(is_hex32),
            "tx_skeleton_schema": json_pointer_str(&case, "/tx_skeleton/schema") == Some("novaseal-service-builder-tx-skeleton-v0.1"),
            "tx_skeleton_operator_hash_matches": json_pointer_str(&case, "/tx_skeleton/operator_fixture_hash")
                == json_pointer_str(&case, "/operator_fixture_hash"),
            "tx_skeleton_public_btc_anchor_empty_when_not_required": public_btc_anchor_required
                || tx_skeleton_public_btc_anchor.is_none_or(Value::is_null),
            "tx_skeleton_public_btc_anchor_present_when_required": !public_btc_anchor_required
                || tx_skeleton_public_btc_anchor.is_some_and(|anchor| anchor.is_object()),
            "tx_skeleton_public_btc_anchor_shape_matches_profile": !public_btc_anchor_required
                || public_btc_anchor_shape_matches_profile(expected.profile, tx_skeleton_public_btc_anchor),
            "tx_skeleton_public_btc_anchor_matches_operator_fixture": !public_btc_anchor_required
                || tx_skeleton_public_btc_anchor == operator_public_btc_anchor,
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert(format!("{}:{}", expected.profile, expected.action), checks);
    }

    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-service-builder-fixtures-v0.1"),
        "builder_name": json_pointer_str(report, "/builder_name") == Some("novaseal-profile-service-builder-v0"),
        "source_operator_fixture_report_hash": json_pointer_str(report, "/source_operator_fixture_report_hash").is_some_and(is_hex32),
        "source_operator_fixture_report_hash_matches_current_report": json_pointer_str(report, "/source_operator_fixture_report_hash")
            == Some(expected_operator_report_hash.as_str()),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(EXPECTED_PROFILE_OPERATOR_FIXTURES.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total")
            && json_pointer_i64(report, "/summary/profile_count") == Some(expected_profiles.len() as i64),
        "exact_profiles": actual_profiles == expected_profiles,
        "exact_profile_actions": actual_actions == expected_actions,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_profiles": expected_profiles.into_iter().collect::<Vec<_>>(),
        "expected_actions": expected_actions.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
    })
}

#[cfg(test)]
fn validate_btc_spv_evidence_adapter_detail(report: &Value) -> Value {
    validate_btc_spv_evidence_adapter_detail_with_sources(report, None, None)
}

fn validate_btc_spv_evidence_adapter_detail_with_sources(
    report: &Value,
    service_builder: Option<&Value>,
    public_btc_spv_template: Option<&Value>,
) -> Value {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_profile: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let Some(profile) = json_pointer_str(case, "/profile") {
            by_profile.entry(profile.to_string()).or_default().push(case.clone());
        }
    }
    let expected_profiles = EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().map(|profile| (*profile).to_string()).collect::<BTreeSet<_>>();
    let actual_profiles =
        cases.iter().filter_map(|case| json_pointer_str(case, "/profile").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let service_builder_cases = service_builder.and_then(|builder| builder.get("cases")).and_then(Value::as_array);
    let service_builder_source_required = service_builder.is_some();

    let mut case_checks = Map::new();
    for expected_profile in EXPECTED_BTC_SPV_EVIDENCE_PROFILES {
        let matches = by_profile.get(*expected_profile).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let required_fields = json_array_strings(&case, "/request/required_public_fields");
        let external_inputs = json_array_strings(&case, "/request/required_external_inputs");
        let required_public_fields_complete =
            EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS.iter().all(|field| required_fields.iter().any(|actual| actual == field));
        let service_builder_case = service_builder_cases.and_then(|builder_cases| {
            builder_cases.iter().find(|builder_case| json_pointer_str(builder_case, "/profile") == Some(*expected_profile))
        });
        let expected_service_builder_case_hash =
            service_builder_case.map(|builder_case| novaseal_btc_spv_adapter_report_hash("service_builder_case", builder_case));
        let checks = json!({
            "exactly_one_case": matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "request_profile_matches": json_pointer_str(&case, "/request/profile") == Some(*expected_profile),
            "scenario_matches_expected": json_pointer_str(&case, "/request/scenario") == expected_btc_spv_scenario(expected_profile),
            "minimum_confirmations_at_least_six": json_pointer_i64(&case, "/request/minimum_confirmations").unwrap_or_default() >= 6,
            "public_btc_spv_external_input_named": external_inputs.iter().any(|value| value == "public_btc_spv_evidence"),
            "ckb_live_tx_hash": json_pointer_str(&case, "/request/ckb_live_tx_hash").is_some_and(is_hex32),
            "live_report_hash": json_pointer_str(&case, "/request/live_report_hash").is_some_and(is_hex32),
            "service_builder_case_present_in_current_report": !service_builder_source_required || service_builder_case.is_some(),
            "service_builder_case_hash": json_pointer_str(&case, "/request/service_builder_case_hash").is_some_and(is_hex32),
            "service_builder_case_hash_matches_current_report": if service_builder_source_required {
                expected_service_builder_case_hash
                    .as_deref()
                    .is_some_and(|expected| json_pointer_str(&case, "/request/service_builder_case_hash") == Some(expected))
            } else {
                true
            },
            "service_builder_tx_skeleton_hash": json_pointer_str(&case, "/request/service_builder_tx_skeleton_hash").is_some_and(is_hex32),
            "service_builder_tx_skeleton_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| json_pointer_str(builder_case, "/response/tx_skeleton_hash"))
                    .is_some_and(|expected| json_pointer_str(&case, "/request/service_builder_tx_skeleton_hash") == Some(expected))
            } else {
                true
            },
            "service_builder_receipt_binding_hash": json_pointer_str(&case, "/request/service_builder_receipt_binding_hash").is_some_and(is_hex32),
            "service_builder_receipt_binding_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| json_pointer_str(builder_case, "/response/receipt_binding_hash"))
                    .is_some_and(|expected| json_pointer_str(&case, "/request/service_builder_receipt_binding_hash") == Some(expected))
            } else {
                true
            },
            "ckb_live_tx_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| json_pointer_str(builder_case, "/request/required_live_inputs/live_devnet_tx_hash"))
                    .is_some_and(|expected| json_pointer_str(&case, "/request/ckb_live_tx_hash") == Some(expected))
            } else {
                true
            },
            "live_report_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| json_pointer_str(builder_case, "/request/required_live_inputs/live_report_hash"))
                    .is_some_and(|expected| json_pointer_str(&case, "/request/live_report_hash") == Some(expected))
            } else {
                true
            },
            "expected_anchor_source_production_eligible": json_pointer_str(&case, "/request/expected_anchor_source")
                .is_some_and(|source| btc_anchor_source_production_eligible(expected_profile, source)),
            "local_anchor_source_present": json_pointer_str(&case, "/request/local_anchor_source").is_some_and(|source| !source.is_empty()),
            "ckb_btc_commitment_hash": json_pointer_str(&case, "/request/ckb_btc_commitment_hash").is_some_and(is_hex32),
            "ckb_btc_commitment_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| {
                        json_pointer_str(builder_case, "/request/required_live_inputs/public_btc_anchor/ckb_btc_commitment_hash")
                    })
                    .is_some_and(|expected| json_pointer_str(&case, "/request/ckb_btc_commitment_hash") == Some(expected))
            } else {
                true
            },
            "expected_btc_txid_present": json_pointer_str(&case, "/request/expected_btc_txid").is_some_and(is_hex32),
            "expected_btc_wtxid_present": json_pointer_str(&case, "/request/expected_btc_wtxid").is_some_and(is_hex32),
            "expected_output_fields_present": *expected_profile != EXPECTED_BTC_TX_COMMITMENT_PROFILE
                || (json_pointer_i64(&case, "/request/expected_btc_output_index").is_some_and(|value| value >= 0)
                    && json_pointer_i64(&case, "/request/expected_btc_amount_sats").is_some_and(|value| value > 0)),
            "expected_utxo_fields_present": *expected_profile != EXPECTED_BTC_UTXO_SEAL_PROFILE
                || (json_pointer_str(&case, "/request/expected_sealed_btc_txid").is_some_and(is_hex32)
                    && json_pointer_i64(&case, "/request/expected_sealed_btc_vout_index").is_some_and(|value| value >= 0)
                    && json_pointer_i64(&case, "/request/expected_sealed_btc_amount_sats").is_some_and(|value| value > 0)
                    && json_pointer_str(&case, "/request/expected_script_pubkey_hash").is_some_and(is_hex32)
                    && json_pointer_i64(&case, "/request/expected_spend_input_index").is_some_and(|value| value >= 0)
                    && json_pointer_str(&case, "/request/expected_sealed_utxo_commitment_hash").is_some_and(is_hex32)),
            "expected_dual_sealed_utxo_fields_present": *expected_profile != EXPECTED_DUAL_SEAL_PROFILE
                || (json_pointer_str(&case, "/request/expected_sealed_btc_txid").is_some_and(is_hex32)
                    && json_pointer_i64(&case, "/request/expected_sealed_btc_vout_index").is_some_and(|value| value >= 0)
                    && json_pointer_i64(&case, "/request/expected_sealed_btc_amount_sats").is_some_and(|value| value > 0)
                    && json_pointer_str(&case, "/request/expected_script_pubkey_hash").is_some_and(is_hex32)
                    && json_pointer_i64(&case, "/request/expected_spend_input_index").is_some_and(|value| value >= 0)
                    && json_pointer_str(&case, "/request/expected_sealed_utxo_commitment_hash").is_some_and(is_hex32)),
            "template_case_hash": json_pointer_str(&case, "/request/template_case_hash").is_some_and(is_hex32),
            "required_public_fields_complete": required_public_fields_complete,
            "required_public_fields_exact": exact_string_set(&required_fields, EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS),
            "field_constraints_exact": exact_string_map(
                case.pointer("/request/field_constraints").unwrap_or(&Value::Null),
                EXPECTED_BTC_SPV_FIELD_CONSTRAINTS,
            ),
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert((*expected_profile).to_string(), checks);
    }

    let expected_service_builder_report_hash =
        service_builder.map(|builder| novaseal_btc_spv_adapter_report_hash("service_builder_report", builder));
    let expected_public_btc_spv_template_hash =
        public_btc_spv_template.map(|template| novaseal_btc_spv_adapter_report_hash("public_btc_spv_template", template));
    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-btc-spv-evidence-adapter-v0.1"),
        "adapter_status_request_ready": json_pointer_str(report, "/adapter_status") == Some("request_ready_external_evidence_required"),
        "service_builder_report_hash": json_pointer_str(report, "/source_service_builder_report_hash").is_some_and(is_hex32),
        "service_builder_report_hash_matches_current_report": expected_service_builder_report_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_service_builder_report_hash") == Some(expected)),
        "public_btc_spv_template_hash": json_pointer_str(report, "/source_public_btc_spv_template_hash").is_some_and(is_hex32),
        "public_btc_spv_template_hash_matches_current_template": expected_public_btc_spv_template_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_public_btc_spv_template_hash") == Some(expected)),
        "production_output_named": json_pointer_str(report, "/production_output") == Some(PUBLIC_BTC_SPV_EVIDENCE),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(EXPECTED_BTC_SPV_EVIDENCE_PROFILES.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total"),
        "exact_profiles": actual_profiles == expected_profiles,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_profiles": expected_profiles.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
        "production_boundary": json_pointer_str(report, "/production_boundary"),
    })
}

#[cfg(test)]
fn validate_external_attestation_adapter_detail(report: &Value) -> Value {
    validate_external_attestation_adapter_detail_with_sources(report, None, None, None)
}

fn validate_external_attestation_adapter_detail_with_sources(
    report: &Value,
    tcb_review: Option<&Value>,
    public_cell_dep_template: Option<&Value>,
    external_tcb_template: Option<&Value>,
) -> Value {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_name: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let Some(name) = json_pointer_str(case, "/name") {
            by_name.entry(name.to_string()).or_default().push(case.clone());
        }
    }
    let expected = [
        (
            "public_shared_cell_dep_attestation",
            PUBLIC_CELLDEP_ATTESTATION,
            "novaseal-public-shared-cell-dep-attestation-v0.1",
            "attested",
        ),
        (
            "external_bip340_tcb_review_attestation",
            EXTERNAL_TCB_ATTESTATION,
            "novaseal-bip340-external-tcb-review-attestation-v0.1",
            "accepted",
        ),
    ];
    let expected_names = expected.iter().map(|(name, _, _, _)| (*name).to_string()).collect::<BTreeSet<_>>();
    let actual_names =
        cases.iter().filter_map(|case| json_pointer_str(case, "/name").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let expected_tcb_artifact_hash =
        tcb_review.and_then(|tcb| normalize_hex(json_pointer_str(tcb, "/runtime_artifact/artifact_hash")));
    let expected_tcb_artifact_hash_algorithm =
        tcb_review.and_then(|tcb| json_pointer_str(tcb, "/runtime_artifact/artifact_hash_algorithm"));
    let expected_tcb_source_tree_hash =
        tcb_review.and_then(|tcb| normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256")));
    let expected_tcb_repo_commit = tcb_review.and_then(|tcb| json_pointer_str(tcb, "/repo_commit"));

    let mut case_checks = Map::new();
    for (name, production_output, template_schema, required_status) in expected {
        let matches = by_name.get(name).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let required_fields = json_array_strings(&case, "/request/required_public_fields");
        let expected_required_fields = if name == "public_shared_cell_dep_attestation" {
            EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS
        } else {
            EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS
        };
        let expected_field_constraints = if name == "public_shared_cell_dep_attestation" {
            EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS
        } else {
            EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS
        };
        let expected_template_hash = if name == "public_shared_cell_dep_attestation" {
            public_cell_dep_template.map(|template| novaseal_external_attestation_report_hash("public_celldep_template", template))
        } else {
            external_tcb_template.map(|template| novaseal_external_attestation_report_hash("external_tcb_template", template))
        };
        let checks = json!({
            "exactly_one_case": matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "production_output_matches": json_pointer_str(&case, "/request/production_output") == Some(production_output),
            "template_schema_matches": json_pointer_str(&case, "/request/template_schema") == Some(template_schema),
            "template_hash": json_pointer_str(&case, "/request/template_hash").is_some_and(is_hex32),
            "template_hash_matches_current_template": expected_template_hash
                .as_deref()
                .is_none_or(|expected| json_pointer_str(&case, "/request/template_hash") == Some(expected)),
            "verifier_id_current": json_pointer_str(&case, "/request/verifier_id") == Some("btc.bip340.v0"),
            "ipc_abi_current": json_pointer_str(&case, "/request/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
            "required_status_matches": json_pointer_str(&case, "/request/required_status") == Some(required_status),
            "required_fields_complete": expected_required_fields.iter().all(|field| required_fields.iter().any(|actual| actual == field)),
            "required_fields_exact": exact_string_set(&required_fields, expected_required_fields),
            "field_constraints_exact": exact_string_map(
                case.pointer("/request/field_constraints").unwrap_or(&Value::Null),
                expected_field_constraints,
            ),
            "artifact_hash_present": json_pointer_str(&case, "/request/expected_artifact_hash").is_some_and(is_hex32),
            "expected_release_package_current": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_release_package") == Some("novaseal"),
            "expected_release_version_current": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_release_version") == Some(EXPECTED_NOVASEAL_RELEASE_VERSION),
            "expected_dep_type_current": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_dep_type") == Some(EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE),
            "expected_hash_type_current": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_hash_type") == Some(EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE),
            "expected_release_manifest_commit_present": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_release_manifest_commit").is_some_and(is_git_commit_hash),
            "expected_release_manifest_commit_matches_current_tcb": name != "public_shared_cell_dep_attestation"
                || expected_tcb_repo_commit
                    .is_none_or(|expected| json_pointer_str(&case, "/request/expected_release_manifest_commit") == Some(expected)),
            "expected_review_scope_exact": name != "external_bip340_tcb_review_attestation"
                || exact_string_set(&json_array_strings(&case, "/request/expected_review_scope"), EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE),
            "expected_artifact_hash_matches_current_tcb": expected_tcb_artifact_hash
                .as_deref()
                .is_none_or(|expected| {
                    normalize_hex(json_pointer_str(&case, "/request/expected_artifact_hash")).as_deref() == Some(expected)
                }),
            "template_artifact_hash_matches_current_tcb": expected_tcb_artifact_hash
                .as_deref()
                .is_none_or(|expected| {
                    normalize_hex(json_pointer_str(&case, "/request/template_artifact_hash")).as_deref() == Some(expected)
                }),
            "expected_source_tree_sha256_matches_current_tcb": name != "external_bip340_tcb_review_attestation"
                || expected_tcb_source_tree_hash
                    .as_deref()
                    .is_none_or(|expected| {
                        normalize_hex(json_pointer_str(&case, "/request/expected_source_tree_sha256")).as_deref() == Some(expected)
                    }),
            "template_source_tree_sha256_matches_current_tcb": name != "external_bip340_tcb_review_attestation"
                || expected_tcb_source_tree_hash
                    .as_deref()
                    .is_none_or(|expected| {
                        normalize_hex(json_pointer_str(&case, "/request/template_source_tree_sha256")).as_deref() == Some(expected)
                    }),
            "expected_artifact_hash_algorithm_matches_current_tcb": name == "public_shared_cell_dep_attestation"
                || expected_tcb_artifact_hash_algorithm
                    .is_none_or(|expected| json_pointer_str(&case, "/request/expected_artifact_hash_algorithm") == Some(expected)),
            "artifact_hash_algorithm_matches_tcb": name == "public_shared_cell_dep_attestation"
                || (
                    json_pointer_str(&case, "/request/expected_artifact_hash_algorithm") == Some("sha256")
                        && json_pointer_str(&case, "/request/template_artifact_hash_algorithm")
                            == json_pointer_str(&case, "/request/expected_artifact_hash_algorithm")
                ),
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert(name.to_string(), checks);
    }

    let expected_tcb_review_hash = tcb_review.map(|tcb| novaseal_external_attestation_report_hash("tcb_review", tcb));
    let expected_public_cell_dep_template_hash =
        public_cell_dep_template.map(|template| novaseal_external_attestation_report_hash("public_celldep_template", template));
    let expected_external_tcb_template_hash =
        external_tcb_template.map(|template| novaseal_external_attestation_report_hash("external_tcb_template", template));
    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-external-attestation-adapter-v0.1"),
        "adapter_status_request_ready": json_pointer_str(report, "/adapter_status") == Some("request_ready_external_attestations_required"),
        "source_tcb_review_hash": json_pointer_str(report, "/source_tcb_review_hash").is_some_and(is_hex32),
        "source_tcb_review_hash_matches_current_report": expected_tcb_review_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_tcb_review_hash") == Some(expected)),
        "source_public_cell_dep_template_hash": json_pointer_str(report, "/source_public_cell_dep_template_hash").is_some_and(is_hex32),
        "source_public_cell_dep_template_hash_matches_current_template": expected_public_cell_dep_template_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_public_cell_dep_template_hash") == Some(expected)),
        "source_external_tcb_template_hash": json_pointer_str(report, "/source_external_tcb_template_hash").is_some_and(is_hex32),
        "source_external_tcb_template_hash_matches_current_template": expected_external_tcb_template_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_external_tcb_template_hash") == Some(expected)),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(expected_names.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total"),
        "exact_attestations": actual_names == expected_names,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_attestations": expected_names.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
        "production_boundary": json_pointer_str(report, "/production_boundary"),
    })
}

fn validate_external_evidence_handoff_detail(
    repo_root: &Path,
    report: &Value,
    btc_spv_adapter: &Value,
    external_attestation_adapter: &Value,
) -> Value {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_group: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let Some(group) = json_pointer_str(case, "/group") {
            by_group.entry(group.to_string()).or_default().push(case.clone());
        }
    }

    let expected = [
        ("public_btc_spv_evidence", PUBLIC_BTC_SPV_EVIDENCE),
        ("public_shared_cell_dep_attestation", PUBLIC_CELLDEP_ATTESTATION),
        ("external_bip340_tcb_review_attestation", EXTERNAL_TCB_ATTESTATION),
        ("rwa_legal_registry_review_evidence", RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE),
    ];
    let expected_groups = expected.iter().map(|(group, _)| (*group).to_string()).collect::<BTreeSet<_>>();
    let expected_outputs = expected.iter().map(|(_, output)| (*output).to_string()).collect::<BTreeSet<_>>();
    let actual_groups =
        cases.iter().filter_map(|case| json_pointer_str(case, "/group").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let actual_outputs = json_array_strings(report, "/production_outputs").into_iter().collect::<BTreeSet<_>>();
    let expected_btc_spv_adapter_hash = novaseal_handoff_report_hash("btc_spv_adapter", btc_spv_adapter);
    let expected_external_attestation_adapter_hash =
        novaseal_handoff_report_hash("external_attestation_adapter", external_attestation_adapter);
    let expected_handoff_bundle_hash = external_evidence_handoff_reference_hash(report);
    let expected_btc_spv_scenarios = btc_spv_adapter_expected_scenarios(btc_spv_adapter);
    let expected_btc_spv_case_bindings = btc_spv_adapter_expected_case_bindings(btc_spv_adapter);
    let expected_public_manifest_commit = adapter_case_request_str(
        external_attestation_adapter,
        "public_shared_cell_dep_attestation",
        "/request/expected_release_manifest_commit",
    );
    let expected_public_release_package = adapter_case_request_str(
        external_attestation_adapter,
        "public_shared_cell_dep_attestation",
        "/request/expected_release_package",
    );
    let expected_public_release_version = adapter_case_request_str(
        external_attestation_adapter,
        "public_shared_cell_dep_attestation",
        "/request/expected_release_version",
    );
    let expected_public_artifact_hash = adapter_case_request_str(
        external_attestation_adapter,
        "public_shared_cell_dep_attestation",
        "/request/expected_artifact_hash",
    );
    let expected_public_dep_type =
        adapter_case_request_str(external_attestation_adapter, "public_shared_cell_dep_attestation", "/request/expected_dep_type");
    let expected_public_hash_type =
        adapter_case_request_str(external_attestation_adapter, "public_shared_cell_dep_attestation", "/request/expected_hash_type");
    let expected_public_ipc_abi =
        adapter_case_request_str(external_attestation_adapter, "public_shared_cell_dep_attestation", "/request/ipc_abi");
    let expected_public_verifier_id =
        adapter_case_request_str(external_attestation_adapter, "public_shared_cell_dep_attestation", "/request/verifier_id");
    let expected_external_tcb_artifact_hash = adapter_case_request_str(
        external_attestation_adapter,
        "external_bip340_tcb_review_attestation",
        "/request/expected_artifact_hash",
    );
    let expected_external_tcb_artifact_hash_algorithm = adapter_case_request_str(
        external_attestation_adapter,
        "external_bip340_tcb_review_attestation",
        "/request/expected_artifact_hash_algorithm",
    );
    let expected_external_tcb_ipc_abi =
        adapter_case_request_str(external_attestation_adapter, "external_bip340_tcb_review_attestation", "/request/ipc_abi");
    let expected_external_tcb_verifier_id =
        adapter_case_request_str(external_attestation_adapter, "external_bip340_tcb_review_attestation", "/request/verifier_id");
    let expected_external_tcb_source_tree_hash = adapter_case_request_str(
        external_attestation_adapter,
        "external_bip340_tcb_review_attestation",
        "/request/expected_source_tree_sha256",
    );
    let expected_external_tcb_review_scope = adapter_case_request_strings(
        external_attestation_adapter,
        "external_bip340_tcb_review_attestation",
        "/request/expected_review_scope",
    );
    let expected_rwa_profile_source_hash = source_tree_hash(repo_root, RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)
        .ok()
        .and_then(|value| normalize_hex(json_pointer_str(&value, "/sha256")));

    let mut case_checks = Map::new();
    for (group, production_output) in expected {
        let matches = by_group.get(group).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let required_external_fields = json_array_strings(&case, "/required_external_fields");
        let required_profiles = json_array_strings(&case, "/required_profiles");
        let expected_btc_profiles =
            EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().map(|profile| (*profile).to_string()).collect::<Vec<_>>();
        let expected_source_hash = if group == "public_btc_spv_evidence" {
            expected_btc_spv_adapter_hash.as_str()
        } else {
            expected_external_attestation_adapter_hash.as_str()
        };
        let expected_source_adapter =
            if group == "public_btc_spv_evidence" { BTC_SPV_EVIDENCE_ADAPTER } else { EXTERNAL_ATTESTATION_ADAPTER };
        let expected_required_external_fields = match group {
            "public_btc_spv_evidence" => EXPECTED_PUBLIC_BTC_SPV_HANDOFF_FIELDS,
            "public_shared_cell_dep_attestation" => EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS,
            "rwa_legal_registry_review_evidence" => EXPECTED_RWA_LEGAL_REVIEW_REQUIRED_FIELDS,
            _ => EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS,
        };
        let expected_field_constraints = match group {
            "public_btc_spv_evidence" => EXPECTED_BTC_SPV_FIELD_CONSTRAINTS,
            "public_shared_cell_dep_attestation" => EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS,
            "rwa_legal_registry_review_evidence" => EXPECTED_RWA_LEGAL_REVIEW_FIELD_CONSTRAINTS,
            _ => EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS,
        };
        let expected_scenarios_match_source_adapter = group != "public_btc_spv_evidence"
            || (json_object_string_map(case.get("expected_scenarios").unwrap_or(&Value::Null)) == expected_btc_spv_scenarios
                && EXPECTED_BTC_SPV_PROFILE_SCENARIOS
                    .iter()
                    .all(|(profile, scenario)| expected_btc_spv_scenarios.get(*profile).is_some_and(|actual| actual == *scenario)));
        let expected_case_bindings_match_source_adapter = group != "public_btc_spv_evidence"
            || (case.get("expected_case_bindings").is_some_and(handoff_expected_bindings_exact)
                && case.get("expected_case_bindings") == Some(&expected_btc_spv_case_bindings));
        let expected_values_match_source_adapter = match group {
            "public_shared_cell_dep_attestation" => {
                exact_object_keys(case.get("expected_values").unwrap_or(&Value::Null), EXPECTED_PUBLIC_CELLDEP_EXPECTED_VALUE_FIELDS)
                    && expected_public_manifest_commit.is_some_and(is_git_commit_hash)
                    && expected_public_release_package == Some("novaseal")
                    && expected_public_release_version == Some(EXPECTED_NOVASEAL_RELEASE_VERSION)
                    && expected_public_artifact_hash.is_some_and(is_hex32)
                    && expected_public_dep_type == Some(EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE)
                    && expected_public_hash_type == Some(EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE)
                    && expected_public_ipc_abi == Some("cellscript-btc-bip340-ipc-v0")
                    && expected_public_verifier_id == Some("btc.bip340.v0")
                    && json_pointer_str(&case, "/expected_values/artifact_hash") == expected_public_artifact_hash
                    && json_pointer_str(&case, "/expected_values/release.package") == expected_public_release_package
                    && json_pointer_str(&case, "/expected_values/release.version") == expected_public_release_version
                    && json_pointer_str(&case, "/expected_values/release.manifest_commit") == expected_public_manifest_commit
                    && json_pointer_str(&case, "/expected_values/runtime_verifier.dep_type") == expected_public_dep_type
                    && json_pointer_str(&case, "/expected_values/runtime_verifier.hash_type") == expected_public_hash_type
                    && json_pointer_str(&case, "/expected_values/runtime_verifier.ipc_abi") == expected_public_ipc_abi
                    && json_pointer_str(&case, "/expected_values/runtime_verifier.verifier_id") == expected_public_verifier_id
            }
            "external_bip340_tcb_review_attestation" => {
                exact_object_keys(case.get("expected_values").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_TCB_EXPECTED_VALUE_FIELDS)
                    && expected_external_tcb_artifact_hash.is_some_and(is_hex32)
                    && expected_external_tcb_source_tree_hash.is_some_and(is_hex32)
                    && expected_external_tcb_ipc_abi == Some("cellscript-btc-bip340-ipc-v0")
                    && expected_external_tcb_verifier_id == Some("btc.bip340.v0")
                    && exact_string_set(&expected_external_tcb_review_scope, EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE)
                    && json_pointer_str(&case, "/expected_values/artifact_hash") == expected_external_tcb_artifact_hash
                    && json_pointer_str(&case, "/expected_values/artifact_hash_algorithm")
                        == expected_external_tcb_artifact_hash_algorithm
                    && json_pointer_str(&case, "/expected_values/ipc_abi") == expected_external_tcb_ipc_abi
                    && json_pointer_str(&case, "/expected_values/verifier_id") == expected_external_tcb_verifier_id
                    && json_array_strings(&case, "/expected_values/review_scope") == expected_external_tcb_review_scope
                    && json_pointer_str(&case, "/expected_values/source_tree_sha256") == expected_external_tcb_source_tree_hash
            }
            "rwa_legal_registry_review_evidence" => {
                exact_object_keys(case.get("expected_values").unwrap_or(&Value::Null), EXPECTED_RWA_LEGAL_REVIEW_EXPECTED_VALUE_FIELDS)
                    && expected_rwa_profile_source_hash.as_deref().is_some_and(is_hex32)
                    && json_pointer_str(&case, "/expected_values/profile") == Some(EXPECTED_RWA_RECEIPT_PROFILE)
                    && exact_string_set(&json_array_strings(&case, "/expected_values/review_scope"), EXPECTED_RWA_LEGAL_REVIEW_SCOPE)
                    && normalize_hex(json_pointer_str(&case, "/expected_values/profile_source_tree_sha256"))
                        == expected_rwa_profile_source_hash
            }
            _ => true,
        };
        let checks = json!({
            "exactly_one_case": matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "production_output_matches": json_pointer_str(&case, "/production_output") == Some(production_output),
            "source_adapter_path_matches_current": json_pointer_str(&case, "/source_adapter") == Some(expected_source_adapter),
            "source_adapter_hash_matches_current": json_pointer_str(&case, "/source_adapter_hash") == Some(expected_source_hash),
            "required_external_fields_complete": expected_required_external_fields
                .iter()
                .all(|field| required_external_fields.iter().any(|actual| actual == field)),
            "required_external_fields_exact": exact_string_set(&required_external_fields, expected_required_external_fields),
            "field_constraints_exact": exact_string_map(
                case.get("field_constraints").unwrap_or(&Value::Null),
                expected_field_constraints,
            ),
            "expected_scenarios_match_source_adapter": expected_scenarios_match_source_adapter,
            "expected_case_bindings_match_source_adapter": expected_case_bindings_match_source_adapter,
            "expected_values_match_source_adapter": expected_values_match_source_adapter,
            "btc_profiles_complete": group != "public_btc_spv_evidence"
                || required_profiles == expected_btc_profiles,
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert(group.to_string(), checks);
    }

    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-external-evidence-handoff-bundle-v0.1"),
        "handoff_status_request_ready": json_pointer_str(report, "/handoff_status") == Some("request_bundle_ready_external_evidence_required"),
        "source_btc_spv_adapter_path_matches_current": json_pointer_str(report, "/source_btc_spv_adapter") == Some(BTC_SPV_EVIDENCE_ADAPTER),
        "source_external_attestation_adapter_path_matches_current": json_pointer_str(report, "/source_external_attestation_adapter")
            == Some(EXTERNAL_ATTESTATION_ADAPTER),
        "source_btc_spv_adapter_hash_matches_current": json_pointer_str(report, "/source_btc_spv_adapter_hash")
            == Some(expected_btc_spv_adapter_hash.as_str()),
        "source_external_attestation_adapter_hash_matches_current": json_pointer_str(report, "/source_external_attestation_adapter_hash")
            == Some(expected_external_attestation_adapter_hash.as_str()),
        "bundle_hash_matches_reference": normalize_hex(json_pointer_str(report, "/bundle_hash")).as_deref()
            == Some(expected_handoff_bundle_hash.as_str()),
        "bundle_hash_algorithm": json_pointer_str(report, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(expected_groups.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total"),
        "exact_groups": actual_groups == expected_groups,
        "exact_production_outputs": actual_outputs == expected_outputs,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_groups": expected_groups.into_iter().collect::<Vec<_>>(),
        "expected_production_outputs": expected_outputs.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
        "production_boundary": json_pointer_str(report, "/production_boundary"),
    })
}

fn adapter_case_request_str<'a>(adapter: &'a Value, case_name: &str, pointer: &str) -> Option<&'a str> {
    adapter
        .get("cases")?
        .as_array()?
        .iter()
        .find(|case| json_pointer_str(case, "/name") == Some(case_name))
        .and_then(|case| json_pointer_str(case, pointer))
}

fn adapter_case_request_strings(adapter: &Value, case_name: &str, pointer: &str) -> Vec<String> {
    adapter
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| cases.iter().find(|case| json_pointer_str(case, "/name") == Some(case_name)))
        .map_or_else(Vec::new, |case| json_array_strings(case, pointer))
}

fn handoff_case<'a>(handoff: &'a Value, group: &str) -> Option<&'a Value> {
    handoff
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| cases.iter().find(|case| json_pointer_str(case, "/group") == Some(group)))
}

fn handoff_case_expected_values<'a>(handoff: &'a Value, group: &str) -> Option<&'a Value> {
    handoff_case(handoff, group).and_then(|case| case.get("expected_values"))
}

fn btc_spv_adapter_expected_scenarios(adapter: &Value) -> BTreeMap<String, String> {
    adapter
        .get("cases")
        .and_then(Value::as_array)
        .map(|cases| {
            cases
                .iter()
                .filter_map(|case| {
                    let profile = json_pointer_str(case, "/profile")?;
                    let scenario = json_pointer_str(case, "/request/scenario")?;
                    Some((profile.to_string(), scenario.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn btc_spv_adapter_expected_case_bindings(adapter: &Value) -> Value {
    let binding_fields = [
        ("ckb_live_tx_hash", "/request/ckb_live_tx_hash"),
        ("live_report_hash", "/request/live_report_hash"),
        ("service_builder_case_hash", "/request/service_builder_case_hash"),
        ("service_builder_tx_skeleton_hash", "/request/service_builder_tx_skeleton_hash"),
        ("service_builder_receipt_binding_hash", "/request/service_builder_receipt_binding_hash"),
        ("ckb_btc_commitment_hash", "/request/ckb_btc_commitment_hash"),
    ];
    let mut profiles = Map::new();
    if let Some(cases) = adapter.get("cases").and_then(Value::as_array) {
        for case in cases {
            let Some(profile) = json_pointer_str(case, "/profile") else {
                continue;
            };
            let mut binding = Map::new();
            for (field, pointer) in binding_fields {
                if let Some(value) = json_pointer_str(case, pointer) {
                    binding.insert(field.to_string(), Value::String(value.to_string()));
                }
            }
            for (field, pointer) in [
                ("anchor_source", "/request/expected_anchor_source"),
                ("btc_txid", "/request/expected_btc_txid"),
                ("btc_wtxid", "/request/expected_btc_wtxid"),
                ("sealed_btc_txid", "/request/expected_sealed_btc_txid"),
                ("script_pubkey_hash", "/request/expected_script_pubkey_hash"),
                ("sealed_utxo_commitment_hash", "/request/expected_sealed_utxo_commitment_hash"),
            ] {
                if let Some(value) = json_pointer_str(case, pointer) {
                    binding.insert(field.to_string(), Value::String(value.to_string()));
                }
            }
            for (field, pointer) in [
                ("btc_output_index", "/request/expected_btc_output_index"),
                ("btc_amount_sats", "/request/expected_btc_amount_sats"),
                ("spend_input_index", "/request/expected_spend_input_index"),
                ("sealed_btc_vout_index", "/request/expected_sealed_btc_vout_index"),
                ("sealed_btc_amount_sats", "/request/expected_sealed_btc_amount_sats"),
            ] {
                if let Some(value) = json_pointer_u64(case, pointer) {
                    binding.insert(field.to_string(), Value::Number(value.into()));
                }
            }
            profiles.insert(profile.to_string(), Value::Object(binding));
        }
    }
    Value::Object(profiles)
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
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-fungible-xudt-devnet-stateful-live.json"),
            ),
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
        "operator_fixture_evidence": PROFILE_OPERATOR_FIXTURES,
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
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-rwa-receipt-devnet-stateful-live.json"),
            ),
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
        "operator_fixture_evidence": PROFILE_OPERATOR_FIXTURES,
        "remaining_acceptance_gap": "legal/registry review evidence is still required before rwa_receipt_lifecycle can make production RWA title or registry claims",
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
    let expected_actions = ["commit_btc_transaction_transition", "nova_btc_transaction_commitment_lifecycle"]
        .iter()
        .map(|action| (*action).to_string())
        .collect::<BTreeSet<_>>();
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
            Value::Bool(
                metadata_str("stateful_dispatcher")
                    == Some("src/nova_btc_transaction_commitment_type.cell:nova_btc_transaction_commitment_lifecycle"),
            ),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(metadata_str("btc_public_verification") == Some("missing-spv-or-indexer-evidence")),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some(
                        "src/nova_btc_transaction_commitment_type.cell:commit_btc_transaction_transition;src/nova_btc_transaction_commitment_type.cell:nova_btc_transaction_commitment_lifecycle",
                    ),
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
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-btc-transaction-commitment-devnet-stateful-live.json"),
            ),
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
        "remaining_acceptance_gap": "public BTC SPV evidence is still required before btc_transaction_commitment_transition can make production BTC-finality claims",
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
    let expected_actions =
        ["close_btc_utxo_seal", "nova_btc_utxo_seal_lifecycle"].iter().map(|action| (*action).to_string()).collect::<BTreeSet<_>>();
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
            Value::Bool(metadata_str("stateful_dispatcher") == Some("src/nova_btc_utxo_seal_type.cell:nova_btc_utxo_seal_lifecycle")),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(metadata_str("btc_public_verification") == Some("missing-spv-or-indexer-evidence")),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some("src/nova_btc_utxo_seal_type.cell:close_btc_utxo_seal;src/nova_btc_utxo_seal_type.cell:nova_btc_utxo_seal_lifecycle"),
            ),
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
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-btc-utxo-seal-devnet-stateful-live.json"),
            ),
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
        "remaining_acceptance_gap": "public BTC SPV spend-verification evidence is still required before btc_utxo_seal_closure can make production BTC-spend claims",
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
    let expected_actions =
        ["finalize_dual_seal", "nova_dual_seal_lifecycle"].iter().map(|action| (*action).to_string()).collect::<BTreeSet<_>>();
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
            Value::Bool(metadata_str("stateful_dispatcher") == Some("src/nova_dual_seal_type.cell:nova_dual_seal_lifecycle")),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(metadata_str("btc_public_verification") == Some("missing-spv-or-indexer-evidence")),
        ),
        (
            "manifest_ckb_finality_gap".to_string(),
            Value::Bool(metadata_str("ckb_finality_verification") == Some("target/novaseal-dual-seal-devnet-stateful-live.json")),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some("src/nova_dual_seal_type.cell:finalize_dual_seal;src/nova_dual_seal_type.cell:nova_dual_seal_lifecycle"),
            ),
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
            Value::Bool(coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str) == Some("live-devnet-covered")),
        ),
        (
            "btc_public_verification_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("btc_public_verification").and_then(Value::as_str) == Some("missing-spv-or-indexer-evidence"),
            ),
        ),
        (
            "ckb_finality_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("ckb_finality_verification").and_then(Value::as_str) == Some("live-devnet-covered")),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-dual-seal-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-plus-live-ckb-stateful-evidence-not-btc-finality",
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
        "remaining_acceptance_gap": "public BTC closure evidence is still required before dual_seal_finality can make production BTC-finality claims",
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
    let expected_actions = ["settle_fiber_candidate", "nova_fiber_candidate_lifecycle"]
        .iter()
        .map(|action| (*action).to_string())
        .collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_FIBER_CANDIDATE_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_FIBER_CANDIDATE_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_FIBER_CANDIDATE_DOCS)?;
    let audit_doc = std::fs::read_to_string(root.join("docs/AUDIT_STATUS.md")).unwrap_or_default();
    let devnet_doc = std::fs::read_to_string(root.join("docs/DEVNET_STATEFUL_ACCEPTANCE.md")).unwrap_or_default();
    let security_doc = std::fs::read_to_string(root.join("docs/SECURITY.md")).unwrap_or_default();
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
            Value::Bool(metadata_str("stateful_dispatcher") == Some("src/nova_fiber_candidate_type.cell:nova_fiber_candidate_lifecycle")),
        ),
        (
            "manifest_fiber_execution_gap".to_string(),
            Value::Bool(metadata_str("fiber_execution") == Some(FIBER_NODE_EXPERIMENTS)),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some("src/nova_fiber_candidate_type.cell:settle_fiber_candidate;src/nova_fiber_candidate_type.cell:nova_fiber_candidate_lifecycle"),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "docs_fiber_execution_claim_current".to_string(),
            Value::Bool(
                audit_doc.contains("live-fiber-node-execution-evidence")
                    && audit_doc.contains("Fiber workflow discovery | live-fiber-workflow-suite-evidence")
                    && !audit_doc.contains("pending-fiber-node-suite-execution")
                    && !audit_doc.contains("discovery-ready-live-not-run")
                    && devnet_doc.contains("all required Fiber workflow suites")
                    && devnet_doc.contains("executed and passed")
                    && !devnet_doc.contains("workflow execution remains pending")
                    && security_doc.contains("external Fiber workflow execution evidence is present")
                    && security_doc.contains("does not verify Fiber HTLCs, routes, liquidity, fees, or revocations"),
            ),
        ),
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
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-fiber-candidate-devnet-stateful-live.json"),
            ),
        ),
        (
            "fiber_execution_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("fiber_execution").and_then(Value::as_str) == Some(FIBER_NODE_EXPERIMENTS)),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-fiber-candidate-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-with-live-stateful-and-fiber-node-execution-evidence",
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
        "operator_fixture_evidence": PROFILE_OPERATOR_FIXTURES,
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

fn validate_btc_spv_evidence(repo_root: &Path, rel_path: &str, external_evidence_handoff: &Value) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.is_file() {
        return Ok(json!({
            "status": "external_required",
            "reason": "missing public BTC SPV evidence",
            "required_report": rel_path,
            "template": PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE,
            "required_handoff": EXTERNAL_EVIDENCE_HANDOFF,
            "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
        }));
    }
    let payload = json_load(repo_root, rel_path)?;
    let handoff_hash = external_evidence_handoff_reference_hash(external_evidence_handoff);
    let handoff_case = handoff_case(external_evidence_handoff, "public_btc_spv_evidence").unwrap_or(&Value::Null);
    let handoff_required_profiles = json_array_strings(handoff_case, "/required_profiles");
    let handoff_expected_scenarios = json_object_string_map(handoff_case.get("expected_scenarios").unwrap_or(&Value::Null));
    let handoff_expected_bindings = handoff_case.get("expected_case_bindings").unwrap_or(&Value::Null);
    let cases = payload.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let covered_profile_list =
        cases.iter().filter_map(|case| json_pointer_str(case, "/profile").map(str::to_string)).collect::<Vec<_>>();
    let covered_scenarios = cases
        .iter()
        .filter_map(|case| {
            let profile = json_pointer_str(case, "/profile")?;
            let scenario = json_pointer_str(case, "/scenario")?;
            Some((profile.to_string(), scenario.to_string()))
        })
        .collect::<BTreeMap<_, _>>();
    let covered_profiles = covered_profile_list.iter().cloned().collect::<BTreeSet<_>>();
    let required_profiles = EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().map(|profile| (*profile).to_string()).collect::<BTreeSet<_>>();
    let mut case_checks = Map::new();
    for profile in EXPECTED_BTC_SPV_EVIDENCE_PROFILES {
        let Some(case) = cases.iter().find(|case| json_pointer_str(case, "/profile") == Some(*profile)) else {
            case_checks.insert((*profile).to_string(), json!({"present": false}));
            continue;
        };
        let cell_dep = case.get("spv_client_cell_dep").unwrap_or(&Value::Null);
        let source_service = case.get("source_service").unwrap_or(&Value::Null);
        let merkle_proof = case.get("btc_merkle_proof").unwrap_or(&Value::Null);
        let out_point = parse_out_point(json_pointer_str(cell_dep, "/out_point"));
        let hash_type = json_pointer_str(cell_dep, "/hash_type");
        let confirmations = json_pointer_i64(case, "/confirmations").unwrap_or_default();
        let minimum_confirmations = json_pointer_i64(case, "/minimum_confirmations").unwrap_or_default();
        let expected_binding = handoff_expected_bindings.get(*profile).unwrap_or(&Value::Null);
        let proof_checks = validate_btc_spv_case_proof(case, confirmations);
        let tx_checks = validate_btc_transaction_binding(profile, case, expected_binding);
        let mut checks = Map::new();
        macro_rules! check {
            ($name:literal, $value:expr) => {
                checks.insert($name.to_string(), Value::Bool($value));
            };
        }
        check!("present", true);
        check!("fields_exact", exact_object_keys(case, EXPECTED_PUBLIC_BTC_SPV_CASE_FIELDS));
        check!("scenario_matches_expected", json_pointer_str(case, "/scenario") == expected_btc_spv_scenario(profile));
        check!(
            "scenario_matches_handoff",
            handoff_expected_scenarios
                .get(*profile)
                .is_some_and(|scenario| json_pointer_str(case, "/scenario") == Some(scenario.as_str()))
        );
        check!("ckb_live_tx_hash_valid", json_pointer_str(case, "/ckb_live_tx_hash").is_some_and(is_hex32));
        check!("ckb_live_tx_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/ckb_live_tx_hash")));
        check!(
            "ckb_live_tx_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/ckb_live_tx_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/ckb_live_tx_hash")).as_deref()
        );
        check!("live_report_hash_valid", json_pointer_str(case, "/live_report_hash").is_some_and(is_hex32));
        check!("live_report_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/live_report_hash")));
        check!(
            "live_report_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/live_report_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/live_report_hash")).as_deref()
        );
        check!("service_builder_case_hash_valid", json_pointer_str(case, "/service_builder_case_hash").is_some_and(is_hex32));
        check!("service_builder_case_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/service_builder_case_hash")));
        check!(
            "service_builder_case_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/service_builder_case_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/service_builder_case_hash")).as_deref()
        );
        check!(
            "service_builder_tx_skeleton_hash_valid",
            json_pointer_str(case, "/service_builder_tx_skeleton_hash").is_some_and(is_hex32)
        );
        check!(
            "service_builder_tx_skeleton_hash_non_placeholder",
            !placeholder_hash(json_pointer_str(case, "/service_builder_tx_skeleton_hash"))
        );
        check!(
            "service_builder_tx_skeleton_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/service_builder_tx_skeleton_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/service_builder_tx_skeleton_hash")).as_deref()
        );
        check!(
            "service_builder_receipt_binding_hash_valid",
            json_pointer_str(case, "/service_builder_receipt_binding_hash").is_some_and(is_hex32)
        );
        check!(
            "service_builder_receipt_binding_hash_non_placeholder",
            !placeholder_hash(json_pointer_str(case, "/service_builder_receipt_binding_hash"))
        );
        check!(
            "service_builder_receipt_binding_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/service_builder_receipt_binding_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/service_builder_receipt_binding_hash")).as_deref()
        );
        check!("ckb_btc_commitment_hash_valid", json_pointer_str(case, "/ckb_btc_commitment_hash").is_some_and(is_hex32));
        check!("ckb_btc_commitment_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/ckb_btc_commitment_hash")));
        check!(
            "ckb_btc_commitment_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/ckb_btc_commitment_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/ckb_btc_commitment_hash")).as_deref()
        );
        check!("btc_txid_valid", json_pointer_str(case, "/btc_txid").is_some_and(is_hex32));
        check!("btc_txid_non_placeholder", !placeholder_hash(json_pointer_str(case, "/btc_txid")));
        check!(
            "btc_txid_matches_handoff_when_bound",
            json_pointer_str(expected_binding, "/btc_txid").is_none_or(|expected| {
                normalize_hex(json_pointer_str(case, "/btc_txid")).as_deref() == normalize_hex(Some(expected)).as_deref()
            })
        );
        check!("btc_wtxid_valid", json_pointer_str(case, "/btc_wtxid").is_some_and(is_hex32));
        check!("btc_wtxid_non_placeholder", !placeholder_hash(json_pointer_str(case, "/btc_wtxid")));
        check!(
            "btc_wtxid_matches_handoff_when_bound",
            json_pointer_str(expected_binding, "/btc_wtxid").is_none_or(|expected| {
                normalize_hex(json_pointer_str(case, "/btc_wtxid")).as_deref() == normalize_hex(Some(expected)).as_deref()
            })
        );
        check!("btc_tx_hex_valid", json_pointer_bool(&tx_checks, "/btc_tx_hex_valid"));
        check!("btc_txid_matches_tx_hex", json_pointer_bool(&tx_checks, "/btc_txid_matches_tx_hex"));
        check!("btc_wtxid_matches_tx_hex", json_pointer_bool(&tx_checks, "/btc_wtxid_matches_tx_hex"));
        check!("btc_transaction_binding_fields_exact", json_pointer_bool(&tx_checks, "/binding_fields_exact"));
        check!("btc_transaction_binding_kind_matches_profile", json_pointer_bool(&tx_checks, "/binding_kind_matches_profile"));
        check!("btc_transaction_binding_matches_handoff", json_pointer_bool(&tx_checks, "/binding_matches_handoff"));
        check!("btc_transaction_output_matches_anchor", json_pointer_bool(&tx_checks, "/transaction_output_matches_anchor"));
        check!("btc_utxo_spend_input_matches_anchor", json_pointer_bool(&tx_checks, "/utxo_spend_input_matches_anchor"));
        check!("btc_utxo_sealed_tx_matches_anchor", json_pointer_bool(&tx_checks, "/utxo_sealed_tx_matches_anchor"));
        check!(
            "btc_utxo_sealed_utxo_commitment_matches_tuple",
            json_pointer_bool(&tx_checks, "/utxo_sealed_utxo_commitment_matches_tuple")
        );
        check!("btc_dual_spend_input_matches_anchor", json_pointer_bool(&tx_checks, "/dual_spend_input_matches_anchor"));
        check!("btc_dual_sealed_tx_matches_anchor", json_pointer_bool(&tx_checks, "/dual_sealed_tx_matches_anchor"));
        check!(
            "btc_dual_sealed_utxo_commitment_matches_tuple",
            json_pointer_bool(&tx_checks, "/dual_sealed_utxo_commitment_matches_tuple")
        );
        check!("btc_block_hash_valid", json_pointer_str(case, "/btc_block_hash").is_some_and(is_hex32));
        check!("btc_block_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/btc_block_hash")));
        check!("btc_block_header_valid", json_pointer_str(case, "/btc_block_header").is_some_and(|value| is_hex_bytes_len(value, 80)));
        check!("btc_block_hash_matches_header", json_pointer_bool(&proof_checks, "/block_hash_matches_header"));
        check!("btc_merkle_proof_fields_exact", exact_object_keys(merkle_proof, EXPECTED_PUBLIC_BTC_SPV_MERKLE_PROOF_FIELDS));
        check!("btc_merkle_proof_tx_index_non_negative", json_pointer_i64(merkle_proof, "/tx_index").is_some_and(|value| value >= 0));
        check!("btc_merkle_proof_branch_valid", json_pointer_bool(&proof_checks, "/merkle_branch_valid"));
        check!("btc_merkle_proof_merkle_root_valid", json_pointer_str(merkle_proof, "/merkle_root").is_some_and(is_hex32));
        check!("btc_merkle_proof_merkle_root_non_placeholder", !placeholder_hash(json_pointer_str(merkle_proof, "/merkle_root")));
        check!("btc_merkle_root_matches_header", json_pointer_bool(&proof_checks, "/merkle_root_matches_header"));
        check!("btc_merkle_branch_verifies_txid", json_pointer_bool(&proof_checks, "/merkle_branch_verifies_txid"));
        check!("btc_confirmations_match_heights", json_pointer_bool(&proof_checks, "/confirmations_match_heights"));
        check!("spv_proof_hash_valid", json_pointer_str(case, "/spv_proof_hash").is_some_and(is_hex32));
        check!("spv_proof_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/spv_proof_hash")));
        check!("spv_proof_hash_matches_material", json_pointer_bool(&proof_checks, "/proof_hash_matches_material"));
        check!("minimum_confirmations_at_least_six", minimum_confirmations >= 6);
        check!("confirmations_meet_minimum", confirmations >= minimum_confirmations && minimum_confirmations >= 6);
        check!("spv_client_cell_dep_fields_exact", exact_object_keys(cell_dep, EXPECTED_PUBLIC_BTC_SPV_CELLDEP_FIELDS));
        check!("spv_client_cell_dep_out_point_valid", json_pointer_bool(&out_point, "/valid"));
        check!("spv_client_cell_dep_out_point_non_placeholder", !placeholder_hash(json_pointer_str(&out_point, "/tx_hash")));
        check!("spv_client_cell_dep_data_hash_valid", json_pointer_str(cell_dep, "/data_hash").is_some_and(is_hex32));
        check!("spv_client_cell_dep_data_hash_non_placeholder", !placeholder_hash(json_pointer_str(cell_dep, "/data_hash")));
        check!("spv_client_cell_dep_dep_type", json_pointer_str(cell_dep, "/dep_type") == Some("code"));
        check!("spv_client_cell_dep_hash_type", matches!(hash_type, Some("data" | "data1" | "type")));
        check!("source_service_fields_exact", exact_object_keys(source_service, EXPECTED_PUBLIC_BTC_SPV_SOURCE_SERVICE_FIELDS));
        check!("source_service_name_present", source_service.get("name").is_some_and(value_is_present));
        check!("source_service_name_identity", json_pointer_str(source_service, "/name").is_some_and(is_external_identity));
        check!("source_service_commit_40_hex", json_pointer_str(source_service, "/commit").is_some_and(is_git_commit_hash));
        check!("source_service_report_hash_valid", json_pointer_str(source_service, "/report_hash").is_some_and(is_hex32));
        check!("source_service_report_hash_non_placeholder", !placeholder_hash(json_pointer_str(source_service, "/report_hash")));
        case_checks.insert((*profile).to_string(), Value::Object(checks));
    }
    let case_checks_passed = case_checks.values().all(|checks| object_values_all_true(Some(checks)));
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-public-btc-spv-evidence-v0.1"),
        "top_level_fields_exact": exact_object_keys(&payload, EXPECTED_PUBLIC_BTC_SPV_EVIDENCE_FIELDS),
        "status_attested": json_pointer_str(&payload, "/status") == Some("attested"),
        "network_public": json_pointer_str(&payload, "/network").is_some_and(is_public_network),
        "evidence_provider_present": payload.get("evidence_provider").is_some_and(value_is_present),
        "evidence_provider_identity": json_pointer_str(&payload, "/evidence_provider").is_some_and(is_external_identity),
        "generated_at_present": payload.get("generated_at").is_some_and(value_is_present),
        "generated_at_utc_timestamp": json_pointer_str(&payload, "/generated_at").is_some_and(is_utc_timestamp_z),
        "generated_at_not_future": json_pointer_str(&payload, "/generated_at").is_some_and(is_utc_timestamp_z_not_future),
        "request_handoff_fields_exact": exact_object_keys(payload.get("request_handoff").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS),
        "request_handoff_bundle_path": json_pointer_str(&payload, "/request_handoff/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
        "request_handoff_bundle_hash_matches_current": normalize_hex(json_pointer_str(&payload, "/request_handoff/bundle_hash")).as_deref()
            == Some(handoff_hash.as_str()),
        "request_handoff_bundle_hash_algorithm": json_pointer_str(&payload, "/request_handoff/bundle_hash_algorithm")
            == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "request_handoff_group": json_pointer_str(&payload, "/request_handoff/group") == Some("public_btc_spv_evidence"),
        "handoff_required_profiles_exact": exact_string_set(&handoff_required_profiles, EXPECTED_BTC_SPV_EVIDENCE_PROFILES),
        "handoff_expected_scenarios_exact": handoff_expected_scenarios == EXPECTED_BTC_SPV_PROFILE_SCENARIOS
            .iter()
            .map(|(profile, scenario)| ((*profile).to_string(), (*scenario).to_string()))
            .collect::<BTreeMap<_, _>>(),
        "handoff_expected_case_bindings_exact": handoff_expected_bindings_exact(handoff_expected_bindings),
        "required_profiles_field_exact": exact_string_set(&json_array_strings(&payload, "/required_profiles"), EXPECTED_BTC_SPV_EVIDENCE_PROFILES),
        "required_profiles_match_handoff": json_array_strings(&payload, "/required_profiles") == handoff_required_profiles,
        "required_profiles_covered_exact": exact_string_set(&covered_profile_list, EXPECTED_BTC_SPV_EVIDENCE_PROFILES),
        "covered_scenarios_match_handoff": covered_scenarios == handoff_expected_scenarios,
        "case_checks_passed": case_checks_passed,
    });
    let missing_profiles = required_profiles.difference(&covered_profiles).cloned().collect::<Vec<_>>();
    let extra_profiles = covered_profiles.difference(&required_profiles).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-public-btc-spv-evidence-validation-v0.1",
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "path": rel(repo_root, &path),
        "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
        "covered_profiles": covered_profiles.into_iter().collect::<Vec<_>>(),
        "missing_profiles": missing_profiles,
        "extra_profiles": extra_profiles,
        "checks": checks,
        "case_checks": case_checks,
        "evidence": payload,
    }))
}

fn validate_public_attestation(
    repo_root: &Path,
    rel_path: &str,
    artifact_hash: Option<&str>,
    tcb_repo_commit: Option<&str>,
    external_evidence_handoff: &Value,
) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({
            "status": "external_required",
            "reason": "missing public/shared CellDep attestation",
            "required_report": rel_path,
            "required_handoff": EXTERNAL_EVIDENCE_HANDOFF,
        }));
    }
    let payload = json_load_path(repo_root, &path)?;
    let handoff_hash = external_evidence_handoff_reference_hash(external_evidence_handoff);
    let verifier = payload.get("runtime_verifier").cloned().unwrap_or(Value::Null);
    let release = payload.get("release").cloned().unwrap_or(Value::Null);
    let handoff_expected_values =
        handoff_case_expected_values(external_evidence_handoff, "public_shared_cell_dep_attestation").unwrap_or(&Value::Null);
    let parsed = parse_out_point(json_pointer_str(&verifier, "/out_point"));
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-public-shared-cell-dep-attestation-v0.1"),
        "top_level_fields_exact": exact_object_keys(&payload, EXPECTED_PUBLIC_CELLDEP_ATTESTATION_FIELDS),
        "status": json_pointer_str(&payload, "/status") == Some("attested"),
        "network_public": json_pointer_str(&payload, "/network").is_some_and(is_public_network),
        "attested_at_utc_timestamp": json_pointer_str(&payload, "/attested_at").is_some_and(is_utc_timestamp_z),
        "attested_at_not_future": json_pointer_str(&payload, "/attested_at").is_some_and(is_utc_timestamp_z_not_future),
        "attestor_identity": json_pointer_str(&payload, "/attestor").is_some_and(is_external_identity),
        "release_fields_exact": exact_object_keys(&release, EXPECTED_PUBLIC_CELLDEP_RELEASE_FIELDS),
        "release_package": json_pointer_str(&release, "/package") == Some("novaseal"),
        "release_version_present": release.get("version").is_some_and(value_is_present),
        "release_version_matches_expected": json_pointer_str(&release, "/version") == Some(EXPECTED_NOVASEAL_RELEASE_VERSION),
        "release_manifest_commit_present": json_pointer_str(&release, "/manifest_commit").is_some_and(is_git_commit_hash),
        "release_manifest_commit_matches_tcb": json_pointer_str(&release, "/manifest_commit") == tcb_repo_commit,
        "request_handoff_fields_exact": exact_object_keys(payload.get("request_handoff").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS),
        "request_handoff_bundle_path": json_pointer_str(&payload, "/request_handoff/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
        "request_handoff_bundle_hash_matches_current": normalize_hex(json_pointer_str(&payload, "/request_handoff/bundle_hash")).as_deref()
            == Some(handoff_hash.as_str()),
        "request_handoff_bundle_hash_algorithm": json_pointer_str(&payload, "/request_handoff/bundle_hash_algorithm")
            == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "request_handoff_group": json_pointer_str(&payload, "/request_handoff/group") == Some("public_shared_cell_dep_attestation"),
        "handoff_expected_values_exact": exact_object_keys(handoff_expected_values, EXPECTED_PUBLIC_CELLDEP_EXPECTED_VALUE_FIELDS),
        "release_package_matches_handoff": json_pointer_str(&release, "/package")
            == json_pointer_str(handoff_expected_values, "/release.package"),
        "release_version_matches_handoff": json_pointer_str(&release, "/version")
            == json_pointer_str(handoff_expected_values, "/release.version"),
        "release_manifest_commit_matches_handoff": json_pointer_str(&release, "/manifest_commit")
            == json_pointer_str(handoff_expected_values, "/release.manifest_commit"),
        "runtime_verifier_fields_exact": exact_object_keys(&verifier, EXPECTED_PUBLIC_CELLDEP_RUNTIME_VERIFIER_FIELDS),
        "artifact_hash_valid": normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref().is_some_and(is_hex32),
        "artifact_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref()),
        "artifact_hash": normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref() == artifact_hash,
        "artifact_hash_matches_handoff": normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref()
            == normalize_hex(json_pointer_str(handoff_expected_values, "/artifact_hash")).as_deref(),
        "data_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&verifier, "/data_hash")).as_deref()),
        "out_point_valid": json_pointer_bool(&parsed, "/valid"),
        "out_point_non_placeholder": !placeholder_hash(json_pointer_str(&parsed, "/tx_hash")),
        "dep_type": json_pointer_str(&verifier, "/dep_type") == Some(EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE),
        "dep_type_matches_handoff": json_pointer_str(&verifier, "/dep_type")
            == json_pointer_str(handoff_expected_values, "/runtime_verifier.dep_type"),
        "hash_type": matches!(json_pointer_str(&verifier, "/hash_type"), Some("data" | "data1" | "type")),
        "hash_type_matches_expected": json_pointer_str(&verifier, "/hash_type") == Some(EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE),
        "hash_type_matches_handoff": json_pointer_str(&verifier, "/hash_type")
            == json_pointer_str(handoff_expected_values, "/runtime_verifier.hash_type"),
        "data_hash_valid": normalize_hex(json_pointer_str(&verifier, "/data_hash")).as_deref().is_some_and(is_hex32),
        "verifier_id": json_pointer_str(&verifier, "/verifier_id") == Some("btc.bip340.v0"),
        "verifier_id_matches_handoff": json_pointer_str(&verifier, "/verifier_id")
            == json_pointer_str(handoff_expected_values, "/runtime_verifier.verifier_id"),
        "ipc_abi": json_pointer_str(&verifier, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
        "ipc_abi_matches_handoff": json_pointer_str(&verifier, "/ipc_abi")
            == json_pointer_str(handoff_expected_values, "/runtime_verifier.ipc_abi"),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "attestation": payload,
    }))
}

fn validate_external_review(
    repo_root: &Path,
    rel_path: &str,
    artifact_hash: Option<&str>,
    source_tree_hash: Option<&str>,
    external_evidence_handoff: &Value,
) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({
            "status": "external_required",
            "reason": "missing external BIP340 TCB review attestation",
            "required_report": rel_path,
            "required_handoff": EXTERNAL_EVIDENCE_HANDOFF,
        }));
    }
    let payload = json_load_path(repo_root, &path)?;
    let handoff_hash = external_evidence_handoff_reference_hash(external_evidence_handoff);
    let handoff_expected_values =
        handoff_case_expected_values(external_evidence_handoff, "external_bip340_tcb_review_attestation").unwrap_or(&Value::Null);
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-bip340-external-tcb-review-attestation-v0.1"),
        "top_level_fields_exact": exact_object_keys(&payload, EXPECTED_EXTERNAL_TCB_REVIEW_ATTESTATION_FIELDS),
        "status": json_pointer_str(&payload, "/status") == Some("accepted"),
        "request_handoff_fields_exact": exact_object_keys(payload.get("request_handoff").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS),
        "request_handoff_bundle_path": json_pointer_str(&payload, "/request_handoff/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
        "request_handoff_bundle_hash_matches_current": normalize_hex(json_pointer_str(&payload, "/request_handoff/bundle_hash")).as_deref()
            == Some(handoff_hash.as_str()),
        "request_handoff_bundle_hash_algorithm": json_pointer_str(&payload, "/request_handoff/bundle_hash_algorithm")
            == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "request_handoff_group": json_pointer_str(&payload, "/request_handoff/group") == Some("external_bip340_tcb_review_attestation"),
        "handoff_expected_values_exact": exact_object_keys(handoff_expected_values, EXPECTED_EXTERNAL_TCB_EXPECTED_VALUE_FIELDS),
        "artifact_hash_valid": normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref().is_some_and(is_hex32),
        "artifact_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref()),
        "artifact_hash": normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref() == artifact_hash,
        "artifact_hash_matches_handoff": normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref()
            == normalize_hex(json_pointer_str(handoff_expected_values, "/artifact_hash")).as_deref(),
        "artifact_hash_algorithm": json_pointer_str(&payload, "/artifact_hash_algorithm") == Some("sha256"),
        "artifact_hash_algorithm_matches_handoff": json_pointer_str(&payload, "/artifact_hash_algorithm")
            == json_pointer_str(handoff_expected_values, "/artifact_hash_algorithm"),
        "source_tree_sha256_valid": normalize_hex(json_pointer_str(&payload, "/source_tree_sha256")).as_deref().is_some_and(is_hex32),
        "source_tree_sha256_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&payload, "/source_tree_sha256")).as_deref()),
        "source_tree_sha256_matches_current_tcb": normalize_hex(json_pointer_str(&payload, "/source_tree_sha256")).as_deref() == source_tree_hash,
        "source_tree_sha256_matches_handoff": normalize_hex(json_pointer_str(&payload, "/source_tree_sha256")).as_deref()
            == normalize_hex(json_pointer_str(handoff_expected_values, "/source_tree_sha256")).as_deref(),
        "verifier_id": json_pointer_str(&payload, "/verifier_id") == Some("btc.bip340.v0"),
        "verifier_id_matches_handoff": json_pointer_str(&payload, "/verifier_id")
            == json_pointer_str(handoff_expected_values, "/verifier_id"),
        "ipc_abi": json_pointer_str(&payload, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
        "ipc_abi_matches_handoff": json_pointer_str(&payload, "/ipc_abi")
            == json_pointer_str(handoff_expected_values, "/ipc_abi"),
        "reviewer_present": json_pointer_str(&payload, "/reviewer").is_some_and(|value| !value.is_empty()),
        "reviewer_identity": json_pointer_str(&payload, "/reviewer").is_some_and(is_external_identity),
        "review_date_present": json_pointer_str(&payload, "/review_date").is_some_and(|value| !value.is_empty()),
        "review_date_utc_date": json_pointer_str(&payload, "/review_date").is_some_and(is_utc_date),
        "review_date_not_future": json_pointer_str(&payload, "/review_date").is_some_and(is_utc_date_not_future),
        "report_uri_https": json_pointer_str(&payload, "/report_uri").is_some_and(is_https_report_uri),
        "review_scope_exact": exact_string_set(&json_array_strings(&payload, "/review_scope"), EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE),
        "review_scope_matches_handoff": json_array_strings(&payload, "/review_scope")
            == json_array_strings(handoff_expected_values, "/review_scope"),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "attestation": payload,
    }))
}

fn validate_rwa_legal_registry_review(repo_root: &Path, rel_path: &str, external_evidence_handoff: &Value) -> Result<Value> {
    let source_hash = source_tree_hash(repo_root, RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)?;
    let expected_source_hash = normalize_hex(json_pointer_str(&source_hash, "/sha256"));
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({
            "status": "external_required",
            "reason": "missing RWA legal/registry review evidence",
            "required_report": rel_path,
            "required_template": RWA_LEGAL_REGISTRY_REVIEW_TEMPLATE,
            "required_handoff": EXTERNAL_EVIDENCE_HANDOFF,
            "expected_profile_source_tree_sha256": expected_source_hash,
        }));
    }
    let payload = json_load_path(repo_root, &path)?;
    let registry = payload.get("registry").cloned().unwrap_or(Value::Null);
    let handoff_hash = external_evidence_handoff_reference_hash(external_evidence_handoff);
    let handoff_expected_values =
        handoff_case_expected_values(external_evidence_handoff, "rwa_legal_registry_review_evidence").unwrap_or(&Value::Null);
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-rwa-legal-registry-review-evidence-v0.1"),
        "top_level_fields_exact": exact_object_keys(&payload, EXPECTED_RWA_LEGAL_REVIEW_EVIDENCE_FIELDS),
        "status": json_pointer_str(&payload, "/status") == Some("accepted"),
        "profile": json_pointer_str(&payload, "/profile") == Some(EXPECTED_RWA_RECEIPT_PROFILE),
        "profile_matches_handoff": json_pointer_str(&payload, "/profile") == json_pointer_str(handoff_expected_values, "/profile"),
        "request_handoff_fields_exact": exact_object_keys(payload.get("request_handoff").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS),
        "request_handoff_bundle_path": json_pointer_str(&payload, "/request_handoff/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
        "request_handoff_bundle_hash_matches_current": normalize_hex(json_pointer_str(&payload, "/request_handoff/bundle_hash")).as_deref()
            == Some(handoff_hash.as_str()),
        "request_handoff_bundle_hash_algorithm": json_pointer_str(&payload, "/request_handoff/bundle_hash_algorithm")
            == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "request_handoff_group": json_pointer_str(&payload, "/request_handoff/group") == Some("rwa_legal_registry_review_evidence"),
        "handoff_expected_values_exact": exact_object_keys(handoff_expected_values, EXPECTED_RWA_LEGAL_REVIEW_EXPECTED_VALUE_FIELDS),
        "reviewer_present": json_pointer_str(&payload, "/reviewer").is_some_and(|value| !value.is_empty()),
        "reviewer_identity": json_pointer_str(&payload, "/reviewer").is_some_and(is_external_identity),
        "review_date_present": json_pointer_str(&payload, "/review_date").is_some_and(|value| !value.is_empty()),
        "review_date_utc_date": json_pointer_str(&payload, "/review_date").is_some_and(is_utc_date),
        "review_date_not_future": json_pointer_str(&payload, "/review_date").is_some_and(is_utc_date_not_future),
        "report_uri_https": json_pointer_str(&payload, "/report_uri").is_some_and(is_https_report_uri),
        "review_scope_exact": exact_string_set(&json_array_strings(&payload, "/review_scope"), EXPECTED_RWA_LEGAL_REVIEW_SCOPE),
        "review_scope_matches_handoff": json_array_strings(&payload, "/review_scope")
            == json_array_strings(handoff_expected_values, "/review_scope"),
        "registry_fields_exact": exact_object_keys(&registry, EXPECTED_RWA_LEGAL_REVIEW_REGISTRY_FIELDS),
        "registry_authority_identity": json_pointer_str(&registry, "/authority").is_some_and(is_external_identity),
        "registry_jurisdiction_present": json_pointer_str(&registry, "/jurisdiction").is_some_and(|value| {
            value_is_present(&Value::String(value.to_string()))
                && !contains_placeholder_token(value)
                && !contains_local_only_token(value)
        }),
        "registry_report_hash_valid": normalize_hex(json_pointer_str(&registry, "/registry_report_hash")).as_deref().is_some_and(is_hex32),
        "registry_report_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&registry, "/registry_report_hash")).as_deref()),
        "profile_source_tree_sha256_valid": normalize_hex(json_pointer_str(&payload, "/profile_source_tree_sha256")).as_deref().is_some_and(is_hex32),
        "profile_source_tree_sha256_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&payload, "/profile_source_tree_sha256")).as_deref()),
        "profile_source_tree_sha256_matches_current": normalize_hex(json_pointer_str(&payload, "/profile_source_tree_sha256")) == expected_source_hash,
        "profile_source_tree_sha256_matches_handoff": normalize_hex(json_pointer_str(&payload, "/profile_source_tree_sha256"))
            == normalize_hex(json_pointer_str(handoff_expected_values, "/profile_source_tree_sha256")),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "evidence": payload,
        "profile_source_tree": source_hash,
    }))
}

fn validate_attestation_templates(
    repo_root: &Path,
    artifact_hash: Option<&str>,
    artifact_hash_algorithm: Option<&str>,
    expected_tcb_source_tree_hash: Option<&str>,
) -> Result<Value> {
    let public_path = repo_root.join(PUBLIC_CELLDEP_ATTESTATION_TEMPLATE);
    let external_path = repo_root.join(EXTERNAL_TCB_ATTESTATION_TEMPLATE);
    let btc_spv_path = repo_root.join(PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE);
    let rwa_legal_path = repo_root.join(RWA_LEGAL_REGISTRY_REVIEW_TEMPLATE);
    let public_payload = if public_path.is_file() { Some(json_load_path(repo_root, &public_path)?) } else { None };
    let external_payload = if external_path.is_file() { Some(json_load_path(repo_root, &external_path)?) } else { None };
    let btc_spv_payload = if btc_spv_path.is_file() { Some(json_load_path(repo_root, &btc_spv_path)?) } else { None };
    let rwa_legal_payload = if rwa_legal_path.is_file() { Some(json_load_path(repo_root, &rwa_legal_path)?) } else { None };
    let public = public_payload.as_ref().unwrap_or(&Value::Null);
    let external = external_payload.as_ref().unwrap_or(&Value::Null);
    let btc_spv = btc_spv_payload.as_ref().unwrap_or(&Value::Null);
    let rwa_legal = rwa_legal_payload.as_ref().unwrap_or(&Value::Null);
    let public_release = public.get("release").unwrap_or(&Value::Null);
    let public_verifier = public.get("runtime_verifier").unwrap_or(&Value::Null);
    let public_handoff = public.get("request_handoff").unwrap_or(&Value::Null);
    let external_handoff = external.get("request_handoff").unwrap_or(&Value::Null);
    let btc_spv_handoff = btc_spv.get("request_handoff").unwrap_or(&Value::Null);
    let rwa_legal_handoff = rwa_legal.get("request_handoff").unwrap_or(&Value::Null);
    let rwa_legal_registry = rwa_legal.get("registry").unwrap_or(&Value::Null);
    let rwa_source_hash = source_tree_hash(repo_root, RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)?;
    let btc_spv_profiles = btc_spv
        .get("required_profiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let expected_btc_spv_profiles = EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().copied().collect::<BTreeSet<_>>();
    let checks = Value::Object(
        [
            ("public_template_present", public_path.is_file()),
            ("external_template_present", external_path.is_file()),
            ("btc_spv_template_present", btc_spv_path.is_file()),
            ("rwa_legal_template_present", rwa_legal_path.is_file()),
            ("public_schema", json_pointer_str(public, "/schema") == Some("novaseal-public-shared-cell-dep-attestation-v0.1")),
            ("external_schema", json_pointer_str(external, "/schema") == Some("novaseal-bip340-external-tcb-review-attestation-v0.1")),
            ("btc_spv_schema", json_pointer_str(btc_spv, "/schema") == Some("novaseal-public-btc-spv-evidence-v0.1")),
            ("rwa_legal_schema", json_pointer_str(rwa_legal, "/schema") == Some("novaseal-rwa-legal-registry-review-evidence-v0.1")),
            ("public_top_level_fields_exact", exact_object_keys(public, EXPECTED_PUBLIC_CELLDEP_ATTESTATION_FIELDS)),
            ("public_release_fields_exact", exact_object_keys(public_release, EXPECTED_PUBLIC_CELLDEP_RELEASE_FIELDS)),
            ("public_release_package", json_pointer_str(public_release, "/package") == Some("novaseal")),
            (
                "public_release_version_current",
                json_pointer_str(public_release, "/version") == Some(EXPECTED_NOVASEAL_RELEASE_VERSION),
            ),
            ("public_release_manifest_commit_present", public_release.get("manifest_commit").is_some_and(value_is_present)),
            ("public_request_handoff_fields_exact", exact_object_keys(public_handoff, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS)),
            ("public_request_handoff_bundle_path", json_pointer_str(public_handoff, "/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF)),
            (
                "public_request_handoff_hash_algorithm",
                json_pointer_str(public_handoff, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
            ),
            ("public_request_handoff_group", json_pointer_str(public_handoff, "/group") == Some("public_shared_cell_dep_attestation")),
            (
                "public_runtime_verifier_fields_exact",
                exact_object_keys(public_verifier, EXPECTED_PUBLIC_CELLDEP_RUNTIME_VERIFIER_FIELDS),
            ),
            ("external_top_level_fields_exact", exact_object_keys(external, EXPECTED_EXTERNAL_TCB_REVIEW_ATTESTATION_FIELDS)),
            ("external_request_handoff_fields_exact", exact_object_keys(external_handoff, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS)),
            ("external_request_handoff_bundle_path", json_pointer_str(external_handoff, "/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF)),
            (
                "external_request_handoff_hash_algorithm",
                json_pointer_str(external_handoff, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
            ),
            (
                "external_request_handoff_group",
                json_pointer_str(external_handoff, "/group") == Some("external_bip340_tcb_review_attestation"),
            ),
            ("external_artifact_hash_algorithm", json_pointer_str(external, "/artifact_hash_algorithm") == Some("sha256")),
            (
                "external_artifact_hash_algorithm_matches_current_tcb",
                json_pointer_str(external, "/artifact_hash_algorithm") == artifact_hash_algorithm,
            ),
            ("btc_spv_top_level_fields_exact", exact_object_keys(btc_spv, EXPECTED_PUBLIC_BTC_SPV_EVIDENCE_FIELDS)),
            ("btc_spv_request_handoff_fields_exact", exact_object_keys(btc_spv_handoff, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS)),
            ("btc_spv_request_handoff_bundle_path", json_pointer_str(btc_spv_handoff, "/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF)),
            (
                "btc_spv_request_handoff_hash_algorithm",
                json_pointer_str(btc_spv_handoff, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
            ),
            ("btc_spv_request_handoff_group", json_pointer_str(btc_spv_handoff, "/group") == Some("public_btc_spv_evidence")),
            ("btc_spv_required_profiles_exact", btc_spv_profiles == expected_btc_spv_profiles),
            ("rwa_legal_top_level_fields_exact", exact_object_keys(rwa_legal, EXPECTED_RWA_LEGAL_REVIEW_EVIDENCE_FIELDS)),
            ("rwa_legal_registry_fields_exact", exact_object_keys(rwa_legal_registry, EXPECTED_RWA_LEGAL_REVIEW_REGISTRY_FIELDS)),
            ("rwa_legal_request_handoff_fields_exact", exact_object_keys(rwa_legal_handoff, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS)),
            (
                "rwa_legal_request_handoff_bundle_path",
                json_pointer_str(rwa_legal_handoff, "/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
            ),
            (
                "rwa_legal_request_handoff_hash_algorithm",
                json_pointer_str(rwa_legal_handoff, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
            ),
            (
                "rwa_legal_request_handoff_group",
                json_pointer_str(rwa_legal_handoff, "/group") == Some("rwa_legal_registry_review_evidence"),
            ),
            ("rwa_legal_profile", json_pointer_str(rwa_legal, "/profile") == Some(EXPECTED_RWA_RECEIPT_PROFILE)),
            (
                "rwa_legal_review_scope_exact",
                exact_string_set(&json_array_strings(rwa_legal, "/review_scope"), EXPECTED_RWA_LEGAL_REVIEW_SCOPE),
            ),
            (
                "rwa_legal_profile_source_tree_hash_matches_current",
                normalize_hex(json_pointer_str(rwa_legal, "/profile_source_tree_sha256")).as_deref()
                    == normalize_hex(json_pointer_str(&rwa_source_hash, "/sha256")).as_deref(),
            ),
            (
                "public_template_network_not_local_devnet",
                json_pointer_str(public, "/network").is_some_and(|network| !network.is_empty() && network != "local-devnet"),
            ),
            (
                "public_artifact_hash_matches_current_tcb",
                normalize_hex(json_pointer_str(public_verifier, "/artifact_hash")).as_deref() == artifact_hash,
            ),
            ("public_dep_type", json_pointer_str(public_verifier, "/dep_type") == Some(EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE)),
            ("public_hash_type", matches!(json_pointer_str(public_verifier, "/hash_type"), Some("data" | "data1" | "type"))),
            (
                "public_hash_type_matches_expected",
                json_pointer_str(public_verifier, "/hash_type") == Some(EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE),
            ),
            (
                "external_artifact_hash_matches_current_tcb",
                normalize_hex(json_pointer_str(external, "/artifact_hash")).as_deref() == artifact_hash,
            ),
            (
                "external_source_tree_hash_matches_current_tcb",
                normalize_hex(json_pointer_str(external, "/source_tree_sha256")).as_deref() == expected_tcb_source_tree_hash,
            ),
            ("public_verifier_id", json_pointer_str(public_verifier, "/verifier_id") == Some("btc.bip340.v0")),
            ("external_verifier_id", json_pointer_str(external, "/verifier_id") == Some("btc.bip340.v0")),
            ("public_ipc_abi", json_pointer_str(public_verifier, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0")),
            ("external_ipc_abi", json_pointer_str(external, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0")),
        ]
        .into_iter()
        .map(|(key, passed)| (key.to_string(), Value::Bool(passed)))
        .collect(),
    );
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "expected_artifact_hash": artifact_hash,
        "expected_artifact_hash_algorithm": artifact_hash_algorithm,
        "expected_source_tree_sha256": expected_tcb_source_tree_hash,
        "checks": checks,
        "templates": {
            "public_shared_cell_dep": rel(repo_root, &public_path),
            "external_bip340_tcb_review": rel(repo_root, &external_path),
            "public_btc_spv_evidence": rel(repo_root, &btc_spv_path),
            "rwa_legal_registry_review_evidence": rel(repo_root, &rwa_legal_path),
        },
    }))
}

fn validate_bip340_tcb_source_inventory(repo_root: &Path, tcb: &Value) -> Result<Value> {
    let current_source = source_tree_hash_with_options(repo_root, BIP340_TCB_SOURCE_HASH_PATHS, true)?;
    let recorded_repo_commit = json_pointer_str(tcb, "/repo_commit");
    let current_repo_commit = git_commit(repo_root);
    let recorded_files = tcb
        .pointer("/source_inventory/files")
        .and_then(Value::as_array)
        .map(|files| files.iter().filter_map(|row| json_pointer_str(row, "/path").map(str::to_string)).collect::<Vec<_>>())
        .unwrap_or_default();
    let current_files = json_array_strings(&current_source, "/files");
    let checks = json!({
        "current_source_tree_valid": json_pointer_bool(&current_source, "/valid"),
        "source_tree_sha256_matches_current": normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256"))
            == normalize_hex(json_pointer_str(&current_source, "/sha256")),
        "source_tree_file_count_matches_current": json_pointer_i64(tcb, "/source_inventory/total_files")
            == json_pointer_i64(&current_source, "/file_count"),
        "source_tree_file_list_matches_current": recorded_files == current_files,
        "source_tree_has_files": json_pointer_i64(&current_source, "/file_count").is_some_and(|count| count > 0),
        "source_tree_invalid_paths_empty": json_array_strings(&current_source, "/invalid_paths").is_empty(),
        "repo_commit_present": recorded_repo_commit.is_some_and(is_git_commit_hash),
        "current_repo_commit_available": current_repo_commit.as_deref().is_some_and(is_git_commit_hash),
        "repo_commit_matches_current_head": recorded_repo_commit == current_repo_commit.as_deref(),
    });
    Ok(json!({
        "schema": "novaseal-bip340-tcb-source-inventory-validation-v0.2",
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "recorded_repo_commit": recorded_repo_commit,
        "current_repo_commit": current_repo_commit,
        "recorded_source_tree_sha256": normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256")),
        "current_source_tree": current_source,
        "recorded_files": recorded_files,
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
    let tcb_source_inventory = validate_bip340_tcb_source_inventory(repo_root, tcb)?;
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
        "tcb_source_inventory_matches_current": json_pointer_str(&tcb_source_inventory, "/status") == Some("passed"),
        "tcb_review_hits_empty": review_hits.is_empty(),
        "unsafe_boundary_documented": riscv_shell_doc.contains("## Unsafe Boundary")
            && riscv_shell_doc.contains("syscall register ABI only"),
        "unsafe_surface_isolated": unsafe_surface_isolated,
        "unsafe_blocks_have_safety_comments": unsafe_block_count > 0 && safety_comment_count >= unsafe_block_count,
        "external_attestation_templates_current": json_pointer_str(attestation_templates, "/status") == Some("passed"),
        "production_blockers_explicit": agreement_security.contains("public/shared CellDep")
            && agreement_security.contains("public BTC SPV")
            && agreement_security.contains("RWA legal/registry review")
            && agreement_security.contains("external BIP340")
            && agreement_audit.contains("external production attestations, public BTC SPV evidence, and RWA legal/registry review evidence still required"),
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
        "tcb_source_inventory": tcb_source_inventory,
        "residual_production_blockers": [
            "public/shared CellDep pinning attestation",
            "public BTC SPV evidence for BTC-facing profiles",
            "RWA legal/registry review evidence",
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
    let canonical_repo_root = repo_root.canonicalize()?;
    let expected = names.iter().map(|name| (*name).to_string()).collect::<BTreeSet<_>>();
    let mut invalid = BTreeSet::new();
    let mut found = BTreeSet::new();
    if safe_directory_within_root(&canonical_repo_root, root)?.is_some() {
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                invalid.insert(name);
            } else if file_type.is_file() && safe_regular_file_within_root(&canonical_repo_root, &path)?.is_some() {
                found.insert(name);
            }
        }
    } else {
        invalid.insert(rel(repo_root, root));
    }
    let mut hashes = Map::new();
    for name in &expected {
        let path = root.join(name);
        if safe_regular_file_within_root(&canonical_repo_root, &path)?.is_some() {
            hashes.insert(name.clone(), Value::String(sha256_file_hex(&path)?));
        }
    }
    Ok(json!({
        "root": rel(repo_root, root),
        "expected": expected.iter().cloned().collect::<Vec<_>>(),
        "present": found.intersection(&expected).cloned().collect::<Vec<_>>(),
        "missing": expected.difference(&found).cloned().collect::<Vec<_>>(),
        "extra": found.difference(&expected).cloned().collect::<Vec<_>>(),
        "invalid": invalid.iter().cloned().collect::<Vec<_>>(),
        "hashes": hashes,
        "exact": found == expected && invalid.is_empty(),
    }))
}

fn symlink_metadata_optional(path: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn safe_regular_file_within_root(canonical_root: &Path, path: &Path) -> Result<Option<PathBuf>> {
    let Some(metadata) = symlink_metadata_optional(path)? else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(None);
    }
    let canonical_path = path.canonicalize()?;
    if canonical_path.starts_with(canonical_root) {
        Ok(Some(canonical_path))
    } else {
        Ok(None)
    }
}

fn safe_directory_within_root(canonical_root: &Path, path: &Path) -> Result<Option<PathBuf>> {
    let Some(metadata) = symlink_metadata_optional(path)? else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(None);
    }
    let canonical_path = path.canonicalize()?;
    if canonical_path.starts_with(canonical_root) {
        Ok(Some(canonical_path))
    } else {
        Ok(None)
    }
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
    let Some(path) = safe_json_report_path(repo_root, path)? else {
        return Ok(json!({"missing": true, "path": rel(repo_root, path)}));
    };
    let bytes = std::fs::read(&path)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| CompileError::without_span(format!("failed to parse JSON '{}': {}", path.display(), error)))
}

fn json_load_path_optional(repo_root: &Path, path: &Path) -> Result<Option<Value>> {
    let Some(path) = safe_json_report_path(repo_root, path)? else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path)?;
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Ok(Some(value)),
        Ok(_) => Ok(Some(json!({"_invalid_json": "top-level value is not an object"}))),
        Err(error) => Ok(Some(json!({"_invalid_json": error.to_string()}))),
    }
}

fn safe_json_report_path(repo_root: &Path, path: &Path) -> Result<Option<PathBuf>> {
    let Some(metadata) = symlink_metadata_optional(path)? else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CompileError::without_span(format!(
            "refusing to read JSON report '{}' because it is not a regular file within repository root '{}'",
            path.display(),
            repo_root.display()
        )));
    }
    let canonical_repo_root = repo_root.canonicalize()?;
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(&canonical_repo_root) {
        return Err(CompileError::without_span(format!(
            "refusing to read JSON report '{}' because it resolves outside repository root '{}'",
            path.display(),
            repo_root.display()
        )));
    }
    Ok(Some(canonical_path))
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
    state.update(canonical_json_for_report_hash(value).as_bytes());
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

fn validate_btc_transaction_binding(profile: &str, case: &Value, expected_binding: &Value) -> Value {
    let binding = case.get("btc_transaction_binding").unwrap_or(&Value::Null);
    let tx = json_pointer_str(case, "/btc_tx_hex").and_then(parse_bitcoin_tx_hex);
    let sealed_tx = json_pointer_str(binding, "/sealed_btc_tx_hex").and_then(parse_bitcoin_tx_hex);
    let txid_matches =
        tx.as_ref().is_some_and(|tx| normalize_hex(json_pointer_str(case, "/btc_txid")).as_deref() == Some(tx.txid.as_str()));
    let wtxid_matches =
        tx.as_ref().is_some_and(|tx| normalize_hex(json_pointer_str(case, "/btc_wtxid")).as_deref() == Some(tx.wtxid.as_str()));

    json!({
        "btc_tx_hex_valid": tx.is_some(),
        "btc_txid_matches_tx_hex": txid_matches,
        "btc_wtxid_matches_tx_hex": wtxid_matches,
        "binding_fields_exact": btc_transaction_binding_fields_exact(profile, binding),
        "binding_kind_matches_profile": btc_transaction_binding_kind_matches_profile(profile, binding),
        "binding_matches_handoff": btc_binding_fields_match_handoff(profile, binding, expected_binding),
        "transaction_output_matches_anchor": profile != EXPECTED_BTC_TX_COMMITMENT_PROFILE
            || btc_transaction_output_matches_anchor(binding, tx.as_ref(), expected_binding),
        "utxo_spend_input_matches_anchor": profile != EXPECTED_BTC_UTXO_SEAL_PROFILE
            || btc_utxo_spend_input_matches_anchor(binding, tx.as_ref(), expected_binding),
        "utxo_sealed_tx_matches_anchor": profile != EXPECTED_BTC_UTXO_SEAL_PROFILE
            || btc_utxo_sealed_tx_matches_anchor(binding, sealed_tx.as_ref(), expected_binding),
        "utxo_sealed_utxo_commitment_matches_tuple": profile != EXPECTED_BTC_UTXO_SEAL_PROFILE
            || btc_sealed_utxo_commitment_matches_tuple(binding, expected_binding),
        "dual_spend_input_matches_anchor": profile != EXPECTED_DUAL_SEAL_PROFILE
            || btc_dual_spend_input_matches_anchor(binding, tx.as_ref(), expected_binding),
        "dual_sealed_tx_matches_anchor": profile != EXPECTED_DUAL_SEAL_PROFILE
            || btc_utxo_sealed_tx_matches_anchor(binding, sealed_tx.as_ref(), expected_binding),
        "dual_sealed_utxo_commitment_matches_tuple": profile != EXPECTED_DUAL_SEAL_PROFILE
            || btc_sealed_utxo_commitment_matches_tuple(binding, expected_binding),
    })
}

fn btc_transaction_binding_fields_exact(profile: &str, binding: &Value) -> bool {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => exact_object_keys(binding, &["kind", "btc_output_index", "btc_amount_sats"]),
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => exact_object_keys(
            binding,
            &[
                "kind",
                "spend_input_index",
                "sealed_btc_txid",
                "sealed_btc_vout_index",
                "sealed_btc_amount_sats",
                "script_pubkey_hash",
                "sealed_btc_tx_hex",
                "sealed_utxo_commitment_hash",
            ],
        ),
        _ => false,
    }
}

fn btc_transaction_binding_kind_matches_profile(profile: &str, binding: &Value) -> bool {
    let expected = match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => "btc_transaction_output",
        EXPECTED_BTC_UTXO_SEAL_PROFILE => "btc_utxo_spend",
        EXPECTED_DUAL_SEAL_PROFILE => "dual_seal_btc_closure",
        _ => return false,
    };
    json_pointer_str(binding, "/kind") == Some(expected)
}

fn btc_binding_fields_match_handoff(profile: &str, binding: &Value, expected_binding: &Value) -> bool {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => {
            json_pointer_u64(binding, "/btc_output_index") == json_pointer_u64(expected_binding, "/btc_output_index")
                && json_pointer_u64(binding, "/btc_amount_sats") == json_pointer_u64(expected_binding, "/btc_amount_sats")
        }
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => {
            btc_sealed_binding_fields_match_handoff(binding, expected_binding)
        }
        _ => false,
    }
}

fn btc_sealed_binding_fields_match_handoff(binding: &Value, expected_binding: &Value) -> bool {
    json_pointer_u64(binding, "/spend_input_index") == json_pointer_u64(expected_binding, "/spend_input_index")
        && normalize_hex(json_pointer_str(binding, "/sealed_btc_txid")).as_deref()
            == normalize_hex(json_pointer_str(expected_binding, "/sealed_btc_txid")).as_deref()
        && json_pointer_u64(binding, "/sealed_btc_vout_index") == json_pointer_u64(expected_binding, "/sealed_btc_vout_index")
        && json_pointer_u64(binding, "/sealed_btc_amount_sats") == json_pointer_u64(expected_binding, "/sealed_btc_amount_sats")
        && normalize_hex(json_pointer_str(binding, "/script_pubkey_hash")).as_deref()
            == normalize_hex(json_pointer_str(expected_binding, "/script_pubkey_hash")).as_deref()
        && normalize_hex(json_pointer_str(binding, "/sealed_utxo_commitment_hash")).as_deref()
            == normalize_hex(json_pointer_str(expected_binding, "/sealed_utxo_commitment_hash")).as_deref()
}

fn btc_transaction_output_matches_anchor(binding: &Value, tx: Option<&BitcoinTxSummary>, expected_binding: &Value) -> bool {
    let Some(tx) = tx else {
        return false;
    };
    let Some(index) = json_pointer_u64(binding, "/btc_output_index") else {
        return false;
    };
    let Some(expected_index) = json_pointer_u64(expected_binding, "/btc_output_index") else {
        return false;
    };
    let Some(expected_amount) = json_pointer_u64(expected_binding, "/btc_amount_sats") else {
        return false;
    };
    index == expected_index && tx.outputs.get(index as usize).is_some_and(|output| output.amount_sats == expected_amount)
}

fn btc_utxo_spend_input_matches_anchor(binding: &Value, tx: Option<&BitcoinTxSummary>, expected_binding: &Value) -> bool {
    let Some(tx) = tx else {
        return false;
    };
    let Some(index) = json_pointer_u64(binding, "/spend_input_index") else {
        return false;
    };
    let Some(expected_index) = json_pointer_u64(expected_binding, "/spend_input_index") else {
        return false;
    };
    let Some(expected_vout) = json_pointer_u64(expected_binding, "/sealed_btc_vout_index") else {
        return false;
    };
    let expected_txid = normalize_hex(json_pointer_str(expected_binding, "/sealed_btc_txid"));
    index == expected_index
        && tx
            .inputs
            .get(index as usize)
            .is_some_and(|input| Some(input.prev_txid.as_str()) == expected_txid.as_deref() && input.prev_vout == expected_vout)
}

fn btc_utxo_sealed_tx_matches_anchor(binding: &Value, sealed_tx: Option<&BitcoinTxSummary>, expected_binding: &Value) -> bool {
    let Some(sealed_tx) = sealed_tx else {
        return false;
    };
    let Some(index) = json_pointer_u64(binding, "/sealed_btc_vout_index") else {
        return false;
    };
    let Some(expected_amount) = json_pointer_u64(expected_binding, "/sealed_btc_amount_sats") else {
        return false;
    };
    let expected_txid = normalize_hex(json_pointer_str(expected_binding, "/sealed_btc_txid"));
    let expected_script_hash = normalize_hex(json_pointer_str(expected_binding, "/script_pubkey_hash"));
    Some(sealed_tx.txid.as_str()) == expected_txid.as_deref()
        && sealed_tx.outputs.get(index as usize).is_some_and(|output| {
            let actual_script_hash = format!("0x{}", hex::encode(crate::ckb_blake2b256(&output.script_pubkey)));
            output.amount_sats == expected_amount && Some(actual_script_hash.as_str()) == expected_script_hash.as_deref()
        })
}

fn btc_dual_spend_input_matches_anchor(binding: &Value, tx: Option<&BitcoinTxSummary>, expected_binding: &Value) -> bool {
    btc_utxo_spend_input_matches_anchor(binding, tx, expected_binding)
}

fn btc_sealed_utxo_commitment_matches_tuple(binding: &Value, expected_binding: &Value) -> bool {
    let Some(txid) = json_pointer_str(binding, "/sealed_btc_txid") else {
        return false;
    };
    let Some(vout) = json_pointer_u64(binding, "/sealed_btc_vout_index") else {
        return false;
    };
    let Some(amount) = json_pointer_u64(binding, "/sealed_btc_amount_sats") else {
        return false;
    };
    let Some(script_hash) = json_pointer_str(binding, "/script_pubkey_hash") else {
        return false;
    };
    let Some(actual) = btc_sealed_utxo_commitment_hash(txid, vout, amount, script_hash) else {
        return false;
    };
    Some(actual.as_str()) == normalize_hex(json_pointer_str(binding, "/sealed_utxo_commitment_hash")).as_deref()
        && Some(actual.as_str()) == normalize_hex(json_pointer_str(expected_binding, "/sealed_utxo_commitment_hash")).as_deref()
}

fn btc_sealed_utxo_commitment_hash(txid: &str, vout: u64, amount: u64, script_hash: &str) -> Option<String> {
    let txid = normalize_hex(Some(txid)).and_then(|value| hex_bytes_exact(&value, 32))?;
    let script_hash = normalize_hex(Some(script_hash)).and_then(|value| hex_bytes_exact(&value, 32))?;
    let vout = u32::try_from(vout).ok()?;
    let mut packed = Vec::with_capacity(76);
    packed.extend_from_slice(&txid);
    packed.extend_from_slice(&vout.to_le_bytes());
    packed.extend_from_slice(&amount.to_le_bytes());
    packed.extend_from_slice(&script_hash);
    cellscript_packed_hash_hex("BtcUtxoCommitmentV0", &packed)
}

fn cellscript_packed_hash_hex(type_name: &str, packed: &[u8]) -> Option<String> {
    let packed_len = u32::try_from(packed.len()).ok()?;
    let mut preimage = Vec::with_capacity(b"CellScriptPackedHashV0\0".len() + type_name.len() + 1 + 4 + packed.len());
    preimage.extend_from_slice(b"CellScriptPackedHashV0\0");
    preimage.extend_from_slice(type_name.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&packed_len.to_le_bytes());
    preimage.extend_from_slice(packed);
    Some(format!("0x{}", hex::encode(crate::ckb_blake2b256(&preimage))))
}

fn parse_bitcoin_tx_hex(value: &str) -> Option<BitcoinTxSummary> {
    parse_bitcoin_tx(&hex_bytes(value)?)
}

fn parse_bitcoin_tx(bytes: &[u8]) -> Option<BitcoinTxSummary> {
    let mut cursor = 0usize;
    let version = read_slice(bytes, &mut cursor, 4)?.to_vec();
    let mut segwit = false;
    if bytes.get(cursor) == Some(&0) {
        match bytes.get(cursor + 1).copied() {
            Some(0x01) => {
                segwit = true;
                cursor += 2;
            }
            Some(flag) if flag != 0 => return None,
            _ => {}
        }
    }
    let input_count_start = cursor;
    let input_count = read_varint(bytes, &mut cursor)?;
    if input_count == 0 {
        return None;
    }
    let input_count_bytes = bytes.get(input_count_start..cursor)?.to_vec();
    let inputs_start = cursor;
    let mut inputs = Vec::new();
    for _ in 0..input_count {
        let prev_hash = read_slice(bytes, &mut cursor, 32)?;
        let prev_txid = bitcoin_display_hash_from_internal(prev_hash);
        let prev_vout = u64::from(read_u32_le(bytes, &mut cursor)?);
        let script_len = read_varint(bytes, &mut cursor)?;
        read_slice(bytes, &mut cursor, usize::try_from(script_len).ok()?)?;
        read_slice(bytes, &mut cursor, 4)?;
        inputs.push(BitcoinTxInput { prev_txid, prev_vout });
    }
    let inputs_bytes = bytes.get(inputs_start..cursor)?.to_vec();
    let output_count_start = cursor;
    let output_count = read_varint(bytes, &mut cursor)?;
    if output_count == 0 {
        return None;
    }
    let output_count_bytes = bytes.get(output_count_start..cursor)?.to_vec();
    let outputs_start = cursor;
    let mut outputs = Vec::new();
    for _ in 0..output_count {
        let amount_sats = read_u64_le(bytes, &mut cursor)?;
        let script_len = read_varint(bytes, &mut cursor)?;
        let script_pubkey = read_slice(bytes, &mut cursor, usize::try_from(script_len).ok()?)?.to_vec();
        outputs.push(BitcoinTxOutput { amount_sats, script_pubkey });
    }
    let outputs_bytes = bytes.get(outputs_start..cursor)?.to_vec();
    if segwit {
        for _ in 0..input_count {
            let item_count = read_varint(bytes, &mut cursor)?;
            for _ in 0..item_count {
                let item_len = read_varint(bytes, &mut cursor)?;
                read_slice(bytes, &mut cursor, usize::try_from(item_len).ok()?)?;
            }
        }
    }
    let lock_time = read_slice(bytes, &mut cursor, 4)?.to_vec();
    if cursor != bytes.len() {
        return None;
    }
    let mut stripped = Vec::new();
    stripped.extend_from_slice(&version);
    stripped.extend_from_slice(&input_count_bytes);
    stripped.extend_from_slice(&inputs_bytes);
    stripped.extend_from_slice(&output_count_bytes);
    stripped.extend_from_slice(&outputs_bytes);
    stripped.extend_from_slice(&lock_time);
    Some(BitcoinTxSummary {
        txid: bitcoin_display_hash(&stripped),
        wtxid: if segwit { bitcoin_display_hash(bytes) } else { bitcoin_display_hash(&stripped) },
        inputs,
        outputs,
    })
}

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = cursor.checked_add(len)?;
    let slice = bytes.get(*cursor..end)?;
    *cursor = end;
    Some(slice)
}

fn read_u32_le(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    Some(u32::from_le_bytes(read_slice(bytes, cursor, 4)?.try_into().ok()?))
}

fn read_u64_le(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    Some(u64::from_le_bytes(read_slice(bytes, cursor, 8)?.try_into().ok()?))
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let first = *read_slice(bytes, cursor, 1)?.first()?;
    match first {
        0x00..=0xfc => Some(u64::from(first)),
        0xfd => {
            let value: [u8; 2] = read_slice(bytes, cursor, 2)?.try_into().ok()?;
            let value = u16::from_le_bytes(value);
            (value >= 0xfd).then_some(u64::from(value))
        }
        0xfe => {
            let value = read_u32_le(bytes, cursor)?;
            (value > u32::from(u16::MAX)).then_some(u64::from(value))
        }
        0xff => {
            let value = read_u64_le(bytes, cursor)?;
            (value > u64::from(u32::MAX)).then_some(value)
        }
    }
}

fn validate_btc_spv_case_proof(case: &Value, confirmations: i64) -> Value {
    let header = json_pointer_str(case, "/btc_block_header").and_then(|value| hex_bytes_exact(value, 80));
    let header_hash = header.as_deref().map(bitcoin_display_hash);
    let header_merkle_root = header.as_deref().map(bitcoin_header_merkle_root_display);
    let proof = case.get("btc_merkle_proof").unwrap_or(&Value::Null);
    let txid = json_pointer_str(case, "/btc_txid");
    let tx_index = json_pointer_i64(proof, "/tx_index");
    let branch = json_array_strings(proof, "/merkle_branch");
    let branch_valid = match tx_index {
        Some(0) if branch.is_empty() => true,
        Some(_) if branch.is_empty() => false,
        Some(_) => branch.iter().all(|hash| is_hex32(hash) && !placeholder_hash(Some(hash))),
        None => false,
    };
    let computed_merkle_root = txid.zip(tx_index).and_then(|(txid, tx_index)| bitcoin_merkle_root_display(txid, &branch, tx_index));
    let block_height = json_pointer_i64(proof, "/block_height");
    let observed_tip_height = json_pointer_i64(proof, "/observed_tip_height");
    let expected_confirmations = block_height.zip(observed_tip_height).and_then(|(block_height, observed_tip_height)| {
        if block_height >= 0 && observed_tip_height >= block_height {
            observed_tip_height.checked_sub(block_height)?.checked_add(1)
        } else {
            None
        }
    });
    let proof_hash = btc_spv_proof_material_hash(case);
    json!({
        "block_hash_matches_header": header_hash.as_deref()
            == normalize_hex(json_pointer_str(case, "/btc_block_hash")).as_deref(),
        "merkle_branch_valid": branch_valid,
        "merkle_root_matches_header": header_merkle_root.as_deref()
            == normalize_hex(json_pointer_str(proof, "/merkle_root")).as_deref(),
        "merkle_branch_verifies_txid": computed_merkle_root.as_deref()
            == normalize_hex(json_pointer_str(proof, "/merkle_root")).as_deref(),
        "confirmations_match_heights": expected_confirmations == Some(confirmations),
        "proof_hash_matches_material": proof_hash.as_deref()
            == normalize_hex(json_pointer_str(case, "/spv_proof_hash")).as_deref(),
    })
}

fn btc_spv_proof_material_hash(case: &Value) -> Option<String> {
    let material = json!({
        "btc_txid": json_pointer_str(case, "/btc_txid")?,
        "btc_wtxid": json_pointer_str(case, "/btc_wtxid")?,
        "btc_tx_hex": json_pointer_str(case, "/btc_tx_hex")?,
        "btc_transaction_binding": case.get("btc_transaction_binding")?,
        "btc_block_hash": json_pointer_str(case, "/btc_block_hash")?,
        "btc_block_header": json_pointer_str(case, "/btc_block_header")?,
        "btc_merkle_proof": case.get("btc_merkle_proof")?,
    });
    Some(format!("0x{}", hex::encode(Sha256::digest(canonical_json_for_report_hash(&material).as_bytes()))))
}

fn bitcoin_merkle_root_display(txid_display: &str, branch_display: &[String], tx_index: i64) -> Option<String> {
    let mut index = u64::try_from(tx_index).ok()?;
    let mut current = bitcoin_internal_hash_from_display(txid_display)?;
    for sibling in branch_display {
        let sibling = bitcoin_internal_hash_from_display(sibling)?;
        let mut preimage = Vec::with_capacity(64);
        if index & 1 == 0 {
            preimage.extend_from_slice(&current);
            preimage.extend_from_slice(&sibling);
        } else {
            preimage.extend_from_slice(&sibling);
            preimage.extend_from_slice(&current);
        }
        current = bitcoin_double_sha256(&preimage);
        index >>= 1;
    }
    Some(bitcoin_display_hash_from_internal(&current))
}

fn bitcoin_header_merkle_root_display(header: &[u8]) -> String {
    bitcoin_display_hash_from_internal(&header[36..68])
}

fn bitcoin_display_hash(header: &[u8]) -> String {
    let digest = bitcoin_double_sha256(header);
    bitcoin_display_hash_from_internal(&digest)
}

fn bitcoin_display_hash_from_internal(hash: &[u8]) -> String {
    let mut display = hash.to_vec();
    display.reverse();
    format!("0x{}", hex::encode(display))
}

fn bitcoin_internal_hash_from_display(value: &str) -> Option<[u8; 32]> {
    let mut bytes = hex_bytes_exact(value, 32)?;
    bytes.reverse();
    bytes.try_into().ok()
}

fn bitcoin_double_sha256(bytes: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(bytes);
    Sha256::digest(first).into()
}

fn novaseal_profile_operator_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaProfileFxV0").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(canonical_json_for_report_hash(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

fn novaseal_service_builder_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaSvcBuildV0").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(canonical_json_for_report_hash(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

fn novaseal_btc_spv_adapter_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaBtcSpvReqV0").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(canonical_json_for_report_hash(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

fn novaseal_external_attestation_report_hash(label: &str, value: &Value) -> String {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(b"NovaExtAttReqV0").to_state();
    state.update(label.as_bytes());
    state.update(b"\x00");
    state.update(canonical_json_for_report_hash(value).as_bytes());
    format!("0x{}", hex::encode(state.finalize().as_bytes()))
}

fn canonical_json_for_report_hash(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string()),
        Value::Array(values) => {
            let items = values.iter().map(canonical_json_for_report_hash).collect::<Vec<_>>();
            format!("[{}]", items.join(","))
        }
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(key, _)| *key);
            let items = entries
                .into_iter()
                .map(|(key, value)| {
                    let key = serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string());
                    format!("{}:{}", key, canonical_json_for_report_hash(value))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", items.join(","))
        }
    }
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

fn is_external_identity(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed == value
        && trimmed.len() >= 3
        && !contains_placeholder_token(trimmed)
        && !contains_local_only_token(trimmed)
        && !contains_first_party_attestation_token(trimmed)
}

fn is_public_network(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed != value || trimmed.is_empty() || contains_placeholder_token(trimmed) {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower == "testnet-or-mainnet"
        || ["local", "devnet", "regtest", "simnet", "private", "fake"].iter().any(|token| lower.contains(token))
    {
        return false;
    }
    lower == "mainnet"
        || lower == "testnet"
        || lower.ends_with("-mainnet")
        || lower.ends_with("-testnet")
        || lower.ends_with(" mainnet")
        || lower.ends_with(" testnet")
}

fn contains_placeholder_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "replace_with",
        "replace-",
        "placeholder",
        "todo",
        "tbd",
        "unknown",
        "example",
        "sample",
        "dummy",
        "not_applicable",
        "not-applicable",
        "n/a",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn contains_local_only_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["local", "devnet", "regtest", "simnet", "fake", "internal", "mock"].iter().any(|token| lower.contains(token))
}

fn contains_first_party_attestation_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "novaseal",
        "nova seal",
        "cellscript",
        "cell script",
        "a19q3",
        "first-party",
        "first_party",
        "self-attested",
        "self_attested",
        "self attested",
        "self-attestation",
        "self_attestation",
        "self attestation",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn is_https_report_uri(value: &str) -> bool {
    if value != value.trim() || contains_placeholder_token(value) || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return false;
    }
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let Some(host) = report_uri_host(authority) else {
        return false;
    };
    is_public_report_host(host)
}

fn report_uri_host(authority: &str) -> Option<&str> {
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, suffix) = rest.split_once(']')?;
        if !valid_optional_port_suffix(suffix) {
            return None;
        }
        return Some(host);
    }
    if authority.matches(':').count() > 1 {
        return None;
    }
    match authority.split_once(':') {
        Some((host, port)) if valid_port(port) && !host.is_empty() => Some(host),
        Some(_) => None,
        None => Some(authority),
    }
}

fn valid_optional_port_suffix(suffix: &str) -> bool {
    if suffix.is_empty() {
        return true;
    }
    suffix.strip_prefix(':').is_some_and(valid_port)
}

fn valid_port(port: &str) -> bool {
    port.parse::<u16>().is_ok_and(|value| value != 0)
}

fn is_public_report_host(host: &str) -> bool {
    if host != host.trim() || host.is_empty() || contains_placeholder_token(host) {
        return false;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_public_report_ip(ip);
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".invalid")
        || lower.ends_with(".local")
        || lower.ends_with(".test")
        || lower.starts_with('.')
        || lower.ends_with('.')
        || !lower.contains('.')
    {
        return false;
    }
    let mut has_alpha = false;
    for label in lower.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return false;
        }
        has_alpha |= label.bytes().any(|byte| byte.is_ascii_alphabetic());
    }
    has_alpha
}

fn is_public_report_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(value) => is_public_report_ipv4(value),
        IpAddr::V6(value) => is_public_report_ipv6(value),
    }
}

fn is_public_report_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _d] = ip.octets();
    if a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (18..=19).contains(&b))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
    {
        return false;
    }
    true
}

fn is_public_report_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
    {
        return false;
    }
    true
}

fn is_utc_timestamp_z(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && ascii_digits(&bytes[0..4])
        && ascii_digits(&bytes[5..7])
        && ascii_digits(&bytes[8..10])
        && ascii_digits(&bytes[11..13])
        && ascii_digits(&bytes[14..16])
        && ascii_digits(&bytes[17..19])
        && valid_ymd_time(
            parse_digits(&bytes[0..4]),
            parse_digits(&bytes[5..7]),
            parse_digits(&bytes[8..10]),
            parse_digits(&bytes[11..13]),
            parse_digits(&bytes[14..16]),
            parse_digits(&bytes[17..19]),
        )
}

fn is_utc_timestamp_z_not_future(value: &str) -> bool {
    let Some(timestamp) = utc_timestamp_seconds(value) else {
        return false;
    };
    let Some(now) = current_unix_seconds() else {
        return false;
    };
    timestamp <= now
}

fn is_utc_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && ascii_digits(&bytes[0..4])
        && ascii_digits(&bytes[5..7])
        && ascii_digits(&bytes[8..10])
        && valid_ymd(parse_digits(&bytes[0..4]), parse_digits(&bytes[5..7]), parse_digits(&bytes[8..10]))
}

fn is_utc_date_not_future(value: &str) -> bool {
    let Some(days) = utc_date_days(value) else {
        return false;
    };
    let Some(now) = current_unix_seconds() else {
        return false;
    };
    days <= (now / 86_400) as i64
}

fn ascii_digits(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_digit)
}

fn parse_digits(bytes: &[u8]) -> Option<u32> {
    if ascii_digits(bytes) {
        Some(bytes.iter().fold(0, |acc, byte| (acc * 10) + u32::from(byte - b'0')))
    } else {
        None
    }
}

fn valid_ymd_time(
    year: Option<u32>,
    month: Option<u32>,
    day: Option<u32>,
    hour: Option<u32>,
    minute: Option<u32>,
    second: Option<u32>,
) -> bool {
    valid_ymd(year, month, day)
        && hour.is_some_and(|value| value < 24)
        && minute.is_some_and(|value| value < 60)
        && second.is_some_and(|value| value < 60)
}

fn valid_ymd(year: Option<u32>, month: Option<u32>, day: Option<u32>) -> bool {
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let Some(max_day) = days_in_month(year, month) else {
        return false;
    };
    year > 0 && (1..=max_day).contains(&day)
}

fn utc_timestamp_seconds(value: &str) -> Option<u64> {
    if !is_utc_timestamp_z(value) {
        return None;
    }
    let bytes = value.as_bytes();
    let days = utc_date_components_days(parse_digits(&bytes[0..4])?, parse_digits(&bytes[5..7])?, parse_digits(&bytes[8..10])?)?;
    let seconds = days.checked_mul(86_400)?.checked_add(
        parse_digits(&bytes[11..13])? as i64 * 3_600
            + parse_digits(&bytes[14..16])? as i64 * 60
            + parse_digits(&bytes[17..19])? as i64,
    )?;
    u64::try_from(seconds).ok()
}

fn utc_date_days(value: &str) -> Option<i64> {
    if !is_utc_date(value) {
        return None;
    }
    let bytes = value.as_bytes();
    utc_date_components_days(parse_digits(&bytes[0..4])?, parse_digits(&bytes[5..7])?, parse_digits(&bytes[8..10])?)
}

fn utc_date_components_days(year: u32, month: u32, day: u32) -> Option<i64> {
    if !valid_ymd(Some(year), Some(month), Some(day)) {
        return None;
    }
    let mut year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_for_year = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_for_year + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

fn current_unix_seconds() -> Option<u64> {
    SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs())
}

fn days_in_month(year: u32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

fn leap_year(year: u32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
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

fn expected_btc_spv_scenario(profile: &str) -> Option<&'static str> {
    EXPECTED_BTC_SPV_PROFILE_SCENARIOS
        .iter()
        .find_map(|(expected_profile, scenario)| (*expected_profile == profile).then_some(*scenario))
}

fn expected_public_btc_anchor_pointer(profile: &str) -> Option<&'static str> {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => Some("/commit_transaction/public_btc_anchor"),
        EXPECTED_BTC_UTXO_SEAL_PROFILE => Some("/close_utxo_seal/public_btc_anchor"),
        EXPECTED_DUAL_SEAL_PROFILE => Some("/finalize_dual_seal/public_btc_anchor"),
        _ => None,
    }
}

fn expected_public_btc_commitment_hash_pointer(profile: &str) -> Option<&'static str> {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => Some("/commit_transaction/btc_tx_commitment_hash"),
        EXPECTED_BTC_UTXO_SEAL_PROFILE => Some("/close_utxo_seal/closure_commitment_hash"),
        EXPECTED_DUAL_SEAL_PROFILE => Some("/finalize_dual_seal/btc_closure_commitment_hash"),
        _ => None,
    }
}

fn public_btc_anchor_shape_matches_profile(profile: &str, anchor: Option<&Value>) -> bool {
    let Some(anchor) = anchor else {
        return false;
    };
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => {
            exact_object_keys(
                anchor,
                &["kind", "anchor_source", "btc_txid", "btc_wtxid", "btc_output_index", "btc_amount_sats", "ckb_btc_commitment_hash"],
            ) && json_pointer_str(anchor, "/kind") == Some("btc_transaction_commitment")
                && json_pointer_str(anchor, "/anchor_source").is_some_and(|source| btc_anchor_source_matches_profile(profile, source))
                && json_pointer_str(anchor, "/btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_wtxid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/btc_output_index").is_some()
                && json_pointer_u64(anchor, "/btc_amount_sats").is_some_and(|amount| amount > 0)
                && json_pointer_str(anchor, "/ckb_btc_commitment_hash").is_some_and(is_real_tx_hash)
        }
        EXPECTED_BTC_UTXO_SEAL_PROFILE => {
            exact_object_keys(
                anchor,
                &[
                    "kind",
                    "anchor_source",
                    "sealed_btc_txid",
                    "sealed_btc_vout_index",
                    "sealed_btc_amount_sats",
                    "script_pubkey_hash",
                    "btc_txid",
                    "btc_wtxid",
                    "spend_input_index",
                    "ckb_btc_commitment_hash",
                    "sealed_utxo_commitment_hash",
                ],
            ) && json_pointer_str(anchor, "/kind") == Some("btc_utxo_spend")
                && json_pointer_str(anchor, "/anchor_source").is_some_and(|source| btc_anchor_source_matches_profile(profile, source))
                && json_pointer_str(anchor, "/sealed_btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/sealed_btc_vout_index").is_some()
                && json_pointer_u64(anchor, "/sealed_btc_amount_sats").is_some_and(|amount| amount > 0)
                && json_pointer_str(anchor, "/script_pubkey_hash").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_wtxid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/spend_input_index").is_some()
                && json_pointer_str(anchor, "/ckb_btc_commitment_hash").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/sealed_utxo_commitment_hash").is_some_and(is_real_tx_hash)
        }
        EXPECTED_DUAL_SEAL_PROFILE => {
            exact_object_keys(
                anchor,
                &[
                    "kind",
                    "anchor_source",
                    "sealed_btc_txid",
                    "sealed_btc_vout_index",
                    "sealed_btc_amount_sats",
                    "script_pubkey_hash",
                    "btc_txid",
                    "btc_wtxid",
                    "spend_input_index",
                    "ckb_btc_commitment_hash",
                    "sealed_utxo_commitment_hash",
                ],
            ) && json_pointer_str(anchor, "/kind") == Some("dual_seal_btc_closure")
                && json_pointer_str(anchor, "/anchor_source").is_some_and(|source| btc_anchor_source_matches_profile(profile, source))
                && json_pointer_str(anchor, "/sealed_btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/sealed_btc_vout_index").is_some()
                && json_pointer_u64(anchor, "/sealed_btc_amount_sats").is_some_and(|amount| amount > 0)
                && json_pointer_str(anchor, "/script_pubkey_hash").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_wtxid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/spend_input_index").is_some()
                && json_pointer_str(anchor, "/ckb_btc_commitment_hash").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/sealed_utxo_commitment_hash").is_some_and(is_real_tx_hash)
        }
        _ => false,
    }
}

fn btc_anchor_source_matches_profile(profile: &str, source: &str) -> bool {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => matches!(source, "local_deterministic_fixture" | "external_public_btc_transaction"),
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => {
            matches!(source, "local_deterministic_fixture" | "external_public_btc_spend")
        }
        _ => false,
    }
}

fn btc_anchor_source_production_eligible(profile: &str, source: &str) -> bool {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => source == "external_public_btc_transaction",
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => source == "external_public_btc_spend",
        _ => false,
    }
}

fn handoff_expected_bindings_exact(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.len() != EXPECTED_BTC_SPV_EVIDENCE_PROFILES.len() {
        return false;
    }
    let expected_hash_fields = [
        "ckb_live_tx_hash",
        "live_report_hash",
        "service_builder_case_hash",
        "service_builder_tx_skeleton_hash",
        "service_builder_receipt_binding_hash",
        "ckb_btc_commitment_hash",
    ];
    EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().all(|profile| {
        object.get(*profile).is_some_and(|binding| {
            let profile_fields: &[&str] = match *profile {
                EXPECTED_BTC_TX_COMMITMENT_PROFILE => {
                    &["anchor_source", "btc_txid", "btc_wtxid", "btc_output_index", "btc_amount_sats"]
                }
                EXPECTED_BTC_UTXO_SEAL_PROFILE => &[
                    "anchor_source",
                    "btc_txid",
                    "btc_wtxid",
                    "spend_input_index",
                    "sealed_btc_txid",
                    "sealed_btc_vout_index",
                    "sealed_btc_amount_sats",
                    "script_pubkey_hash",
                    "sealed_utxo_commitment_hash",
                ],
                EXPECTED_DUAL_SEAL_PROFILE => &[
                    "anchor_source",
                    "btc_txid",
                    "btc_wtxid",
                    "spend_input_index",
                    "sealed_btc_txid",
                    "sealed_btc_vout_index",
                    "sealed_btc_amount_sats",
                    "script_pubkey_hash",
                    "sealed_utxo_commitment_hash",
                ],
                _ => return false,
            };
            let allowed_fields = expected_hash_fields.iter().chain(profile_fields.iter()).copied().collect::<Vec<_>>();
            exact_object_keys(binding, &allowed_fields)
                && expected_hash_fields.iter().all(|field| {
                    normalize_hex(json_pointer_str(binding, &format!("/{field}"))).as_deref().is_some_and(is_hex32)
                        && !placeholder_hash(normalize_hex(json_pointer_str(binding, &format!("/{field}"))).as_deref())
                })
                && profile_fields.iter().all(|field| btc_binding_expected_field_valid(profile, binding, field))
        })
    })
}

fn btc_binding_expected_field_valid(profile: &str, binding: &Value, field: &str) -> bool {
    match field {
        "anchor_source" => {
            json_pointer_str(binding, "/anchor_source").is_some_and(|source| btc_anchor_source_production_eligible(profile, source))
        }
        "btc_txid" | "btc_wtxid" | "sealed_btc_txid" | "script_pubkey_hash" | "sealed_utxo_commitment_hash" => {
            normalize_hex(json_pointer_str(binding, &format!("/{field}"))).as_deref().is_some_and(is_hex32)
                && !placeholder_hash(normalize_hex(json_pointer_str(binding, &format!("/{field}"))).as_deref())
        }
        "btc_output_index" | "spend_input_index" | "sealed_btc_vout_index" => {
            json_pointer_u64(binding, &format!("/{field}")).is_some()
        }
        "btc_amount_sats" | "sealed_btc_amount_sats" => {
            json_pointer_u64(binding, &format!("/{field}")).is_some_and(|amount| amount > 0)
        }
        _ => false,
    }
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

fn fiber_repo_git_provenance(fiber_repo_path: Option<&Path>, report: &Value) -> Value {
    let Some(path) = fiber_repo_path else {
        return json!({
            "verified": false,
            "reason": "missing fiber repo path",
            "checks": {
                "git_commands_succeeded": false,
                "origin_matches_report": false,
                "branch_matches_report": false,
                "commit_matches_report": false,
                "dirty_matches_report": false,
                "origin_matches_expected": false,
                "commit_is_full_sha1": false,
                "clean_tree": false,
            }
        });
    };
    let origin = git_stdout(path, &["remote", "get-url", "origin"]);
    let branch = git_stdout(path, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let commit = git_stdout(path, &["rev-parse", "HEAD"]);
    let status = git_stdout(path, &["status", "--porcelain"]);
    let dirty = status.as_ref().map(|value| !value.trim().is_empty());
    let checks = json!({
        "git_commands_succeeded": origin.is_some() && branch.is_some() && commit.is_some() && dirty.is_some(),
        "origin_matches_report": origin.as_deref() == json_pointer_str(report, "/fiber_repo/origin"),
        "branch_matches_report": branch.as_deref() == json_pointer_str(report, "/fiber_repo/branch"),
        "commit_matches_report": commit.as_deref() == json_pointer_str(report, "/fiber_repo/commit"),
        "dirty_matches_report": dirty == json_pointer_bool_opt(report, "/fiber_repo/dirty"),
        "origin_matches_expected": origin.as_deref() == Some(EXPECTED_FIBER_REPO_ORIGIN),
        "commit_is_full_sha1": commit.as_deref().is_some_and(is_git_commit_hash),
        "clean_tree": dirty == Some(false),
    });
    json!({
        "verified": object_values_all_true(Some(&checks)),
        "path": path.display().to_string(),
        "origin": origin,
        "branch": branch,
        "commit": commit,
        "dirty": dirty,
        "checks": checks,
    })
}

fn git_stdout(repo: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["-c", "protocol.ext.allow=never", "-c", "protocol.file.allow=never", "-c", "core.fsmonitor=false"])
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn relative_file_exists(root: &Path, rel_path: Option<&str>, require_nonempty: bool) -> bool {
    let Some(rel_path) = rel_path else {
        return false;
    };
    if !safe_relative_path(rel_path) {
        return false;
    }
    let path = root.join(rel_path);
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(link_metadata) = std::fs::symlink_metadata(&path) else {
        return false;
    };
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return false;
    }
    let Ok(canonical_path) = path.canonicalize() else {
        return false;
    };
    if !canonical_path.starts_with(root) {
        return false;
    }
    !require_nonempty || link_metadata.len() > 0
}

fn relative_file_array_all_exist(root: &Path, value: Option<&Value>, require_nonempty: bool) -> bool {
    value
        .and_then(Value::as_array)
        .is_some_and(|paths| !paths.is_empty() && paths.iter().all(|path| relative_file_exists(root, path.as_str(), require_nonempty)))
}

fn bruno_compatibility_patch_contract(repo_root: &Path, bruno_cwd: Option<&str>, patches: Option<&Value>) -> bool {
    match (bruno_cwd, patches) {
        (None, None) => true,
        (Some(bruno_cwd), Some(patches)) if safe_relative_path(bruno_cwd) => {
            let Some(patches) = patches.as_array() else {
                return false;
            };
            if patches.is_empty() {
                return false;
            }
            let bruno_root = repo_root.join(bruno_cwd);
            let Ok(repo_root) = repo_root.canonicalize() else {
                return false;
            };
            let Ok(metadata) = std::fs::symlink_metadata(&bruno_root) else {
                return false;
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return false;
            }
            let Ok(bruno_root) = bruno_root.canonicalize() else {
                return false;
            };
            bruno_root.starts_with(repo_root) && patches.iter().all(|path| relative_file_exists(&bruno_root, path.as_str(), true))
        }
        _ => false,
    }
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
    value.len() > 2 && value.len() % 2 == 0 && value.starts_with("0x") && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
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
mod tests {
    use super::*;

    fn constraint_object(expected: &[(&str, &str)]) -> Value {
        Value::Object(expected.iter().map(|(key, value)| ((*key).to_string(), Value::String((*value).to_string()))).collect())
    }

    fn test_hex32(byte: u8) -> String {
        format!("0x{}", format!("{byte:02x}").repeat(32))
    }

    struct TestBtcProfileMaterial {
        tx_hex: String,
        txid: String,
        wtxid: String,
        binding: Value,
        expected_binding: Value,
    }

    fn push_test_compact_size(target: &mut Vec<u8>, value: u64) {
        match value {
            0x00..=0xfc => target.push(value as u8),
            0xfd..=0xffff => {
                target.push(0xfd);
                target.extend_from_slice(&(value as u16).to_le_bytes());
            }
            0x1_0000..=0xffff_ffff => {
                target.push(0xfe);
                target.extend_from_slice(&(value as u32).to_le_bytes());
            }
            _ => {
                target.push(0xff);
                target.extend_from_slice(&value.to_le_bytes());
            }
        }
    }

    fn test_bitcoin_tx_hex(prevouts: &[(String, u32)], outputs: &[(u64, Vec<u8>)]) -> String {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2u32.to_le_bytes());
        push_test_compact_size(&mut bytes, prevouts.len() as u64);
        for (prev_txid, prev_vout) in prevouts {
            bytes.extend_from_slice(&bitcoin_internal_hash_from_display(prev_txid).unwrap());
            bytes.extend_from_slice(&prev_vout.to_le_bytes());
            push_test_compact_size(&mut bytes, 0);
            bytes.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
        }
        push_test_compact_size(&mut bytes, outputs.len() as u64);
        for (amount_sats, script_pubkey) in outputs {
            bytes.extend_from_slice(&amount_sats.to_le_bytes());
            push_test_compact_size(&mut bytes, script_pubkey.len() as u64);
            bytes.extend_from_slice(script_pubkey);
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        format!("0x{}", hex::encode(bytes))
    }

    fn test_bitcoin_tx_bytes(prevouts: &[(String, u32)], outputs: &[(u64, Vec<u8>)]) -> Vec<u8> {
        let hex = test_bitcoin_tx_hex(prevouts, outputs);
        hex_bytes(&hex).expect("test transaction hex should decode")
    }

    fn test_btc_script(seed: u8) -> Vec<u8> {
        vec![0x51, seed, seed.wrapping_add(1)]
    }

    fn test_anchor_source(profile: &str) -> &'static str {
        match profile {
            EXPECTED_BTC_TX_COMMITMENT_PROFILE => "external_public_btc_transaction",
            EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => "external_public_btc_spend",
            _ => "external_public_btc_transaction",
        }
    }

    fn test_btc_profile_material(profile: &str, seed: u8) -> TestBtcProfileMaterial {
        match profile {
            EXPECTED_BTC_UTXO_SEAL_PROFILE => {
                let sealed_script = test_btc_script(seed);
                let sealed_amount = 75_000 + u64::from(seed);
                let script_pubkey_hash = format!("0x{}", hex::encode(crate::ckb_blake2b256(&sealed_script)));
                let sealed_tx_hex = test_bitcoin_tx_hex(
                    &[(test_hex32(seed.wrapping_add(0x30)), 0)],
                    &[(10_000, vec![0x51]), (sealed_amount, sealed_script)],
                );
                let sealed_tx = parse_bitcoin_tx_hex(&sealed_tx_hex).unwrap();
                let tx_hex = test_bitcoin_tx_hex(&[(sealed_tx.txid.clone(), 1)], &[(20_000 + u64::from(seed), vec![0x51, seed])]);
                let tx = parse_bitcoin_tx_hex(&tx_hex).unwrap();
                let sealed_utxo_commitment_hash =
                    btc_sealed_utxo_commitment_hash(&sealed_tx.txid, 1, sealed_amount, &script_pubkey_hash).unwrap();
                let binding = json!({
                    "kind": "btc_utxo_spend",
                    "spend_input_index": 0,
                    "sealed_btc_txid": sealed_tx.txid,
                    "sealed_btc_vout_index": 1,
                    "sealed_btc_amount_sats": sealed_amount,
                    "script_pubkey_hash": script_pubkey_hash,
                    "sealed_btc_tx_hex": sealed_tx_hex,
                    "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
                });
                let expected_binding = json!({
                    "anchor_source": test_anchor_source(profile),
                    "btc_txid": tx.txid,
                    "btc_wtxid": tx.wtxid,
                    "spend_input_index": 0,
                    "sealed_btc_txid": binding["sealed_btc_txid"].clone(),
                    "sealed_btc_vout_index": 1,
                    "sealed_btc_amount_sats": sealed_amount,
                    "script_pubkey_hash": binding["script_pubkey_hash"].clone(),
                    "sealed_utxo_commitment_hash": binding["sealed_utxo_commitment_hash"].clone(),
                });
                TestBtcProfileMaterial {
                    tx_hex,
                    txid: json_pointer_str(&expected_binding, "/btc_txid").unwrap().to_string(),
                    wtxid: json_pointer_str(&expected_binding, "/btc_wtxid").unwrap().to_string(),
                    binding,
                    expected_binding,
                }
            }
            EXPECTED_DUAL_SEAL_PROFILE => {
                let sealed_script = test_btc_script(seed.wrapping_add(0x10));
                let sealed_amount = 85_000 + u64::from(seed);
                let script_pubkey_hash = format!("0x{}", hex::encode(crate::ckb_blake2b256(&sealed_script)));
                let sealed_tx_hex = test_bitcoin_tx_hex(
                    &[(test_hex32(seed.wrapping_add(0x40)), 0)],
                    &[(10_000, vec![0x51]), (sealed_amount, sealed_script)],
                );
                let sealed_tx = parse_bitcoin_tx_hex(&sealed_tx_hex).unwrap();
                let tx_hex = test_bitcoin_tx_hex(&[(sealed_tx.txid.clone(), 1)], &[(30_000 + u64::from(seed), vec![0x51, seed])]);
                let tx = parse_bitcoin_tx_hex(&tx_hex).unwrap();
                let sealed_utxo_commitment_hash =
                    btc_sealed_utxo_commitment_hash(&sealed_tx.txid, 1, sealed_amount, &script_pubkey_hash).unwrap();
                let binding = json!({
                    "kind": "dual_seal_btc_closure",
                    "spend_input_index": 0,
                    "sealed_btc_txid": sealed_tx.txid,
                    "sealed_btc_vout_index": 1,
                    "sealed_btc_amount_sats": sealed_amount,
                    "script_pubkey_hash": script_pubkey_hash,
                    "sealed_btc_tx_hex": sealed_tx_hex,
                    "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
                });
                let expected_binding = json!({
                    "anchor_source": test_anchor_source(profile),
                    "btc_txid": tx.txid,
                    "btc_wtxid": tx.wtxid,
                    "spend_input_index": 0,
                    "sealed_btc_txid": binding["sealed_btc_txid"].clone(),
                    "sealed_btc_vout_index": 1,
                    "sealed_btc_amount_sats": sealed_amount,
                    "script_pubkey_hash": binding["script_pubkey_hash"].clone(),
                    "sealed_utxo_commitment_hash": binding["sealed_utxo_commitment_hash"].clone(),
                });
                TestBtcProfileMaterial {
                    tx_hex,
                    txid: json_pointer_str(&expected_binding, "/btc_txid").unwrap().to_string(),
                    wtxid: json_pointer_str(&expected_binding, "/btc_wtxid").unwrap().to_string(),
                    binding,
                    expected_binding,
                }
            }
            _ => {
                let amount = 50_000 + u64::from(seed);
                let tx_hex = test_bitcoin_tx_hex(
                    &[(test_hex32(seed.wrapping_add(0x20)), 0)],
                    &[(10_000, vec![0x51]), (20_000, vec![0x51, seed]), (amount, test_btc_script(seed))],
                );
                let tx = parse_bitcoin_tx_hex(&tx_hex).unwrap();
                let binding = json!({
                    "kind": "btc_transaction_output",
                    "btc_output_index": 2,
                    "btc_amount_sats": amount,
                });
                let expected_binding = json!({
                    "anchor_source": test_anchor_source(profile),
                    "btc_txid": tx.txid,
                    "btc_wtxid": tx.wtxid,
                    "btc_output_index": 2,
                    "btc_amount_sats": amount,
                });
                TestBtcProfileMaterial {
                    tx_hex,
                    txid: json_pointer_str(&expected_binding, "/btc_txid").unwrap().to_string(),
                    wtxid: json_pointer_str(&expected_binding, "/btc_wtxid").unwrap().to_string(),
                    binding,
                    expected_binding,
                }
            }
        }
    }

    fn merge_expected_btc_binding_fields(target: &mut Value, material: &TestBtcProfileMaterial) {
        for (key, value) in material.expected_binding.as_object().unwrap() {
            target.as_object_mut().unwrap().insert(key.clone(), value.clone());
        }
    }

    fn add_expected_btc_request_fields(request: &mut Value, material: &TestBtcProfileMaterial) {
        for (binding_field, request_field) in [
            ("anchor_source", "local_anchor_source"),
            ("anchor_source", "expected_anchor_source"),
            ("btc_txid", "expected_btc_txid"),
            ("btc_wtxid", "expected_btc_wtxid"),
            ("btc_output_index", "expected_btc_output_index"),
            ("btc_amount_sats", "expected_btc_amount_sats"),
            ("spend_input_index", "expected_spend_input_index"),
            ("sealed_btc_txid", "expected_sealed_btc_txid"),
            ("sealed_btc_vout_index", "expected_sealed_btc_vout_index"),
            ("sealed_btc_amount_sats", "expected_sealed_btc_amount_sats"),
            ("script_pubkey_hash", "expected_script_pubkey_hash"),
            ("sealed_utxo_commitment_hash", "expected_sealed_utxo_commitment_hash"),
        ] {
            if let Some(value) = material.expected_binding.get(binding_field) {
                request.as_object_mut().unwrap().insert(request_field.to_string(), value.clone());
            }
        }
    }

    fn test_btc_spv_material(seed: u8, confirmations: i64, btc: &TestBtcProfileMaterial) -> Value {
        let txid = btc.txid.clone();
        let sibling = test_hex32(seed.wrapping_add(1));
        let merkle_root = bitcoin_merkle_root_display(&txid, std::slice::from_ref(&sibling), 0).unwrap();
        let mut header = vec![0u8; 80];
        header[0..4].copy_from_slice(&2u32.to_le_bytes());
        let merkle_root_internal = bitcoin_internal_hash_from_display(&merkle_root).unwrap();
        header[36..68].copy_from_slice(&merkle_root_internal);
        header[68..72].copy_from_slice(&1_800_000_000u32.to_le_bytes());
        header[72..76].copy_from_slice(&0x1d00ffffu32.to_le_bytes());
        header[76..80].copy_from_slice(&u32::from(seed).to_le_bytes());
        let btc_block_header = format!("0x{}", hex::encode(&header));
        let btc_block_hash = bitcoin_display_hash(&header);
        let block_height = 900_000i64 + i64::from(seed);
        let observed_tip_height = block_height + confirmations - 1;
        let mut material = json!({
            "btc_txid": txid,
            "btc_wtxid": btc.wtxid.clone(),
            "btc_tx_hex": btc.tx_hex.clone(),
            "btc_transaction_binding": btc.binding.clone(),
            "btc_block_hash": btc_block_hash,
            "btc_block_header": btc_block_header,
            "btc_merkle_proof": {
                "tx_index": 0,
                "merkle_branch": [sibling],
                "merkle_root": merkle_root,
                "block_height": block_height,
                "observed_tip_height": observed_tip_height,
            },
        });
        let proof_hash = btc_spv_proof_material_hash(&material).unwrap();
        material["spv_proof_hash"] = json!(proof_hash);
        material
    }

    fn test_btc_single_tx_block_spv_material(seed: u8, confirmations: i64, btc: &TestBtcProfileMaterial) -> Value {
        let txid = btc.txid.clone();
        let mut header = vec![0u8; 80];
        header[0..4].copy_from_slice(&2u32.to_le_bytes());
        let merkle_root_internal = bitcoin_internal_hash_from_display(&txid).unwrap();
        header[36..68].copy_from_slice(&merkle_root_internal);
        header[68..72].copy_from_slice(&1_800_000_000u32.to_le_bytes());
        header[72..76].copy_from_slice(&0x1d00ffffu32.to_le_bytes());
        header[76..80].copy_from_slice(&u32::from(seed).to_le_bytes());
        let btc_block_header = format!("0x{}", hex::encode(&header));
        let btc_block_hash = bitcoin_display_hash(&header);
        let block_height = 910_000i64 + i64::from(seed);
        let observed_tip_height = block_height + confirmations - 1;
        let mut material = json!({
            "btc_txid": txid,
            "btc_wtxid": btc.wtxid.clone(),
            "btc_tx_hex": btc.tx_hex.clone(),
            "btc_transaction_binding": btc.binding.clone(),
            "btc_block_hash": btc_block_hash,
            "btc_block_header": btc_block_header,
            "btc_merkle_proof": {
                "tx_index": 0,
                "merkle_branch": [],
                "merkle_root": txid,
                "block_height": block_height,
                "observed_tip_height": observed_tip_height,
            },
        });
        let proof_hash = btc_spv_proof_material_hash(&material).unwrap();
        material["spv_proof_hash"] = json!(proof_hash);
        material
    }

    fn set_json_pointer_string(value: &mut Value, pointer: &str, content: String) {
        let mut current = value;
        let mut parts = pointer.trim_start_matches('/').split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                current.as_object_mut().unwrap().insert(part.to_string(), Value::String(content));
                return;
            }
            current = current.as_object_mut().unwrap().entry(part.to_string()).or_insert_with(|| json!({}));
        }
    }

    fn set_json_pointer_value(value: &mut Value, pointer: &str, content: Value) {
        let mut current = value;
        let mut parts = pointer.trim_start_matches('/').split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                current.as_object_mut().unwrap().insert(part.to_string(), content);
                return;
            }
            current = current.as_object_mut().unwrap().entry(part.to_string()).or_insert_with(|| json!({}));
        }
    }

    fn test_public_btc_anchor(profile: &str, seed: u8) -> Value {
        match profile {
            EXPECTED_BTC_TX_COMMITMENT_PROFILE => json!({
                "kind": "btc_transaction_commitment",
                "anchor_source": test_anchor_source(profile),
                "btc_txid": test_hex32(seed),
                "btc_wtxid": test_hex32(seed.wrapping_add(1)),
                "btc_output_index": 0,
                "btc_amount_sats": 50_000,
                "ckb_btc_commitment_hash": test_hex32(seed.wrapping_add(2)),
            }),
            EXPECTED_BTC_UTXO_SEAL_PROFILE => json!({
                "kind": "btc_utxo_spend",
                "anchor_source": test_anchor_source(profile),
                "sealed_btc_txid": test_hex32(seed),
                "sealed_btc_vout_index": 1,
                "sealed_btc_amount_sats": 75_000,
                "script_pubkey_hash": test_hex32(seed.wrapping_add(1)),
                "btc_txid": test_hex32(seed.wrapping_add(2)),
                "btc_wtxid": test_hex32(seed.wrapping_add(3)),
                "spend_input_index": 0,
                "ckb_btc_commitment_hash": test_hex32(seed.wrapping_add(4)),
                "sealed_utxo_commitment_hash": test_hex32(seed.wrapping_add(5)),
            }),
            EXPECTED_DUAL_SEAL_PROFILE => json!({
                "kind": "dual_seal_btc_closure",
                "anchor_source": test_anchor_source(profile),
                "sealed_btc_txid": test_hex32(seed),
                "sealed_btc_vout_index": 1,
                "sealed_btc_amount_sats": 85_000,
                "script_pubkey_hash": test_hex32(seed.wrapping_add(1)),
                "btc_txid": test_hex32(seed.wrapping_add(2)),
                "btc_wtxid": test_hex32(seed.wrapping_add(3)),
                "spend_input_index": 0,
                "ckb_btc_commitment_hash": test_hex32(seed.wrapping_add(4)),
                "sealed_utxo_commitment_hash": test_hex32(seed.wrapping_add(5)),
            }),
            _ => Value::Null,
        }
    }

    fn write_expected_live_fixture_reports(repo_root: &Path) -> BTreeMap<String, Value> {
        let mut reports = BTreeMap::<String, Value>::new();
        for (index, expected) in EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().enumerate() {
            let report = reports.entry(expected.live_report.to_string()).or_insert_with(|| json!({"status": "passed"}));
            set_json_pointer_string(report, expected.live_tx_hash_pointer, test_hex32(index as u8 + 1));
            if let Some(anchor_pointer) = expected_public_btc_anchor_pointer(expected.profile) {
                let anchor = test_public_btc_anchor(expected.profile, index as u8 + 0x40);
                let ckb_btc_commitment_hash = json_pointer_str(&anchor, "/ckb_btc_commitment_hash").unwrap().to_string();
                set_json_pointer_value(report, anchor_pointer, anchor);
                if let Some(commitment_pointer) = expected_public_btc_commitment_hash_pointer(expected.profile) {
                    set_json_pointer_string(report, commitment_pointer, ckb_btc_commitment_hash);
                }
            }
        }
        for (path, report) in &reports {
            let absolute = repo_root.join(path);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
            std::fs::write(&absolute, serde_json::to_string(report).unwrap()).unwrap();
        }
        let fiber = json!({"workflow_coverage": {"all_required_workflows_executed_passed": true}});
        let fiber_path = repo_root.join(FIBER_NODE_EXPERIMENTS);
        std::fs::create_dir_all(fiber_path.parent().unwrap()).unwrap();
        std::fs::write(&fiber_path, serde_json::to_string(&fiber).unwrap()).unwrap();
        reports
    }

    fn operator_fixture_report(repo_root: &Path) -> Value {
        let mut cases = Vec::new();
        let mut profiles = BTreeSet::new();
        for expected in EXPECTED_PROFILE_OPERATOR_FIXTURES {
            profiles.insert(expected.profile);
            let live_report = json_load_path(repo_root, &repo_root.join(expected.live_report)).unwrap();
            let live_report_hash = novaseal_profile_operator_report_hash(expected.live_report, &live_report);
            let live_tx_hash = json_pointer_str(&live_report, expected.live_tx_hash_pointer).unwrap();
            let fiber_report_hash = expected.fiber_report.map(|path| {
                let report = json_load_path(repo_root, &repo_root.join(path)).unwrap();
                novaseal_profile_operator_report_hash(path, &report)
            });
            let public_btc_anchor = expected_public_btc_anchor_pointer(expected.profile)
                .and_then(|pointer| live_report.pointer(pointer))
                .cloned()
                .unwrap_or(Value::Null);
            let hash = test_hex32(0xaa);
            cases.push(json!({
                "profile": expected.profile,
                "action": expected.action,
                "fixture": expected.fixture,
                "status": "passed",
                "checks": {"fixture_expected_accepted": true},
                "signers": expected.signers,
                "signed_type": "NovaFixtureSignedIntentV0",
                "signed_intent_hash": hash,
                "bip340_message_hash": hash,
                "signed_intent_body_hex": "0x00",
                "signed_intent_hash_preimage_hex": "0x01",
                "witness_shape_hash": test_hex32(0x11),
                "tx_skeleton_hash": test_hex32(0x12),
                "fixture_hash": test_hex32(0x13),
                "source_tree_hash": test_hex32(0x14),
                "schema_set_hash": test_hex32(0x15),
                "proof_matrix_hash": test_hex32(0x16),
                "live_report_hash": live_report_hash,
                "fiber_report_hash": fiber_report_hash,
                "live_devnet_tx_hash": live_tx_hash,
                "public_btc_anchor": public_btc_anchor.clone(),
                "wallet_display": {
                    "profile": expected.profile,
                    "action": expected.action,
                    "live_devnet_tx_hash": live_tx_hash,
                    "public_btc_anchor": public_btc_anchor,
                },
            }));
        }
        json!({
            "schema": "novaseal-profile-operator-fixtures-v0.1",
            "status": "passed",
            "summary": {
                "total": EXPECTED_PROFILE_OPERATOR_FIXTURES.len(),
                "matched": EXPECTED_PROFILE_OPERATOR_FIXTURES.len(),
                "profile_count": profiles.len(),
            },
            "cases": cases,
        })
    }

    fn service_builder_report(operator_fixtures: &Value) -> Value {
        let operator_cases = operator_fixtures.get("cases").and_then(Value::as_array).unwrap();
        let mut cases = Vec::new();
        let mut profiles = BTreeSet::new();
        for operator_case in operator_cases {
            let profile = json_pointer_str(operator_case, "/profile").unwrap();
            let action = json_pointer_str(operator_case, "/action").unwrap();
            let fixture = json_pointer_str(operator_case, "/fixture").unwrap();
            let signers = json_array_strings(operator_case, "/signers");
            profiles.insert(profile.to_string());
            let operator_fixture_hash = novaseal_service_builder_report_hash("operator_case", operator_case);
            let signed_hash = json_pointer_str(operator_case, "/signed_intent_hash").unwrap();
            let witness_hash = json_pointer_str(operator_case, "/witness_shape_hash").unwrap();
            let public_btc_anchor = operator_case.pointer("/public_btc_anchor").cloned().unwrap_or(Value::Null);
            cases.push(json!({
                "profile": profile,
                "action": action,
                "fixture": fixture,
                "status": "passed",
                "checks": {"operator_case_passed": true},
                "builder_name": "novaseal-profile-service-builder-v0",
                "operator_fixture_hash": operator_fixture_hash,
                "signers": signers,
                "request": {
                    "schema": "novaseal-service-builder-request-v0.1",
                    "builder_name": "novaseal-profile-service-builder-v0",
                    "profile": profile,
                    "action": action,
                    "signers": signers,
                    "idempotency_key": test_hex32(0x20),
                    "operator_fixture_hash": operator_fixture_hash,
                    "required_profile_inputs": {
                        "source_tree_hash": json_pointer_str(operator_case, "/source_tree_hash"),
                        "schema_set_hash": json_pointer_str(operator_case, "/schema_set_hash"),
                        "proof_matrix_hash": json_pointer_str(operator_case, "/proof_matrix_hash"),
                        "fixture_hash": json_pointer_str(operator_case, "/fixture_hash"),
                    },
                    "required_live_inputs": {
                        "live_report_hash": json_pointer_str(operator_case, "/live_report_hash"),
                        "live_devnet_tx_hash": json_pointer_str(operator_case, "/live_devnet_tx_hash"),
                        "fiber_report_hash": json_pointer_str(operator_case, "/fiber_report_hash"),
                        "public_btc_anchor": public_btc_anchor.clone(),
                    },
                    "production_external_inputs": ["public_shared_cell_dep_attestation"],
                },
                "response": {
                    "schema": "novaseal-service-builder-response-v0.1",
                    "profile": profile,
                    "action": action,
                    "service_queue_key": test_hex32(0x21),
                    "tx_skeleton_hash": test_hex32(0x22),
                    "witness_shape_hash": witness_hash,
                    "signed_intent_hash": signed_hash,
                    "bip340_message_hash": signed_hash,
                    "receipt_binding_hash": test_hex32(0x23),
                    "builder_trace_hash": test_hex32(0x24),
                },
                "tx_skeleton": {
                    "schema": "novaseal-service-builder-tx-skeleton-v0.1",
                    "operator_fixture_hash": operator_fixture_hash,
                    "public_btc_anchor": public_btc_anchor,
                },
            }));
        }
        json!({
            "schema": "novaseal-service-builder-fixtures-v0.1",
            "status": "passed",
            "builder_name": "novaseal-profile-service-builder-v0",
            "source_operator_fixture_report_hash": novaseal_service_builder_report_hash("operator_report", operator_fixtures),
            "summary": {
                "total": EXPECTED_PROFILE_OPERATOR_FIXTURES.len(),
                "matched": EXPECTED_PROFILE_OPERATOR_FIXTURES.len(),
                "profile_count": profiles.len(),
            },
            "cases": cases,
        })
    }

    fn wallet_lock_alignment_report(legacy_domain_hash_visible: bool) -> Value {
        let fixtures = (0..11)
            .map(|index| {
                json!({
                    "fixture": format!("fixture_{index}.json"),
                    "resolved_intent_size_bytes": 254,
                    "canonical_wallet_message32": test_hex32(0x30),
                    "current_lock_message32": test_hex32(0x30),
                    "current_lock_message_rule": "hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })",
                    "canonical_wallet_message_rule": "signed_intent_hash_after_resolved_receipt",
                    "canonical_vs_current_lock_digest_match": true,
                    "canonical_wallet_positive": {
                        "self_verified": true,
                    },
                    "current_lock_compat_positive": {
                        "self_verified": true,
                    },
                    "cross_check": {
                        "canonical_signature_accepts_current_lock_digest": true,
                        "current_lock_signature_accepts_canonical_digest": true,
                    },
                })
            })
            .collect::<Vec<_>>();
        json!({
            "schema": "novaseal-wallet-signing-alignment-v0.2",
            "classification": "wallet_signing_vectors_and_lock_digest_alignment_probe",
            "source_digest_model": {
                "all_required_snippets_present": !legacy_domain_hash_visible,
                "state_type_uses_packed_signed_intent_hash": true,
                "state_type_verifier_uses_signed_intent_hash": true,
                "package_lock_uses_packed_digest": true,
                "standalone_lock_uses_packed_digest": true,
                "legacy_domain_hash_visible": legacy_domain_hash_visible,
            },
            "summary": {
                "fixtures": 11,
                "canonical_wallet_vectors_self_verified": 11,
                "current_lock_compat_vectors_self_verified": 11,
                "current_lock_digest_matches_canonical": 11,
                "current_lock_digest_mismatches": 0,
                "canonical_wallet_signatures_accepted_by_current_lock_digest": 11,
                "current_lock_signatures_accepted_by_canonical_wallet_digest": 11,
                "wallet_lock_alignment_ready": true,
                "production_wallet_ready": true,
            },
            "fixtures": fixtures,
        })
    }

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
    fn novaseal_handoff_hash_matches_python_generator_vector() {
        let value = json!({
            "z": 1,
            "a": ["b", true, null],
        });

        assert_eq!(canonical_json_for_report_hash(&value), r#"{"a":["b",true,null],"z":1}"#);
        assert_eq!(
            novaseal_handoff_report_hash("test_label", &value),
            "0x91f5e5cc38c16e792d27a3738a7a7c77053fa15f902e2ccb4b210fd7239a476f"
        );
    }

    #[test]
    fn wallet_lock_alignment_rejects_legacy_domain_hash_source_model() {
        let valid = validate_wallet_lock_alignment_detail(&wallet_lock_alignment_report(false));
        assert_eq!(json_pointer_str(&valid, "/status"), Some("passed"));

        let legacy = validate_wallet_lock_alignment_detail(&wallet_lock_alignment_report(true));
        assert_eq!(json_pointer_str(&legacy, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&legacy, "/checks/source_model_uses_packed_intent"));
    }

    #[test]
    fn live_stateful_core_requires_tx_hashes_and_post_negative_liveness() {
        let mut live_core = json!({
            "status": "passed",
            "live_devnet_rpc_executed": true,
            "stateful_lifecycle_executed": true,
            "provenance_freshness_matched": true,
            "bootstrap_tx_hash": test_hex32(1),
            "bootstrap_state_cell_live": true,
            "transition_tx_hash": test_hex32(2),
            "old_state_not_live": true,
            "new_state_live": true,
            "receipt_live": true,
            "wrong_signature_rejected": true,
            "post_negative_state_still_live": true,
        });

        assert!(core_live_summary_passed(&live_core));

        live_core["transition_tx_hash"] = Value::Null;
        assert!(!core_live_summary_passed(&live_core));

        live_core["transition_tx_hash"] = Value::String(test_hex32(2));
        live_core["post_negative_state_still_live"] = Value::Bool(false);
        assert!(!core_live_summary_passed(&live_core));
    }

    #[test]
    fn live_stateful_agreement_requires_all_transaction_hashes() {
        let conformance = json!({"status": "passed"});
        let mut live_agreement = json!({
            "status": "passed",
            "live_devnet_rpc_executed": true,
            "stateful_lifecycle_executed": true,
            "provenance_freshness_matched": true,
            "originate_tx_hash": test_hex32(1),
            "repay_tx_hash": test_hex32(2),
            "claim_originate_tx_hash": test_hex32(3),
            "claim_tx_hash": test_hex32(4),
            "origin_active_live": true,
            "origin_principal_payout_live": true,
            "origin_receipt_live": true,
            "claim_origin_active_live": true,
            "claim_origin_principal_payout_live": true,
            "claim_origin_receipt_live": true,
            "repay_old_active_not_live": true,
            "repay_closed_live": true,
            "repay_lender_repayment_live": true,
            "repay_borrower_collateral_return_live": true,
            "repay_receipt_live": true,
            "claim_old_active_not_live": true,
            "claim_closed_live": true,
            "claim_lender_default_claim_live": true,
            "claim_receipt_live": true,
            "wrong_lender_signature_rejected": true,
            "non_ckb_asset_kind_rejected": true,
            "wrong_borrower_signature_rejected": true,
            "repay_payout_capacity_short_rejected": true,
            "repay_payout_lock_args_mismatch_rejected": true,
            "repay_wrong_payout_amount_rejected": true,
            "early_claim_rejected": true,
            "wrong_lender_claim_signature_rejected": true,
            "post_negative_active_still_live": true,
            "post_claim_negative_active_still_live": true,
        });

        assert!(agreement_live_summary_passed(&live_agreement, &conformance));

        live_agreement["claim_tx_hash"] = Value::Null;
        assert!(!agreement_live_summary_passed(&live_agreement, &conformance));
    }

    #[test]
    fn provenance_freshness_requires_repo_commit_match() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source_paths = ["src/cli/novaseal_certification.rs"];
        let current_source = source_tree_hash(repo_root, &source_paths).unwrap();
        let artifact_path = repo_root.join(source_paths[0]);
        let artifact_sha = sha256_file_hex(&artifact_path).unwrap();
        let report = json!({
            "provenance": {
                "repo_commit": "0000000000000000000000000000000000000000",
                "source_tree": current_source,
                "artifacts": {
                    "verifier": {
                        "path": artifact_path.display().to_string(),
                        "sha256": artifact_sha,
                    },
                    "lifecycle": {
                        "path": artifact_path.display().to_string(),
                        "sha256": artifact_sha,
                    },
                },
            },
        });

        let summary = provenance_summary(&report, repo_root, &source_paths).unwrap();

        assert!(json_pointer_bool(&summary, "/source_hash_matches"));
        assert!(json_pointer_bool(&summary, "/artifact_hashes_match"));
        assert!(!json_pointer_bool(&summary, "/repo_commit_matches"));
        assert!(!json_pointer_bool(&summary, "/freshness_matched"));
    }

    #[cfg(unix)]
    #[test]
    fn source_tree_expected_files_and_provenance_reject_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path().join("repo");
        let src = repo_root.join("src");
        let schemas = repo_root.join("schemas");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&schemas).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(src.join("real.cell"), "module real;\n").unwrap();
        std::fs::write(outside.join("external.cell"), "module external;\n").unwrap();
        std::fs::write(outside.join("schema.schema"), "array External {}\n").unwrap();
        symlink(outside.join("external.cell"), src.join("linked.cell")).unwrap();
        symlink(outside.join("schema.schema"), schemas.join("expected.schema")).unwrap();

        let source = source_tree_hash(&repo_root, &["src"]).unwrap();
        assert!(!json_pointer_bool(&source, "/valid"));
        assert!(json_pointer_str(&source, "/sha256").is_none());
        let invalid_source_paths = json_array_strings(&source, "/invalid_paths");
        assert!(invalid_source_paths.iter().any(|path| path.ends_with("src/linked.cell")));

        let artifact_sha = sha256_file_hex(&src.join("real.cell")).unwrap();
        let invalid_source_report = json!({
            "provenance": {
                "source_tree": source,
                "artifacts": {
                    "verifier": {
                        "path": src.join("real.cell").display().to_string(),
                        "sha256": artifact_sha,
                    },
                    "lifecycle": {
                        "path": src.join("real.cell").display().to_string(),
                        "sha256": artifact_sha,
                    },
                },
            },
        });
        let invalid_source_summary = provenance_summary(&invalid_source_report, &repo_root, &["src"]).unwrap();
        assert!(!json_pointer_bool(&invalid_source_summary, "/source_hash_matches"));
        assert!(!json_pointer_bool(&invalid_source_summary, "/current_source_valid"));
        assert!(!json_pointer_bool(&invalid_source_summary, "/freshness_matched"));

        let expected = expected_files(&repo_root, &schemas, &["expected.schema"]).unwrap();
        assert!(!json_pointer_bool(&expected, "/exact"));
        assert!(json_array_strings(&expected, "/invalid").iter().any(|path| path == "expected.schema"));
        assert!(expected.pointer("/hashes/expected.schema").is_none());

        let current_source = source_tree_hash(&repo_root, &["src/real.cell"]).unwrap();
        assert!(json_pointer_bool(&current_source, "/valid"));
        let outside_artifact = outside.join("artifact.elf");
        std::fs::write(&outside_artifact, b"artifact").unwrap();
        let artifact_sha = sha256_file_hex(&outside_artifact).unwrap();
        let report = json!({
            "provenance": {
                "source_tree": current_source,
                "artifacts": {
                    "verifier": {
                        "path": outside_artifact.display().to_string(),
                        "sha256": artifact_sha,
                    },
                    "lifecycle": {
                        "path": outside_artifact.display().to_string(),
                        "sha256": artifact_sha,
                    },
                },
            },
        });

        let summary = provenance_summary(&report, &repo_root, &["src/real.cell"]).unwrap();
        assert!(json_pointer_bool(&summary, "/source_hash_matches"));
        assert!(!json_pointer_bool(&summary, "/artifact_hashes_match"));
        assert!(!json_pointer_bool(&summary, "/artifacts/verifier/regular_file_within_repo"));
        assert!(!json_pointer_bool(&summary, "/freshness_matched"));
    }

    #[cfg(unix)]
    #[test]
    fn json_evidence_reports_must_be_regular_files_within_repo_root() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path().join("repo");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&repo_root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("external.json"), "{}\n").unwrap();

        symlink(outside.join("external.json"), repo_root.join("linked.json")).unwrap();
        let err = json_load_path(&repo_root, &repo_root.join("linked.json")).unwrap_err();
        assert!(
            err.message.contains("refusing to read JSON report") && err.message.contains("regular file within repository root"),
            "unexpected error: {}",
            err.message
        );

        symlink(&outside, repo_root.join("linked-dir")).unwrap();
        let err = json_load_path_optional(&repo_root, &repo_root.join("linked-dir/external.json")).unwrap_err();
        assert!(
            err.message.contains("refusing to read JSON report") && err.message.contains("resolves outside repository root"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn rwa_lifecycle_event_data_hash_binding_is_required_and_present() {
        let (_, pattern) = REQUIRED_RWA_RECEIPT_SOURCE_PATTERNS
            .iter()
            .find(|(name, _)| *name == "expected_event_data_hash")
            .expect("RWA certification must require event data hash binding");
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source = read_cell_sources(&repo_root.join("proposals/novaseal/rwa-receipt-profile-v0/src")).unwrap();

        assert_eq!(*pattern, "intent.expected_event_data_hash == ckb::hash_data_packed(event)");
        assert!(source.contains(pattern), "RWA lifecycle source must bind event output hash to derived event");
    }

    #[test]
    fn profile_operator_fixtures_bind_current_live_reports() {
        let temp = tempfile::tempdir().unwrap();
        write_expected_live_fixture_reports(temp.path());
        let report = operator_fixture_report(temp.path());

        let detail = validate_profile_operator_fixture_detail(temp.path(), &report).unwrap();
        assert_eq!(json_pointer_str(&detail, "/status"), Some("passed"));

        let mut stale = report;
        stale["cases"][0]["live_report_hash"] = Value::String(test_hex32(0xfe));
        let detail = validate_profile_operator_fixture_detail(temp.path(), &stale).unwrap();
        assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&detail, "/cases/fungible-xudt-profile-v0:issue_xudt/live_report_hash_matches_current_report",));

        let mut stale_anchor = operator_fixture_report(temp.path());
        let btc_index = EXPECTED_PROFILE_OPERATOR_FIXTURES
            .iter()
            .position(|fixture| fixture.profile == EXPECTED_BTC_TX_COMMITMENT_PROFILE)
            .unwrap();
        stale_anchor["cases"][btc_index]["public_btc_anchor"]["ckb_btc_commitment_hash"] = Value::String(test_hex32(0xfc));
        let detail = validate_profile_operator_fixture_detail(temp.path(), &stale_anchor).unwrap();
        assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &detail,
            "/cases/btc-transaction-commitment-profile-v0:commit_btc_transaction_transition/public_btc_anchor_matches_current_report",
        ));

        let mut stale_dual_anchor = operator_fixture_report(temp.path());
        let dual_index =
            EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().position(|fixture| fixture.profile == EXPECTED_DUAL_SEAL_PROFILE).unwrap();
        stale_dual_anchor["cases"][dual_index]["public_btc_anchor"].as_object_mut().unwrap().remove("sealed_btc_txid");
        stale_dual_anchor["cases"][dual_index]["wallet_display"]["public_btc_anchor"]
            .as_object_mut()
            .unwrap()
            .remove("sealed_btc_txid");
        let detail = validate_profile_operator_fixture_detail(temp.path(), &stale_dual_anchor).unwrap();
        assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
        assert!(
            !json_pointer_bool(&detail, "/cases/dual-seal-profile-v0:finalize_dual_seal/public_btc_anchor_shape_matches_profile",)
        );
    }

    #[test]
    fn certification_gate_status_uses_detailed_validator_result() {
        let profile_certification = json!({
            "profile_operator_fixtures": {
                "status": "failed",
                "checks": {
                    "report_passed": true,
                    "summary_counts_match": true,
                    "case_details": false
                }
            }
        });

        let gate = gate(
            "planned_profile_operator_fixtures",
            certification_detail_status(&profile_certification, "/profile_operator_fixtures/status"),
            PROFILE_OPERATOR_FIXTURES,
            certification_detail(&profile_certification, "/profile_operator_fixtures"),
        );

        assert_eq!(json_pointer_str(&gate, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&gate, "/detail/checks/case_details"));
    }

    #[test]
    fn service_builder_fixtures_bind_current_operator_report() {
        let temp = tempfile::tempdir().unwrap();
        write_expected_live_fixture_reports(temp.path());
        let operator_report = operator_fixture_report(temp.path());
        let report = service_builder_report(&operator_report);

        let detail = validate_service_builder_fixture_detail(&report, &operator_report);
        assert_eq!(json_pointer_str(&detail, "/status"), Some("passed"));

        let mut stale = report.clone();
        stale["source_operator_fixture_report_hash"] = Value::String(test_hex32(0xfd));
        let detail = validate_service_builder_fixture_detail(&stale, &operator_report);
        assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&detail, "/checks/source_operator_fixture_report_hash_matches_current_report"));

        let mut stale_anchor = report;
        let btc_index = EXPECTED_PROFILE_OPERATOR_FIXTURES
            .iter()
            .position(|fixture| fixture.profile == EXPECTED_BTC_TX_COMMITMENT_PROFILE)
            .unwrap();
        stale_anchor["cases"][btc_index]["request"]["required_live_inputs"]["public_btc_anchor"]["ckb_btc_commitment_hash"] =
            Value::String(test_hex32(0xfb));
        let detail = validate_service_builder_fixture_detail(&stale_anchor, &operator_report);
        assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &detail,
            "/cases/btc-transaction-commitment-profile-v0:commit_btc_transaction_transition/public_btc_anchor_input_matches_operator_fixture",
        ));

        let mut stale_dual_anchor = service_builder_report(&operator_report);
        let dual_index =
            EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().position(|fixture| fixture.profile == EXPECTED_DUAL_SEAL_PROFILE).unwrap();
        stale_dual_anchor["cases"][dual_index]["request"]["required_live_inputs"]["public_btc_anchor"]
            .as_object_mut()
            .unwrap()
            .remove("sealed_utxo_commitment_hash");
        stale_dual_anchor["cases"][dual_index]["tx_skeleton"]["public_btc_anchor"]
            .as_object_mut()
            .unwrap()
            .remove("sealed_utxo_commitment_hash");
        let detail = validate_service_builder_fixture_detail(&stale_dual_anchor, &operator_report);
        assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &detail,
            "/cases/dual-seal-profile-v0:finalize_dual_seal/public_btc_anchor_input_shape_matches_profile",
        ));
    }

    #[test]
    fn external_evidence_handoff_rejects_stale_source_hashes_and_paths() {
        let btc_handoff_fields = EXPECTED_PUBLIC_BTC_SPV_HANDOFF_FIELDS;
        let public_attestation_handoff_fields = EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS;
        let external_review_handoff_fields = EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS;
        let rwa_legal_review_handoff_fields = EXPECTED_RWA_LEGAL_REVIEW_REQUIRED_FIELDS;
        let public_manifest_commit = "0123456789abcdef0123456789abcdef01234567";
        let public_release_package = "novaseal";
        let public_release_version = EXPECTED_NOVASEAL_RELEASE_VERSION;
        let public_dep_type = EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE;
        let public_hash_type = EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE;
        let public_ipc_abi = "cellscript-btc-bip340-ipc-v0";
        let public_verifier_id = "btc.bip340.v0";
        let public_artifact_hash = format!("0x{}", "99".repeat(32));
        let external_artifact_hash = format!("0x{}", "aa".repeat(32));
        let external_source_tree_hash = format!("0x{}", "bb".repeat(32));
        let rwa_profile_source_hash = source_tree_hash(Path::new("."), RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)
            .unwrap()
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let expected_btc_scenarios = EXPECTED_BTC_SPV_PROFILE_SCENARIOS
            .iter()
            .map(|(profile, scenario)| ((*profile).to_string(), Value::String((*scenario).to_string())))
            .collect::<Map<String, Value>>();
        let expected_btc_bindings = EXPECTED_BTC_SPV_EVIDENCE_PROFILES
            .iter()
            .enumerate()
            .map(|(index, profile)| {
                let byte = index as u8 + 0x50;
                let btc_material = test_btc_profile_material(profile, byte);
                let mut binding = json!({
                    "ckb_live_tx_hash": test_hex32(byte),
                    "live_report_hash": test_hex32(byte + 1),
                    "service_builder_case_hash": test_hex32(byte + 2),
                    "service_builder_tx_skeleton_hash": test_hex32(byte + 3),
                    "service_builder_receipt_binding_hash": test_hex32(byte + 4),
                    "ckb_btc_commitment_hash": test_hex32(byte + 5),
                });
                merge_expected_btc_binding_fields(&mut binding, &btc_material);
                ((*profile).to_string(), binding)
            })
            .collect::<Map<String, Value>>();
        let btc_spv_adapter = json!({
            "status": "passed",
            "adapter_status": "request_ready_external_evidence_required",
            "production_output": PUBLIC_BTC_SPV_EVIDENCE,
            "summary": { "total": 3, "matched": 3 },
            "cases": EXPECTED_BTC_SPV_EVIDENCE_PROFILES
                .iter()
                .enumerate()
                .map(|(index, profile)| {
                    let profile = *profile;
                    let bindings = expected_btc_bindings.get(profile).unwrap();
                    let mut request = json!({
                        "scenario": expected_btc_spv_scenario(profile).unwrap(),
                        "ckb_live_tx_hash": json_pointer_str(bindings, "/ckb_live_tx_hash").unwrap(),
                        "live_report_hash": json_pointer_str(bindings, "/live_report_hash").unwrap(),
                        "service_builder_case_hash": json_pointer_str(bindings, "/service_builder_case_hash").unwrap(),
                        "service_builder_tx_skeleton_hash": json_pointer_str(bindings, "/service_builder_tx_skeleton_hash").unwrap(),
                        "service_builder_receipt_binding_hash": json_pointer_str(bindings, "/service_builder_receipt_binding_hash").unwrap(),
                        "ckb_btc_commitment_hash": json_pointer_str(bindings, "/ckb_btc_commitment_hash").unwrap(),
                    });
                    let btc_material = test_btc_profile_material(profile, index as u8 + 0x50);
                    add_expected_btc_request_fields(&mut request, &btc_material);
                    json!({
                        "profile": profile,
                        "status": "passed",
                        "request": request,
                    })
                })
                .collect::<Vec<_>>(),
        });
        let external_attestation_adapter = json!({
            "status": "passed",
            "adapter_status": "request_ready_external_attestations_required",
            "summary": { "total": 2, "matched": 2 },
            "cases": [
                {
                    "name": "public_shared_cell_dep_attestation",
                    "status": "passed",
                    "request": {
                        "production_output": PUBLIC_CELLDEP_ATTESTATION,
                        "required_public_fields": ["network"],
                        "expected_artifact_hash": public_artifact_hash,
                        "expected_release_package": public_release_package,
                        "expected_release_version": public_release_version,
                        "expected_release_manifest_commit": public_manifest_commit,
                        "expected_dep_type": public_dep_type,
                        "expected_hash_type": public_hash_type,
                        "ipc_abi": public_ipc_abi,
                        "verifier_id": public_verifier_id,
                    },
                },
                {
                    "name": "external_bip340_tcb_review_attestation",
                    "status": "passed",
                    "request": {
                        "production_output": EXTERNAL_TCB_ATTESTATION,
                        "required_public_fields": ["reviewer"],
                        "expected_artifact_hash": external_artifact_hash,
                        "expected_artifact_hash_algorithm": "sha256",
                        "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                        "verifier_id": "btc.bip340.v0",
                        "expected_source_tree_sha256": external_source_tree_hash,
                        "expected_review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                    },
                },
            ],
        });
        let btc_hash = novaseal_handoff_report_hash("btc_spv_adapter", &btc_spv_adapter);
        let attestation_hash = novaseal_handoff_report_hash("external_attestation_adapter", &external_attestation_adapter);
        let mut report = json!({
            "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
            "status": "passed",
            "handoff_status": "request_bundle_ready_external_evidence_required",
            "source_btc_spv_adapter": BTC_SPV_EVIDENCE_ADAPTER,
            "source_btc_spv_adapter_hash": btc_hash,
            "source_external_attestation_adapter": EXTERNAL_ATTESTATION_ADAPTER,
            "source_external_attestation_adapter_hash": attestation_hash,
            "production_outputs": [
                PUBLIC_BTC_SPV_EVIDENCE,
                PUBLIC_CELLDEP_ATTESTATION,
                EXTERNAL_TCB_ATTESTATION,
                RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
            ],
            "summary": {
                "total": 4,
                "matched": 4,
            },
            "cases": [
                {
                    "group": "public_btc_spv_evidence",
                    "status": "passed",
                    "source_adapter": BTC_SPV_EVIDENCE_ADAPTER,
                    "source_adapter_hash": btc_hash,
                    "production_output": PUBLIC_BTC_SPV_EVIDENCE,
                    "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
                    "expected_scenarios": expected_btc_scenarios.clone(),
                    "expected_case_bindings": expected_btc_bindings.clone(),
                    "required_external_fields": btc_handoff_fields,
                    "field_constraints": constraint_object(EXPECTED_BTC_SPV_FIELD_CONSTRAINTS),
                    "checks": { "ok": true },
                },
                {
                    "group": "public_shared_cell_dep_attestation",
                    "status": "passed",
                    "source_adapter": EXTERNAL_ATTESTATION_ADAPTER,
                    "source_adapter_hash": attestation_hash,
                    "production_output": PUBLIC_CELLDEP_ATTESTATION,
                    "required_external_fields": public_attestation_handoff_fields,
                    "field_constraints": constraint_object(EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS),
                    "expected_values": {
                        "artifact_hash": public_artifact_hash,
                        "release.package": public_release_package,
                        "release.version": public_release_version,
                        "release.manifest_commit": public_manifest_commit,
                        "runtime_verifier.dep_type": public_dep_type,
                        "runtime_verifier.hash_type": public_hash_type,
                        "runtime_verifier.ipc_abi": public_ipc_abi,
                        "runtime_verifier.verifier_id": public_verifier_id,
                    },
                    "checks": { "ok": true },
                },
                {
                    "group": "external_bip340_tcb_review_attestation",
                    "status": "passed",
                    "source_adapter": EXTERNAL_ATTESTATION_ADAPTER,
                    "source_adapter_hash": attestation_hash,
                    "production_output": EXTERNAL_TCB_ATTESTATION,
                    "required_external_fields": external_review_handoff_fields,
                    "field_constraints": constraint_object(EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS),
                    "expected_values": {
                        "artifact_hash": external_artifact_hash,
                        "artifact_hash_algorithm": "sha256",
                        "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                        "review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                        "source_tree_sha256": external_source_tree_hash,
                        "verifier_id": "btc.bip340.v0",
                    },
                    "checks": { "ok": true },
                },
                {
                    "group": "rwa_legal_registry_review_evidence",
                    "status": "passed",
                    "source_adapter": EXTERNAL_ATTESTATION_ADAPTER,
                    "source_adapter_hash": attestation_hash,
                    "production_output": RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
                    "required_external_fields": rwa_legal_review_handoff_fields,
                    "field_constraints": constraint_object(EXPECTED_RWA_LEGAL_REVIEW_FIELD_CONSTRAINTS),
                    "expected_values": {
                        "profile": EXPECTED_RWA_RECEIPT_PROFILE,
                        "profile_source_tree_sha256": rwa_profile_source_hash,
                        "review_scope": EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
                    },
                    "checks": { "ok": true },
                },
            ],
        });
        report["bundle_hash_algorithm"] = json!(NOVASEAL_HANDOFF_HASH_ALGORITHM);
        report["bundle_hash"] = json!(external_evidence_handoff_reference_hash(&report));

        let valid =
            validate_external_evidence_handoff_detail(Path::new("."), &report, &btc_spv_adapter, &external_attestation_adapter);
        assert_eq!(json_pointer_str(&valid, "/status"), Some("passed"));
        assert!(external_evidence_handoff_gate_passed(&report));
        assert!(json_pointer_bool(&valid, "/checks/bundle_hash_matches_reference"));
        assert!(json_pointer_bool(&valid, "/checks/bundle_hash_algorithm"));
        assert!(json_pointer_bool(&valid, "/cases/public_btc_spv_evidence/expected_scenarios_match_source_adapter"));
        assert!(json_pointer_bool(&valid, "/cases/public_btc_spv_evidence/expected_case_bindings_match_source_adapter"));
        assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"));
        assert!(json_pointer_bool(&valid, "/cases/external_bip340_tcb_review_attestation/expected_values_match_source_adapter"));
        assert!(json_pointer_bool(&valid, "/cases/rwa_legal_registry_review_evidence/expected_values_match_source_adapter"));

        let mut wrong_btc_anchor_source = report.clone();
        wrong_btc_anchor_source["cases"][0]["expected_case_bindings"][EXPECTED_BTC_TX_COMMITMENT_PROFILE]["anchor_source"] =
            json!("external_public_btc_spend");
        let failed_btc_anchor_source = validate_external_evidence_handoff_detail(
            Path::new("."),
            &wrong_btc_anchor_source,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_btc_anchor_source, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_btc_anchor_source, "/cases/public_btc_spv_evidence/expected_case_bindings_exact"));
        assert!(!json_pointer_bool(
            &failed_btc_anchor_source,
            "/cases/public_btc_spv_evidence/expected_case_bindings_match_source_adapter"
        ));

        let mut zero_btc_amount = report.clone();
        zero_btc_amount["cases"][0]["expected_case_bindings"][EXPECTED_BTC_TX_COMMITMENT_PROFILE]["btc_amount_sats"] = json!(0);
        let failed_zero_btc_amount = validate_external_evidence_handoff_detail(
            Path::new("."),
            &zero_btc_amount,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_zero_btc_amount, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_zero_btc_amount, "/cases/public_btc_spv_evidence/expected_case_bindings_exact"));
        assert!(!json_pointer_bool(
            &failed_zero_btc_amount,
            "/cases/public_btc_spv_evidence/expected_case_bindings_match_source_adapter"
        ));

        let mut stale_bundle_hash = report.clone();
        stale_bundle_hash["bundle_hash"] = json!(format!("0x{}", "22".repeat(32)));
        let stale_bundle = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_bundle_hash,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&stale_bundle, "/status"), Some("failed"));
        assert!(!external_evidence_handoff_gate_passed(&stale_bundle_hash));
        assert!(!json_pointer_bool(&stale_bundle, "/checks/bundle_hash_matches_reference"));

        let mut wrong_bundle_algorithm = report.clone();
        wrong_bundle_algorithm["bundle_hash_algorithm"] = json!("sha256");
        let wrong_algorithm = validate_external_evidence_handoff_detail(
            Path::new("."),
            &wrong_bundle_algorithm,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&wrong_algorithm, "/status"), Some("failed"));
        assert!(!external_evidence_handoff_gate_passed(&wrong_bundle_algorithm));
        assert!(!json_pointer_bool(&wrong_algorithm, "/checks/bundle_hash_algorithm"));

        let mut stale_hash = report.clone();
        stale_hash["source_btc_spv_adapter_hash"] = json!(format!("0x{}", "11".repeat(32)));
        let stale =
            validate_external_evidence_handoff_detail(Path::new("."), &stale_hash, &btc_spv_adapter, &external_attestation_adapter);
        assert_eq!(json_pointer_str(&stale, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&stale, "/checks/source_btc_spv_adapter_hash_matches_current"));

        let mut wrong_path = report.clone();
        wrong_path["cases"][1]["source_adapter"] = json!("target/other-report.json");
        let failed_path =
            validate_external_evidence_handoff_detail(Path::new("."), &wrong_path, &btc_spv_adapter, &external_attestation_adapter);
        assert_eq!(json_pointer_str(&failed_path, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_path, "/cases/public_shared_cell_dep_attestation/source_adapter_path_matches_current"));

        let mut missing_required_field = report.clone();
        missing_required_field["cases"][0]["required_external_fields"] = json!(btc_handoff_fields[..20].to_vec());
        let failed_fields = validate_external_evidence_handoff_detail(
            Path::new("."),
            &missing_required_field,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_fields, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_fields, "/cases/public_btc_spv_evidence/required_external_fields_complete"));

        let mut missing_constraint = report.clone();
        missing_constraint["cases"][0]["field_constraints"].as_object_mut().unwrap().remove("source_service.commit");
        let failed_constraint = validate_external_evidence_handoff_detail(
            Path::new("."),
            &missing_constraint,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

        let mut missing_report_hash_constraint = report.clone();
        missing_report_hash_constraint["cases"][0]["field_constraints"].as_object_mut().unwrap().remove("source_service.report_hash");
        let failed_report_hash_constraint = validate_external_evidence_handoff_detail(
            Path::new("."),
            &missing_report_hash_constraint,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_report_hash_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_report_hash_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

        let mut missing_celldep_out_point_constraint = report.clone();
        missing_celldep_out_point_constraint["cases"][0]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("spv_client_cell_dep.out_point");
        let failed_celldep_out_point_constraint = validate_external_evidence_handoff_detail(
            Path::new("."),
            &missing_celldep_out_point_constraint,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_celldep_out_point_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_celldep_out_point_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

        let mut missing_btc_txid_constraint = report.clone();
        missing_btc_txid_constraint["cases"][0]["field_constraints"].as_object_mut().unwrap().remove("btc_txid");
        let failed_btc_txid_constraint = validate_external_evidence_handoff_detail(
            Path::new("."),
            &missing_btc_txid_constraint,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_btc_txid_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_btc_txid_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

        let mut missing_handoff_group_constraint = report.clone();
        missing_handoff_group_constraint["cases"][0]["field_constraints"].as_object_mut().unwrap().remove("request_handoff.group");
        let failed_handoff_group_constraint = validate_external_evidence_handoff_detail(
            Path::new("."),
            &missing_handoff_group_constraint,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_handoff_group_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_handoff_group_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

        let mut missing_public_artifact_constraint = report.clone();
        missing_public_artifact_constraint["cases"][1]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("runtime_verifier.artifact_hash");
        let failed_public_artifact_constraint = validate_external_evidence_handoff_detail(
            Path::new("."),
            &missing_public_artifact_constraint,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_public_artifact_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_public_artifact_constraint,
            "/cases/public_shared_cell_dep_attestation/field_constraints_exact"
        ));

        let mut missing_external_source_constraint = report.clone();
        missing_external_source_constraint["cases"][2]["field_constraints"].as_object_mut().unwrap().remove("source_tree_sha256");
        let failed_external_source_constraint = validate_external_evidence_handoff_detail(
            Path::new("."),
            &missing_external_source_constraint,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_external_source_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_external_source_constraint,
            "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"
        ));

        let mut stale_expected_scenario = report.clone();
        stale_expected_scenario["cases"][0]["expected_scenarios"][EXPECTED_BTC_TX_COMMITMENT_PROFILE] =
            json!("generic-public-btc-proof");
        let failed_expected_scenario = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_expected_scenario,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_expected_scenario, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_expected_scenario,
            "/cases/public_btc_spv_evidence/expected_scenarios_match_source_adapter"
        ));

        let mut stale_expected_value = report.clone();
        stale_expected_value["cases"][1]["expected_values"]["release.manifest_commit"] =
            json!("fedcba9876543210fedcba9876543210fedcba98");
        let failed_expected_value = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_expected_value,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_expected_value, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_expected_value,
            "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
        ));

        let mut stale_release_package = report.clone();
        stale_release_package["cases"][1]["expected_values"]["release.package"] = json!("other-package");
        let failed_release_package = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_release_package,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_release_package, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_release_package,
            "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
        ));

        let mut stale_release_version = report.clone();
        stale_release_version["cases"][1]["expected_values"]["release.version"] = json!("0.0.2");
        let failed_release_version = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_release_version,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_release_version, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_release_version,
            "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
        ));

        let mut stale_hash_type = report.clone();
        stale_hash_type["cases"][1]["expected_values"]["runtime_verifier.hash_type"] = json!("type");
        let failed_hash_type = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_hash_type,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_hash_type, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_hash_type,
            "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
        ));

        let mut stale_dep_type = report.clone();
        stale_dep_type["cases"][1]["expected_values"]["runtime_verifier.dep_type"] = json!("dep_group");
        let failed_dep_type = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_dep_type,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_dep_type, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_dep_type,
            "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
        ));

        let mut stale_ipc_abi = report.clone();
        stale_ipc_abi["cases"][1]["expected_values"]["runtime_verifier.ipc_abi"] = json!("cellscript-btc-bip340-ipc-v1");
        let failed_ipc_abi =
            validate_external_evidence_handoff_detail(Path::new("."), &stale_ipc_abi, &btc_spv_adapter, &external_attestation_adapter);
        assert_eq!(json_pointer_str(&failed_ipc_abi, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_ipc_abi, "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"));

        let mut stale_verifier_id = report.clone();
        stale_verifier_id["cases"][1]["expected_values"]["runtime_verifier.verifier_id"] = json!("btc.bip340.v1");
        let failed_verifier_id = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_verifier_id,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_verifier_id, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_verifier_id,
            "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
        ));

        let mut stale_expected_review_scope = report.clone();
        stale_expected_review_scope["cases"][2]["expected_values"]["review_scope"] = json!(["BIP340 runtime verifier TCB"]);
        let failed_review_scope = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_expected_review_scope,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_review_scope, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_review_scope,
            "/cases/external_bip340_tcb_review_attestation/expected_values_match_source_adapter"
        ));

        let mut stale_external_ipc_abi = report.clone();
        stale_external_ipc_abi["cases"][2]["expected_values"]["ipc_abi"] = json!("cellscript-btc-bip340-ipc-v1");
        let failed_external_ipc_abi = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_external_ipc_abi,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_external_ipc_abi, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_external_ipc_abi,
            "/cases/external_bip340_tcb_review_attestation/expected_values_match_source_adapter"
        ));

        let mut stale_external_verifier_id = report.clone();
        stale_external_verifier_id["cases"][2]["expected_values"]["verifier_id"] = json!("btc.bip340.v1");
        let failed_external_verifier_id = validate_external_evidence_handoff_detail(
            Path::new("."),
            &stale_external_verifier_id,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_external_verifier_id, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_external_verifier_id,
            "/cases/external_bip340_tcb_review_attestation/expected_values_match_source_adapter"
        ));

        let mut unexpected_required_field = report;
        let mut extended_btc_fields = btc_handoff_fields.to_vec();
        extended_btc_fields.push("unexpected.shadow_field");
        unexpected_required_field["cases"][0]["required_external_fields"] = json!(extended_btc_fields);
        let failed_exact = validate_external_evidence_handoff_detail(
            Path::new("."),
            &unexpected_required_field,
            &btc_spv_adapter,
            &external_attestation_adapter,
        );
        assert_eq!(json_pointer_str(&failed_exact, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_exact, "/cases/public_btc_spv_evidence/required_external_fields_exact"));
    }

    #[test]
    fn external_attestation_adapter_requires_handoff_request_fields() {
        let full_public_fields = EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS;
        let full_review_fields = EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS;
        let public_manifest_commit = "0123456789abcdef0123456789abcdef01234567";
        let public_release_package = "novaseal";
        let public_release_version = EXPECTED_NOVASEAL_RELEASE_VERSION;
        let public_dep_type = EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE;
        let public_hash_type = EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE;
        let report = json!({
            "schema": "novaseal-external-attestation-adapter-v0.1",
            "status": "passed",
            "adapter_status": "request_ready_external_attestations_required",
            "source_tcb_review_hash": format!("0x{}", "aa".repeat(32)),
            "source_public_cell_dep_template_hash": format!("0x{}", "bb".repeat(32)),
            "source_external_tcb_template_hash": format!("0x{}", "cc".repeat(32)),
            "summary": { "total": 2, "matched": 2 },
            "cases": [
                {
                    "name": "public_shared_cell_dep_attestation",
                    "status": "passed",
                    "checks": { "ok": true },
                    "request": {
                        "production_output": PUBLIC_CELLDEP_ATTESTATION,
                        "template_schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
                        "template_hash": format!("0x{}", "dd".repeat(32)),
                        "verifier_id": "btc.bip340.v0",
                        "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                        "required_status": "attested",
                        "expected_artifact_hash": format!("0x{}", "ee".repeat(32)),
                        "expected_release_package": public_release_package,
                        "expected_release_version": public_release_version,
                        "expected_release_manifest_commit": public_manifest_commit,
                        "expected_dep_type": public_dep_type,
                        "expected_hash_type": public_hash_type,
                        "required_public_fields": full_public_fields,
                        "field_constraints": constraint_object(EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS),
                    },
                },
                {
                    "name": "external_bip340_tcb_review_attestation",
                    "status": "passed",
                    "checks": { "ok": true },
                    "request": {
                        "production_output": EXTERNAL_TCB_ATTESTATION,
                        "template_schema": "novaseal-bip340-external-tcb-review-attestation-v0.1",
                        "template_hash": format!("0x{}", "ff".repeat(32)),
                        "verifier_id": "btc.bip340.v0",
                        "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                        "required_status": "accepted",
                        "expected_artifact_hash": format!("0x{}", "11".repeat(32)),
                        "expected_artifact_hash_algorithm": "sha256",
                        "template_artifact_hash_algorithm": "sha256",
                        "expected_review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                        "required_public_fields": full_review_fields,
                        "field_constraints": constraint_object(EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS),
                    },
                },
            ],
        });

        let valid = validate_external_attestation_adapter_detail(&report);
        assert_eq!(json_pointer_str(&valid, "/status"), Some("passed"));
        assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/field_constraints_exact"));
        assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_release_package_current"));
        assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_release_version_current"));
        assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_dep_type_current"));
        assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_hash_type_current"));
        assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_release_manifest_commit_present"));
        assert!(json_pointer_bool(&valid, "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"));
        assert!(json_pointer_bool(&valid, "/cases/external_bip340_tcb_review_attestation/expected_review_scope_exact"));

        let mut missing_handoff_field = report.clone();
        missing_handoff_field["cases"][0]["request"]["required_public_fields"] = json!(full_public_fields[..15].to_vec());
        let failed = validate_external_attestation_adapter_detail(&missing_handoff_field);
        assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed, "/cases/public_shared_cell_dep_attestation/required_fields_complete"));

        let mut mismatched_algorithm = report.clone();
        mismatched_algorithm["cases"][1]["request"]["template_artifact_hash_algorithm"] = json!("ckb-blake2b256");
        let failed_algorithm = validate_external_attestation_adapter_detail(&mismatched_algorithm);
        assert_eq!(json_pointer_str(&failed_algorithm, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_algorithm,
            "/cases/external_bip340_tcb_review_attestation/artifact_hash_algorithm_matches_tcb"
        ));

        let mut stale_constraint = report.clone();
        stale_constraint["cases"][1]["request"]["field_constraints"]["report_uri"] = json!("any URI");
        let failed_constraint = validate_external_attestation_adapter_detail(&stale_constraint);
        assert_eq!(json_pointer_str(&failed_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_constraint, "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"));

        let mut missing_public_handoff_hash_constraint = report.clone();
        missing_public_handoff_hash_constraint["cases"][0]["request"]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("request_handoff.bundle_hash");
        let failed_public_handoff_hash_constraint =
            validate_external_attestation_adapter_detail(&missing_public_handoff_hash_constraint);
        assert_eq!(json_pointer_str(&failed_public_handoff_hash_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_public_handoff_hash_constraint,
            "/cases/public_shared_cell_dep_attestation/field_constraints_exact"
        ));

        let mut missing_public_artifact_constraint = report.clone();
        missing_public_artifact_constraint["cases"][0]["request"]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("runtime_verifier.artifact_hash");
        let failed_public_artifact_constraint = validate_external_attestation_adapter_detail(&missing_public_artifact_constraint);
        assert_eq!(json_pointer_str(&failed_public_artifact_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_public_artifact_constraint,
            "/cases/public_shared_cell_dep_attestation/field_constraints_exact"
        ));

        let mut missing_tcb_handoff_group_constraint = report.clone();
        missing_tcb_handoff_group_constraint["cases"][1]["request"]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("request_handoff.group");
        let failed_tcb_handoff_group_constraint = validate_external_attestation_adapter_detail(&missing_tcb_handoff_group_constraint);
        assert_eq!(json_pointer_str(&failed_tcb_handoff_group_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_tcb_handoff_group_constraint,
            "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"
        ));

        let mut missing_tcb_source_constraint = report.clone();
        missing_tcb_source_constraint["cases"][1]["request"]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("source_tree_sha256");
        let failed_tcb_source_constraint = validate_external_attestation_adapter_detail(&missing_tcb_source_constraint);
        assert_eq!(json_pointer_str(&failed_tcb_source_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_tcb_source_constraint,
            "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"
        ));

        let mut stale_review_scope = report.clone();
        stale_review_scope["cases"][1]["request"]["expected_review_scope"] = json!(["BIP340 runtime verifier TCB"]);
        let failed_review_scope = validate_external_attestation_adapter_detail(&stale_review_scope);
        assert_eq!(json_pointer_str(&failed_review_scope, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_review_scope, "/cases/external_bip340_tcb_review_attestation/expected_review_scope_exact"));

        let mut missing_expected_commit = report.clone();
        missing_expected_commit["cases"][0]["request"]["expected_release_manifest_commit"] = json!("REPLACE_WITH_GIT_COMMIT");
        let failed_expected_commit = validate_external_attestation_adapter_detail(&missing_expected_commit);
        assert_eq!(json_pointer_str(&failed_expected_commit, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_expected_commit,
            "/cases/public_shared_cell_dep_attestation/expected_release_manifest_commit_present"
        ));

        let mut stale_expected_version = report.clone();
        stale_expected_version["cases"][0]["request"]["expected_release_version"] = json!("0.0.2");
        let failed_expected_version = validate_external_attestation_adapter_detail(&stale_expected_version);
        assert_eq!(json_pointer_str(&failed_expected_version, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_expected_version,
            "/cases/public_shared_cell_dep_attestation/expected_release_version_current"
        ));

        let mut stale_expected_package = report.clone();
        stale_expected_package["cases"][0]["request"]["expected_release_package"] = json!("other-package");
        let failed_expected_package = validate_external_attestation_adapter_detail(&stale_expected_package);
        assert_eq!(json_pointer_str(&failed_expected_package, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_expected_package,
            "/cases/public_shared_cell_dep_attestation/expected_release_package_current"
        ));

        let mut stale_expected_hash_type = report.clone();
        stale_expected_hash_type["cases"][0]["request"]["expected_hash_type"] = json!("type");
        let failed_expected_hash_type = validate_external_attestation_adapter_detail(&stale_expected_hash_type);
        assert_eq!(json_pointer_str(&failed_expected_hash_type, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_expected_hash_type,
            "/cases/public_shared_cell_dep_attestation/expected_hash_type_current"
        ));

        let mut stale_expected_dep_type = report.clone();
        stale_expected_dep_type["cases"][0]["request"]["expected_dep_type"] = json!("dep_group");
        let failed_expected_dep_type = validate_external_attestation_adapter_detail(&stale_expected_dep_type);
        assert_eq!(json_pointer_str(&failed_expected_dep_type, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_expected_dep_type, "/cases/public_shared_cell_dep_attestation/expected_dep_type_current"));

        let tcb_review = json!({
            "repo_commit": public_manifest_commit,
            "runtime_artifact": {
                "artifact_hash": format!("0x{}", "11".repeat(32)),
                "artifact_hash_algorithm": "sha256",
            },
            "source_inventory": {
                "source_tree_sha256": format!("0x{}", "22".repeat(32)),
            },
        });
        let public_template = json!({
            "schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
            "status": "attested",
        });
        let external_template = json!({
            "schema": "novaseal-bip340-external-tcb-review-attestation-v0.1",
            "status": "accepted",
        });
        let mut source_bound = report.clone();
        source_bound["source_tcb_review_hash"] = json!(novaseal_external_attestation_report_hash("tcb_review", &tcb_review));
        source_bound["source_public_cell_dep_template_hash"] =
            json!(novaseal_external_attestation_report_hash("public_celldep_template", &public_template));
        source_bound["source_external_tcb_template_hash"] =
            json!(novaseal_external_attestation_report_hash("external_tcb_template", &external_template));
        source_bound["cases"][0]["request"]["template_hash"] =
            json!(novaseal_external_attestation_report_hash("public_celldep_template", &public_template));
        source_bound["cases"][1]["request"]["template_hash"] =
            json!(novaseal_external_attestation_report_hash("external_tcb_template", &external_template));
        source_bound["cases"][0]["request"]["expected_artifact_hash"] = tcb_review["runtime_artifact"]["artifact_hash"].clone();
        source_bound["cases"][0]["request"]["template_artifact_hash"] = tcb_review["runtime_artifact"]["artifact_hash"].clone();
        source_bound["cases"][0]["request"]["expected_release_manifest_commit"] = tcb_review["repo_commit"].clone();
        source_bound["cases"][1]["request"]["expected_artifact_hash"] = tcb_review["runtime_artifact"]["artifact_hash"].clone();
        source_bound["cases"][1]["request"]["template_artifact_hash"] = tcb_review["runtime_artifact"]["artifact_hash"].clone();
        source_bound["cases"][1]["request"]["expected_source_tree_sha256"] =
            tcb_review["source_inventory"]["source_tree_sha256"].clone();
        source_bound["cases"][1]["request"]["template_source_tree_sha256"] =
            tcb_review["source_inventory"]["source_tree_sha256"].clone();
        let source_bound_valid = validate_external_attestation_adapter_detail_with_sources(
            &source_bound,
            Some(&tcb_review),
            Some(&public_template),
            Some(&external_template),
        );
        assert_eq!(json_pointer_str(&source_bound_valid, "/status"), Some("passed"));
        assert!(json_pointer_bool(&source_bound_valid, "/checks/source_tcb_review_hash_matches_current_report"));
        assert!(json_pointer_bool(
            &source_bound_valid,
            "/cases/public_shared_cell_dep_attestation/expected_artifact_hash_matches_current_tcb"
        ));
        assert!(json_pointer_bool(
            &source_bound_valid,
            "/cases/public_shared_cell_dep_attestation/template_artifact_hash_matches_current_tcb"
        ));
        assert!(json_pointer_bool(
            &source_bound_valid,
            "/cases/public_shared_cell_dep_attestation/expected_release_manifest_commit_matches_current_tcb"
        ));
        assert!(json_pointer_bool(
            &source_bound_valid,
            "/cases/external_bip340_tcb_review_attestation/expected_source_tree_sha256_matches_current_tcb"
        ));

        let mut stale_source_hash = source_bound.clone();
        stale_source_hash["source_tcb_review_hash"] = json!(test_hex32(0xf1));
        let failed_stale_source_hash = validate_external_attestation_adapter_detail_with_sources(
            &stale_source_hash,
            Some(&tcb_review),
            Some(&public_template),
            Some(&external_template),
        );
        assert_eq!(json_pointer_str(&failed_stale_source_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_stale_source_hash, "/checks/source_tcb_review_hash_matches_current_report"));

        let mut stale_template_hash = source_bound.clone();
        stale_template_hash["cases"][0]["request"]["template_hash"] = json!(test_hex32(0xf2));
        let failed_stale_template_hash = validate_external_attestation_adapter_detail_with_sources(
            &stale_template_hash,
            Some(&tcb_review),
            Some(&public_template),
            Some(&external_template),
        );
        assert_eq!(json_pointer_str(&failed_stale_template_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_template_hash,
            "/cases/public_shared_cell_dep_attestation/template_hash_matches_current_template"
        ));

        let mut stale_expected_artifact_hash = source_bound.clone();
        stale_expected_artifact_hash["cases"][0]["request"]["expected_artifact_hash"] = json!(test_hex32(0xf3));
        let failed_stale_expected_artifact_hash = validate_external_attestation_adapter_detail_with_sources(
            &stale_expected_artifact_hash,
            Some(&tcb_review),
            Some(&public_template),
            Some(&external_template),
        );
        assert_eq!(json_pointer_str(&failed_stale_expected_artifact_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_expected_artifact_hash,
            "/cases/public_shared_cell_dep_attestation/expected_artifact_hash_matches_current_tcb"
        ));

        let mut stale_manifest_commit = source_bound.clone();
        stale_manifest_commit["cases"][0]["request"]["expected_release_manifest_commit"] =
            json!("fedcba9876543210fedcba9876543210fedcba98");
        let failed_stale_manifest_commit = validate_external_attestation_adapter_detail_with_sources(
            &stale_manifest_commit,
            Some(&tcb_review),
            Some(&public_template),
            Some(&external_template),
        );
        assert_eq!(json_pointer_str(&failed_stale_manifest_commit, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_manifest_commit,
            "/cases/public_shared_cell_dep_attestation/expected_release_manifest_commit_matches_current_tcb"
        ));

        let mut stale_expected_source_tree = source_bound.clone();
        stale_expected_source_tree["cases"][1]["request"]["expected_source_tree_sha256"] = json!(test_hex32(0xf4));
        let failed_stale_expected_source_tree = validate_external_attestation_adapter_detail_with_sources(
            &stale_expected_source_tree,
            Some(&tcb_review),
            Some(&public_template),
            Some(&external_template),
        );
        assert_eq!(json_pointer_str(&failed_stale_expected_source_tree, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_expected_source_tree,
            "/cases/external_bip340_tcb_review_attestation/expected_source_tree_sha256_matches_current_tcb"
        ));

        let mut unexpected_public_field = report;
        let mut extended_public_fields = full_public_fields.to_vec();
        extended_public_fields.push("unexpected.shadow_field");
        unexpected_public_field["cases"][0]["request"]["required_public_fields"] = json!(extended_public_fields);
        let failed_exact = validate_external_attestation_adapter_detail(&unexpected_public_field);
        assert_eq!(json_pointer_str(&failed_exact, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_exact, "/cases/public_shared_cell_dep_attestation/required_fields_exact"));
    }

    #[test]
    fn btc_spv_adapter_requires_exact_public_field_contract() {
        let full_public_fields = EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS;
        let report = json!({
            "schema": "novaseal-btc-spv-evidence-adapter-v0.1",
            "status": "passed",
            "adapter_status": "request_ready_external_evidence_required",
            "source_service_builder_report_hash": format!("0x{}", "aa".repeat(32)),
            "source_public_btc_spv_template_hash": format!("0x{}", "bb".repeat(32)),
            "production_output": PUBLIC_BTC_SPV_EVIDENCE,
            "summary": { "total": EXPECTED_BTC_SPV_EVIDENCE_PROFILES.len(), "matched": EXPECTED_BTC_SPV_EVIDENCE_PROFILES.len() },
            "cases": EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().enumerate().map(|(index, profile)| {
                let profile = *profile;
                let btc_material = test_btc_profile_material(profile, index as u8 + 0x60);
                let mut request = json!({
                    "profile": profile,
                    "scenario": expected_btc_spv_scenario(profile).unwrap(),
                    "minimum_confirmations": 6,
                    "required_external_inputs": ["public_btc_spv_evidence"],
                    "ckb_live_tx_hash": test_hex32(index as u8 + 0x70),
                    "live_report_hash": test_hex32(index as u8 + 0x80),
                    "service_builder_case_hash": format!("0x{}", "cc".repeat(32)),
                    "service_builder_tx_skeleton_hash": format!("0x{}", "dd".repeat(32)),
                    "service_builder_receipt_binding_hash": format!("0x{}", "ee".repeat(32)),
                    "ckb_btc_commitment_hash": format!("0x{}", "ab".repeat(32)),
                    "template_case_hash": format!("0x{}", "ff".repeat(32)),
                    "required_public_fields": full_public_fields,
                    "field_constraints": constraint_object(EXPECTED_BTC_SPV_FIELD_CONSTRAINTS),
                });
                add_expected_btc_request_fields(&mut request, &btc_material);
                json!({
                    "profile": profile,
                    "status": "passed",
                    "checks": { "ok": true },
                    "request": request,
                })
            }).collect::<Vec<_>>(),
        });

        let valid = validate_btc_spv_evidence_adapter_detail(&report);
        assert_eq!(json_pointer_str(&valid, "/status"), Some("passed"));
        assert!(json_pointer_bool(&valid, "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"));
        assert!(json_pointer_bool(&valid, "/cases/btc-transaction-commitment-profile-v0/scenario_matches_expected"));

        let mut wrong_anchor_source = report.clone();
        wrong_anchor_source["cases"][0]["request"]["expected_anchor_source"] = json!("external_public_btc_spend");
        let failed_anchor_source = validate_btc_spv_evidence_adapter_detail(&wrong_anchor_source);
        assert_eq!(json_pointer_str(&failed_anchor_source, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_anchor_source,
            "/cases/btc-transaction-commitment-profile-v0/expected_anchor_source_production_eligible"
        ));

        let mut fixture_local_anchor_source = report.clone();
        fixture_local_anchor_source["cases"][0]["request"]["local_anchor_source"] = json!("local_deterministic_fixture");
        fixture_local_anchor_source["cases"][0]["request"]["expected_anchor_source"] = json!("external_public_btc_transaction");
        let valid_fixture_local_anchor_source = validate_btc_spv_evidence_adapter_detail(&fixture_local_anchor_source);
        assert_eq!(json_pointer_str(&valid_fixture_local_anchor_source, "/status"), Some("passed"));
        assert!(json_pointer_bool(
            &valid_fixture_local_anchor_source,
            "/cases/btc-transaction-commitment-profile-v0/expected_anchor_source_production_eligible"
        ));
        assert!(json_pointer_bool(
            &valid_fixture_local_anchor_source,
            "/cases/btc-transaction-commitment-profile-v0/local_anchor_source_present"
        ));

        let mut missing_local_anchor_source = report.clone();
        missing_local_anchor_source["cases"][0]["request"].as_object_mut().unwrap().remove("local_anchor_source");
        let failed_missing_local_anchor_source = validate_btc_spv_evidence_adapter_detail(&missing_local_anchor_source);
        assert_eq!(json_pointer_str(&failed_missing_local_anchor_source, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_missing_local_anchor_source,
            "/cases/btc-transaction-commitment-profile-v0/local_anchor_source_present"
        ));

        let mut local_anchor_source = report.clone();
        local_anchor_source["cases"][0]["request"]["expected_anchor_source"] = json!("local_deterministic_fixture");
        local_anchor_source["cases"][0]["request"]["local_anchor_source"] = json!("local_deterministic_fixture");
        let failed_local_anchor_source = validate_btc_spv_evidence_adapter_detail(&local_anchor_source);
        assert_eq!(json_pointer_str(&failed_local_anchor_source, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_local_anchor_source,
            "/cases/btc-transaction-commitment-profile-v0/expected_anchor_source_production_eligible"
        ));

        let mut zero_btc_amount = report.clone();
        zero_btc_amount["cases"][0]["request"]["expected_btc_amount_sats"] = json!(0);
        let failed_zero_btc_amount = validate_btc_spv_evidence_adapter_detail(&zero_btc_amount);
        assert_eq!(json_pointer_str(&failed_zero_btc_amount, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_zero_btc_amount,
            "/cases/btc-transaction-commitment-profile-v0/expected_output_fields_present"
        ));

        let mut missing_constraint = report.clone();
        missing_constraint["cases"][0]["request"]["field_constraints"].as_object_mut().unwrap().remove("source_service.commit");
        let failed_constraint = validate_btc_spv_evidence_adapter_detail(&missing_constraint);
        assert_eq!(json_pointer_str(&failed_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_constraint, "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"));

        let mut missing_report_hash_constraint = report.clone();
        missing_report_hash_constraint["cases"][0]["request"]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("source_service.report_hash");
        let failed_report_hash_constraint = validate_btc_spv_evidence_adapter_detail(&missing_report_hash_constraint);
        assert_eq!(json_pointer_str(&failed_report_hash_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_report_hash_constraint,
            "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"
        ));

        let mut missing_celldep_hash_type_constraint = report.clone();
        missing_celldep_hash_type_constraint["cases"][0]["request"]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("spv_client_cell_dep.hash_type");
        let failed_celldep_hash_type_constraint = validate_btc_spv_evidence_adapter_detail(&missing_celldep_hash_type_constraint);
        assert_eq!(json_pointer_str(&failed_celldep_hash_type_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_celldep_hash_type_constraint,
            "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"
        ));

        let mut missing_spv_proof_hash_constraint = report.clone();
        missing_spv_proof_hash_constraint["cases"][0]["request"]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("spv_proof_hash");
        let failed_spv_proof_hash_constraint = validate_btc_spv_evidence_adapter_detail(&missing_spv_proof_hash_constraint);
        assert_eq!(json_pointer_str(&failed_spv_proof_hash_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_spv_proof_hash_constraint,
            "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"
        ));

        let mut missing_handoff_bundle_constraint = report.clone();
        missing_handoff_bundle_constraint["cases"][0]["request"]["field_constraints"]
            .as_object_mut()
            .unwrap()
            .remove("request_handoff.bundle");
        let failed_handoff_bundle_constraint = validate_btc_spv_evidence_adapter_detail(&missing_handoff_bundle_constraint);
        assert_eq!(json_pointer_str(&failed_handoff_bundle_constraint, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_handoff_bundle_constraint,
            "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"
        ));

        let mut stale_scenario = report.clone();
        stale_scenario["cases"][0]["request"]["scenario"] = json!("generic-public-btc-proof");
        let failed_scenario = validate_btc_spv_evidence_adapter_detail(&stale_scenario);
        assert_eq!(json_pointer_str(&failed_scenario, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_scenario, "/cases/btc-transaction-commitment-profile-v0/scenario_matches_expected"));

        let mut source_bound = report.clone();
        let service_builder_cases = source_bound["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                let request = &case["request"];
                json!({
                    "profile": case["profile"].as_str().unwrap(),
                    "response": {
                        "tx_skeleton_hash": request["service_builder_tx_skeleton_hash"].clone(),
                        "receipt_binding_hash": request["service_builder_receipt_binding_hash"].clone(),
                    },
                    "request": {
                        "required_live_inputs": {
                            "live_devnet_tx_hash": request["ckb_live_tx_hash"].clone(),
                            "live_report_hash": request["live_report_hash"].clone(),
                            "public_btc_anchor": {
                                "ckb_btc_commitment_hash": request["ckb_btc_commitment_hash"].clone(),
                            },
                        },
                    },
                })
            })
            .collect::<Vec<_>>();
        let service_builder = json!({ "cases": service_builder_cases });
        for case in source_bound["cases"].as_array_mut().unwrap() {
            let profile = case["profile"].as_str().unwrap();
            let builder_case = service_builder["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|builder_case| json_pointer_str(builder_case, "/profile") == Some(profile))
                .unwrap();
            case["request"]["service_builder_case_hash"] =
                json!(novaseal_btc_spv_adapter_report_hash("service_builder_case", builder_case));
        }
        let public_template = json!({ "schema": "novaseal-public-btc-spv-evidence-template-test" });
        source_bound["source_service_builder_report_hash"] =
            json!(novaseal_btc_spv_adapter_report_hash("service_builder_report", &service_builder));
        source_bound["source_public_btc_spv_template_hash"] =
            json!(novaseal_btc_spv_adapter_report_hash("public_btc_spv_template", &public_template));
        let source_bound_valid =
            validate_btc_spv_evidence_adapter_detail_with_sources(&source_bound, Some(&service_builder), Some(&public_template));
        assert_eq!(json_pointer_str(&source_bound_valid, "/status"), Some("passed"));
        assert!(json_pointer_bool(&source_bound_valid, "/checks/service_builder_report_hash_matches_current_report"));
        assert!(json_pointer_bool(
            &source_bound_valid,
            "/cases/btc-transaction-commitment-profile-v0/service_builder_case_hash_matches_current_report"
        ));

        let mut stale_source_hash = source_bound.clone();
        stale_source_hash["source_service_builder_report_hash"] = json!(test_hex32(0xf3));
        let failed_stale_source_hash =
            validate_btc_spv_evidence_adapter_detail_with_sources(&stale_source_hash, Some(&service_builder), Some(&public_template));
        assert_eq!(json_pointer_str(&failed_stale_source_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_stale_source_hash, "/checks/service_builder_report_hash_matches_current_report"));

        let mut stale_case_hash = source_bound.clone();
        stale_case_hash["cases"][0]["request"]["service_builder_case_hash"] = json!(test_hex32(0xf4));
        let failed_stale_case_hash =
            validate_btc_spv_evidence_adapter_detail_with_sources(&stale_case_hash, Some(&service_builder), Some(&public_template));
        assert_eq!(json_pointer_str(&failed_stale_case_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_case_hash,
            "/cases/btc-transaction-commitment-profile-v0/service_builder_case_hash_matches_current_report"
        ));

        let mut stale_live_hash = source_bound.clone();
        stale_live_hash["cases"][0]["request"]["live_report_hash"] = json!(test_hex32(0xf5));
        let failed_stale_live_hash =
            validate_btc_spv_evidence_adapter_detail_with_sources(&stale_live_hash, Some(&service_builder), Some(&public_template));
        assert_eq!(json_pointer_str(&failed_stale_live_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_live_hash,
            "/cases/btc-transaction-commitment-profile-v0/live_report_hash_matches_current_report"
        ));

        let mut unexpected_public_field = report;
        let mut extended_public_fields = full_public_fields.to_vec();
        extended_public_fields.push("unexpected.shadow_field");
        unexpected_public_field["cases"][0]["request"]["required_public_fields"] = json!(extended_public_fields);
        let failed_exact = validate_btc_spv_evidence_adapter_detail(&unexpected_public_field);
        assert_eq!(json_pointer_str(&failed_exact, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_exact, "/cases/btc-transaction-commitment-profile-v0/required_public_fields_exact"));
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
        let rwa_root = temp.path().join(RWA_RECEIPT_ROOT);
        let rwa_src = rwa_root.join("src");
        let rwa_schemas = rwa_root.join("schemas");
        let rwa_fixtures = rwa_root.join("fixtures");
        let rwa_proofs = rwa_root.join("proofs");
        std::fs::create_dir_all(&rwa_src).unwrap();
        std::fs::create_dir_all(&rwa_schemas).unwrap();
        std::fs::create_dir_all(&rwa_fixtures).unwrap();
        std::fs::create_dir_all(&rwa_proofs).unwrap();
        std::fs::write(rwa_root.join("Cell.toml"), "profile = \"rwa-receipt-profile-v0\"\n").unwrap();
        std::fs::write(rwa_src.join("nova_rwa_receipt_type.cell"), "action materialize_rwa_receipt() {}\n").unwrap();
        std::fs::write(rwa_src.join("nova_rwa_receipt_lifecycle_type.cell"), "action nova_rwa_receipt_lifecycle() {}\n").unwrap();
        std::fs::write(rwa_schemas.join("nova_rwa_receipt_cell_v0.schema"), "cell: Byte32\n").unwrap();
        std::fs::write(rwa_fixtures.join("materialize_valid.json"), "{}\n").unwrap();
        std::fs::write(rwa_proofs.join("invariant_matrix.json"), "{}\n").unwrap();
        let artifact_hash = format!("0x{}", "aa".repeat(32));
        let tcb_source_tree_hash = format!("0x{}", "bb".repeat(32));
        let rwa_profile_source_hash = source_tree_hash(temp.path(), RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)
            .unwrap()
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
                "status": "attested",
                "network": "testnet",
                "attested_at": "YYYY-MM-DDTHH:MM:SSZ",
                "attestor": "REPLACE_WITH_DEPLOYER_OR_RELEASE_SIGNER",
                "release": {
                    "package": "novaseal",
                    "version": "0.0.1-v0-mvp",
                    "manifest_commit": "0123456789abcdef0123456789abcdef01234567",
                },
                "notes": "template fixture",
                "request_handoff": {
                    "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                    "bundle_hash": format!("0x{}", "11".repeat(32)),
                    "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                    "group": "public_shared_cell_dep_attestation",
                },
                "runtime_verifier": {
                    "verifier_id": "btc.bip340.v0",
                    "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                    "artifact_hash": artifact_hash,
                    "out_point": format!("0x{}:0", "22".repeat(32)),
                    "data_hash": format!("0x{}", "33".repeat(32)),
                    "dep_type": "code",
                    "hash_type": "data1",
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
                "artifact_hash_algorithm": "sha256",
                "source_tree_sha256": tcb_source_tree_hash,
                "reviewer": "REPLACE_WITH_EXTERNAL_REVIEWER",
                "review_date": "YYYY-MM-DD",
                "review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                "report_uri": "REPLACE_WITH_EXTERNAL_REVIEW_REPORT_OR_COMMIT_URI",
                "notes": "template fixture",
                "request_handoff": {
                    "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                    "bundle_hash": format!("0x{}", "44".repeat(32)),
                    "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                    "group": "external_bip340_tcb_review_attestation",
                },
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            proofs.join("public_btc_spv_evidence.template.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": "novaseal-public-btc-spv-evidence-v0.1",
                "status": "template",
                "network": "testnet-or-mainnet",
                "generated_at": "YYYY-MM-DDTHH:MM:SSZ",
                "evidence_provider": "REPLACE_WITH_EXTERNAL_SPV_OPERATOR_OR_SERVICE",
                "request_handoff": {
                    "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                    "bundle_hash": format!("0x{}", "55".repeat(32)),
                    "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                    "group": "public_btc_spv_evidence",
                },
                "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
                "cases": [],
                "notes": ["template fixture"],
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            rwa_proofs.join("legal_registry_review_evidence.template.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": "novaseal-rwa-legal-registry-review-evidence-v0.1",
                "status": "accepted",
                "profile": EXPECTED_RWA_RECEIPT_PROFILE,
                "reviewer": "REPLACE_WITH_EXTERNAL_LEGAL_OR_REGISTRY_REVIEWER",
                "review_date": "YYYY-MM-DD",
                "review_scope": EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
                "registry": {
                    "authority": "REPLACE_WITH_REAL_REGISTRY_OR_CUSTODIAN_AUTHORITY",
                    "jurisdiction": "REPLACE_WITH_REAL_WORLD_JURISDICTION",
                    "registry_report_hash": format!("0x{}", "66".repeat(32)),
                },
                "profile_source_tree_sha256": rwa_profile_source_hash,
                "report_uri": "REPLACE_WITH_EXTERNAL_LEGAL_OR_REGISTRY_REVIEW_REPORT_URI",
                "request_handoff": {
                    "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                    "bundle_hash": format!("0x{}", "77".repeat(32)),
                    "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                    "group": "rwa_legal_registry_review_evidence",
                },
                "notes": ["template fixture"],
            }))
            .unwrap(),
        )
        .unwrap();

        let passed =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        let failed = validate_attestation_templates(
            temp.path(),
            Some(&format!("0x{}", "cc".repeat(32))),
            Some("sha256"),
            Some(&tcb_source_tree_hash),
        )
        .unwrap();

        assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
        assert!(json_pointer_bool(&passed, "/checks/public_dep_type"));
        assert!(json_pointer_bool(&passed, "/checks/public_hash_type"));
        assert!(json_pointer_bool(&passed, "/checks/public_hash_type_matches_expected"));
        assert!(json_pointer_bool(&passed, "/checks/public_release_version_current"));
        assert!(json_pointer_bool(&passed, "/checks/public_request_handoff_bundle_path"));
        assert!(json_pointer_bool(&passed, "/checks/public_request_handoff_group"));
        assert!(json_pointer_bool(&passed, "/checks/external_request_handoff_bundle_path"));
        assert!(json_pointer_bool(&passed, "/checks/external_request_handoff_group"));
        assert!(json_pointer_bool(&passed, "/checks/btc_spv_request_handoff_bundle_path"));
        assert!(json_pointer_bool(&passed, "/checks/btc_spv_request_handoff_group"));
        assert!(json_pointer_bool(&passed, "/checks/rwa_legal_request_handoff_bundle_path"));
        assert!(json_pointer_bool(&passed, "/checks/rwa_legal_request_handoff_group"));
        assert!(json_pointer_bool(&passed, "/checks/rwa_legal_profile_source_tree_hash_matches_current"));
        assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed, "/checks/public_artifact_hash_matches_current_tcb"));
        assert!(!json_pointer_bool(&failed, "/checks/external_artifact_hash_matches_current_tcb"));

        let mut drifted_public_template =
            json_load_path(temp.path(), &proofs.join("public_shared_cell_dep_attestation.template.json")).unwrap();
        drifted_public_template["release"]["version"] = json!("0.0.2");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
        )
        .unwrap();
        let failed_release_version =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        assert_eq!(json_pointer_str(&failed_release_version, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_release_version, "/checks/public_release_version_current"));

        drifted_public_template["release"]["version"] = json!(EXPECTED_NOVASEAL_RELEASE_VERSION);
        drifted_public_template["runtime_verifier"]["unexpected_template_field"] = Value::String("must-fail".to_string());
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
        )
        .unwrap();
        let failed_shape =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        assert_eq!(json_pointer_str(&failed_shape, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_shape, "/checks/public_runtime_verifier_fields_exact"));

        drifted_public_template["runtime_verifier"].as_object_mut().unwrap().remove("unexpected_template_field");
        drifted_public_template["runtime_verifier"]["dep_type"] = json!("dep_group");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
        )
        .unwrap();
        let failed_dep_type =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        assert_eq!(json_pointer_str(&failed_dep_type, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_dep_type, "/checks/public_dep_type"));

        drifted_public_template["runtime_verifier"]["dep_type"] = json!("code");
        drifted_public_template["runtime_verifier"]["hash_type"] = json!("type");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
        )
        .unwrap();
        let failed_stale_hash_type =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_hash_type, "/status"), Some("failed"));
        assert!(json_pointer_bool(&failed_stale_hash_type, "/checks/public_hash_type"));
        assert!(!json_pointer_bool(&failed_stale_hash_type, "/checks/public_hash_type_matches_expected"));

        drifted_public_template["runtime_verifier"]["hash_type"] = json!("invalid-hash-type");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
        )
        .unwrap();
        let failed_hash_type =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        assert_eq!(json_pointer_str(&failed_hash_type, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_hash_type, "/checks/public_hash_type"));

        drifted_public_template["runtime_verifier"]["hash_type"] = json!("data1");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
        )
        .unwrap();
        let failed_algorithm =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("ckb-blake2b256"), Some(&tcb_source_tree_hash))
                .unwrap();
        assert_eq!(json_pointer_str(&failed_algorithm, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_algorithm, "/checks/external_artifact_hash_algorithm_matches_current_tcb"));

        let mut public_wrong_handoff_bundle =
            json_load_path(temp.path(), &proofs.join("public_shared_cell_dep_attestation.template.json")).unwrap();
        public_wrong_handoff_bundle["request_handoff"]["bundle"] = json!("target/stale-handoff.json");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&public_wrong_handoff_bundle).unwrap(),
        )
        .unwrap();
        let failed_public_handoff_bundle =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        assert_eq!(json_pointer_str(&failed_public_handoff_bundle, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_public_handoff_bundle, "/checks/public_request_handoff_bundle_path"));

        public_wrong_handoff_bundle["request_handoff"]["bundle"] = json!(EXTERNAL_EVIDENCE_HANDOFF);
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.template.json"),
            serde_json::to_vec_pretty(&public_wrong_handoff_bundle).unwrap(),
        )
        .unwrap();

        let mut external_wrong_handoff_group =
            json_load_path(temp.path(), &proofs.join("bip340_external_tcb_review_attestation.template.json")).unwrap();
        external_wrong_handoff_group["request_handoff"]["group"] = json!("public_shared_cell_dep_attestation");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.template.json"),
            serde_json::to_vec_pretty(&external_wrong_handoff_group).unwrap(),
        )
        .unwrap();
        let failed_external_handoff_group =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        assert_eq!(json_pointer_str(&failed_external_handoff_group, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_external_handoff_group, "/checks/external_request_handoff_group"));

        external_wrong_handoff_group["request_handoff"]["group"] = json!("external_bip340_tcb_review_attestation");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.template.json"),
            serde_json::to_vec_pretty(&external_wrong_handoff_group).unwrap(),
        )
        .unwrap();

        let mut btc_wrong_handoff_group = json_load_path(temp.path(), &proofs.join("public_btc_spv_evidence.template.json")).unwrap();
        btc_wrong_handoff_group["request_handoff"]["group"] = json!("external_bip340_tcb_review_attestation");
        std::fs::write(
            proofs.join("public_btc_spv_evidence.template.json"),
            serde_json::to_vec_pretty(&btc_wrong_handoff_group).unwrap(),
        )
        .unwrap();
        let failed_btc_handoff_group =
            validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
        assert_eq!(json_pointer_str(&failed_btc_handoff_group, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_btc_handoff_group, "/checks/btc_spv_request_handoff_group"));
    }

    #[test]
    fn rwa_legal_registry_review_requires_current_external_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let rwa_root = temp.path().join(RWA_RECEIPT_ROOT);
        let rwa_src = rwa_root.join("src");
        let rwa_schemas = rwa_root.join("schemas");
        let rwa_fixtures = rwa_root.join("fixtures");
        let rwa_proofs = rwa_root.join("proofs");
        std::fs::create_dir_all(&rwa_src).unwrap();
        std::fs::create_dir_all(&rwa_schemas).unwrap();
        std::fs::create_dir_all(&rwa_fixtures).unwrap();
        std::fs::create_dir_all(&rwa_proofs).unwrap();
        std::fs::write(rwa_root.join("Cell.toml"), format!("profile = \"{}\"\n", EXPECTED_RWA_RECEIPT_PROFILE)).unwrap();
        std::fs::write(rwa_src.join("nova_rwa_receipt_type.cell"), "action materialize_rwa_receipt() {}\n").unwrap();
        std::fs::write(rwa_src.join("nova_rwa_receipt_lifecycle_type.cell"), "action nova_rwa_receipt_lifecycle() {}\n").unwrap();
        std::fs::write(rwa_schemas.join("nova_rwa_receipt_cell_v0.schema"), "cell: Byte32\n").unwrap();
        std::fs::write(rwa_fixtures.join("materialize_valid.json"), "{}\n").unwrap();
        std::fs::write(rwa_proofs.join("invariant_matrix.json"), "{}\n").unwrap();

        let source_hash = source_tree_hash(temp.path(), RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS).unwrap();
        let source_sha256 = json_pointer_str(&source_hash, "/sha256").unwrap().to_string();
        let handoff = json!({
            "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
            "status": "passed",
            "cases": [
                {
                    "group": "rwa_legal_registry_review_evidence",
                    "expected_values": {
                        "profile": EXPECTED_RWA_RECEIPT_PROFILE,
                        "profile_source_tree_sha256": source_sha256,
                        "review_scope": EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
                    },
                },
            ],
        });
        let handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &handoff);
        let evidence_path = temp.path().join(RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE);
        let evidence = json!({
            "schema": "novaseal-rwa-legal-registry-review-evidence-v0.1",
            "status": "accepted",
            "profile": EXPECTED_RWA_RECEIPT_PROFILE,
            "reviewer": "NervosRegistryReviewLtd",
            "review_date": "2026-06-04",
            "review_scope": EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
            "registry": {
                "authority": "NervosCustodyRegistry",
                "jurisdiction": "England-and-Wales",
                "registry_report_hash": format!("0x{}", "66".repeat(32)),
            },
            "profile_source_tree_sha256": source_sha256,
            "report_uri": "https://audits.nervos.org/novaseal-rwa-legal-registry-review",
            "request_handoff": {
                "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                "bundle_hash": handoff_hash,
                "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                "group": "rwa_legal_registry_review_evidence",
            },
            "notes": "external RWA legal registry review fixture",
        });
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();

        let passed = validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
        assert!(json_pointer_bool(&passed, "/checks/profile_source_tree_sha256_matches_current"));
        assert!(json_pointer_bool(&passed, "/checks/profile_source_tree_sha256_matches_handoff"));
        assert!(json_pointer_bool(&passed, "/checks/review_scope_matches_handoff"));
        assert!(json_pointer_bool(&passed, "/checks/request_handoff_bundle_hash_matches_current"));

        let mut stale_handoff_scope = handoff.clone();
        stale_handoff_scope["cases"][0]["expected_values"]["review_scope"] = json!(["RWA receipt legal title boundary"]);
        let stale_handoff_scope_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_handoff_scope);
        let mut stale_handoff_scope_evidence = evidence.clone();
        stale_handoff_scope_evidence["request_handoff"]["bundle_hash"] = json!(stale_handoff_scope_hash);
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&stale_handoff_scope_evidence).unwrap()).unwrap();
        let failed_stale_handoff_scope =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &stale_handoff_scope).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_handoff_scope, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_stale_handoff_scope, "/checks/review_scope_matches_handoff"));

        let mut stale_source = evidence.clone();
        stale_source["profile_source_tree_sha256"] = json!(format!("0x{}", "77".repeat(32)));
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&stale_source).unwrap()).unwrap();
        let failed_stale_source =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_source, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_stale_source, "/checks/profile_source_tree_sha256_matches_current"));

        let mut placeholder_reviewer = evidence.clone();
        placeholder_reviewer["reviewer"] = json!("REPLACE_WITH_EXTERNAL_LEGAL_OR_REGISTRY_REVIEWER");
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&placeholder_reviewer).unwrap()).unwrap();
        let failed_placeholder_reviewer =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_placeholder_reviewer, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_placeholder_reviewer, "/checks/reviewer_identity"));

        let mut first_party_reviewer = evidence.clone();
        first_party_reviewer["reviewer"] = json!("NovaSeal Legal Review Team");
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&first_party_reviewer).unwrap()).unwrap();
        let failed_first_party_reviewer =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_first_party_reviewer, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_first_party_reviewer, "/checks/reviewer_identity"));

        let mut first_party_authority = evidence.clone();
        first_party_authority["registry"]["authority"] = json!("CellScript Custody Registry");
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&first_party_authority).unwrap()).unwrap();
        let failed_first_party_authority =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_first_party_authority, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_first_party_authority, "/checks/registry_authority_identity"));

        let mut local_jurisdiction = evidence.clone();
        local_jurisdiction["registry"]["jurisdiction"] = json!("local-devnet-registry");
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&local_jurisdiction).unwrap()).unwrap();
        let failed_local_jurisdiction =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_local_jurisdiction, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_local_jurisdiction, "/checks/registry_jurisdiction_present"));

        let mut placeholder_registry_hash = evidence.clone();
        placeholder_registry_hash["registry"]["registry_report_hash"] = json!(format!("0x{}", "00".repeat(32)));
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&placeholder_registry_hash).unwrap()).unwrap();
        let failed_placeholder_registry_hash =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_placeholder_registry_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_placeholder_registry_hash, "/checks/registry_report_hash_non_placeholder"));

        let mut private_report_uri = evidence.clone();
        private_report_uri["report_uri"] = json!("https://10.0.0.7/novaseal-rwa-legal-registry-review");
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&private_report_uri).unwrap()).unwrap();
        let failed_private_report_uri =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_private_report_uri, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_private_report_uri, "/checks/report_uri_https"));

        let mut stale_handoff_hash = evidence.clone();
        stale_handoff_hash["request_handoff"]["bundle_hash"] = json!(format!("0x{}", "88".repeat(32)));
        std::fs::write(&evidence_path, serde_json::to_vec_pretty(&stale_handoff_hash).unwrap()).unwrap();
        let failed_stale_handoff_hash =
            validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_handoff_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_stale_handoff_hash, "/checks/request_handoff_bundle_hash_matches_current"));
    }

    #[test]
    fn btc_spv_evidence_requires_public_complete_profile_cases() {
        let temp = tempfile::tempdir().unwrap();
        let proofs = temp.path().join("proposals/novaseal/v0-mvp-skeleton/proofs");
        std::fs::create_dir_all(&proofs).unwrap();
        let expected_scenarios = EXPECTED_BTC_SPV_PROFILE_SCENARIOS
            .iter()
            .map(|(profile, scenario)| ((*profile).to_string(), json!(*scenario)))
            .collect::<Map<_, _>>();
        let expected_case_bindings = EXPECTED_BTC_SPV_EVIDENCE_PROFILES
            .iter()
            .enumerate()
            .map(|(index, profile)| {
                let byte = index as u8 + 0x77;
                let btc_material = test_btc_profile_material(profile, byte);
                let mut binding = json!({
                    "ckb_live_tx_hash": test_hex32(byte),
                    "live_report_hash": test_hex32(byte + 1),
                    "service_builder_case_hash": test_hex32(byte + 2),
                    "service_builder_tx_skeleton_hash": test_hex32(byte + 3),
                    "service_builder_receipt_binding_hash": test_hex32(byte + 4),
                    "ckb_btc_commitment_hash": test_hex32(byte + 5),
                });
                merge_expected_btc_binding_fields(&mut binding, &btc_material);
                ((*profile).to_string(), binding)
            })
            .collect::<Map<_, _>>();
        let handoff = json!({
            "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
            "status": "passed",
            "cases": [
                {
                    "group": "public_btc_spv_evidence",
                    "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
                    "expected_scenarios": expected_scenarios,
                    "expected_case_bindings": expected_case_bindings,
                },
            ],
        });
        let handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &handoff);

        let missing = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&missing, "/status"), Some("external_required"));

        let case_for = |profile: &str| {
            let index = EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().position(|expected| *expected == profile).unwrap_or(9);
            let byte = index as u8 + 0x77;
            let btc_material = test_btc_profile_material(profile, byte);
            let spv_material = test_btc_spv_material(byte, 7, &btc_material);
            json!({
                "profile": profile,
                "scenario": expected_btc_spv_scenario(profile).unwrap_or("unexpected-profile-scenario"),
                "ckb_live_tx_hash": test_hex32(byte),
                "live_report_hash": test_hex32(byte + 1),
                "service_builder_case_hash": test_hex32(byte + 2),
                "service_builder_tx_skeleton_hash": test_hex32(byte + 3),
                "service_builder_receipt_binding_hash": test_hex32(byte + 4),
                "ckb_btc_commitment_hash": test_hex32(byte + 5),
                "btc_txid": spv_material["btc_txid"].clone(),
                "btc_wtxid": spv_material["btc_wtxid"].clone(),
                "btc_tx_hex": spv_material["btc_tx_hex"].clone(),
                "btc_transaction_binding": spv_material["btc_transaction_binding"].clone(),
                "btc_block_hash": spv_material["btc_block_hash"].clone(),
                "btc_block_header": spv_material["btc_block_header"].clone(),
                "btc_merkle_proof": spv_material["btc_merkle_proof"].clone(),
                "spv_proof_hash": spv_material["spv_proof_hash"].clone(),
                "minimum_confirmations": 6,
                "confirmations": 7,
                "spv_client_cell_dep": {
                    "out_point": format!("0x{}:0", "44".repeat(32)),
                    "dep_type": "code",
                    "hash_type": "type",
                    "data_hash": format!("0x{}", "55".repeat(32)),
                },
                "source_service": {
                    "name": "rgbpp-style-spv-service",
                    "commit": "0123456789abcdef0123456789abcdef01234567",
                    "report_hash": format!("0x{}", "66".repeat(32)),
                },
            })
        };
        let spv_report = json!({
            "schema": "novaseal-public-btc-spv-evidence-v0.1",
            "status": "attested",
            "network": "testnet",
            "evidence_provider": "external-spv-operator",
            "generated_at": "2026-06-04T00:00:00Z",
            "notes": "external public BTC SPV evidence fixture",
            "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
            "request_handoff": {
                "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                "bundle_hash": handoff_hash,
                "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                "group": "public_btc_spv_evidence",
            },
            "cases": EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().map(|profile| case_for(profile)).collect::<Vec<_>>(),
        });
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&spv_report).unwrap()).unwrap();

        let passed = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
        assert!(json_pointer_bool(&passed, "/checks/top_level_fields_exact"));
        assert!(json_pointer_bool(&passed, "/checks/request_handoff_fields_exact"));
        assert!(json_pointer_bool(&passed, "/checks/request_handoff_bundle_hash_matches_current"));
        assert!(json_pointer_bool(&passed, "/checks/request_handoff_bundle_hash_algorithm"));
        assert!(json_pointer_bool(&passed, "/checks/handoff_required_profiles_exact"));
        assert!(json_pointer_bool(&passed, "/checks/handoff_expected_scenarios_exact"));
        assert!(json_pointer_bool(&passed, "/checks/handoff_expected_case_bindings_exact"));
        assert!(json_pointer_bool(&passed, "/checks/network_public"));
        assert!(json_pointer_bool(&passed, "/checks/evidence_provider_identity"));
        assert!(json_pointer_bool(&passed, "/checks/generated_at_utc_timestamp"));
        assert!(json_pointer_bool(&passed, "/checks/generated_at_not_future"));
        assert!(json_pointer_bool(&passed, "/checks/required_profiles_field_exact"));
        assert!(json_pointer_bool(&passed, "/checks/required_profiles_match_handoff"));
        assert!(json_pointer_bool(&passed, "/checks/required_profiles_covered_exact"));
        assert!(json_pointer_bool(&passed, "/checks/covered_scenarios_match_handoff"));
        assert!(json_pointer_bool(&passed, "/checks/case_checks_passed"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/scenario_matches_expected"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/scenario_matches_handoff"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/ckb_live_tx_hash_matches_handoff"));
        assert!(json_pointer_bool(
            &passed,
            "/case_checks/btc-transaction-commitment-profile-v0/ckb_btc_commitment_hash_matches_handoff"
        ));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_matches_handoff_when_bound"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_wtxid_matches_handoff_when_bound"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_matches_tx_hex"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_wtxid_matches_tx_hex"));
        assert!(json_pointer_bool(
            &passed,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_transaction_binding_matches_handoff"
        ));
        assert!(json_pointer_bool(
            &passed,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_transaction_output_matches_anchor"
        ));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-utxo-seal-profile-v0/btc_utxo_spend_input_matches_anchor"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-utxo-seal-profile-v0/btc_utxo_sealed_tx_matches_anchor"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-utxo-seal-profile-v0/btc_utxo_sealed_utxo_commitment_matches_tuple"));
        assert!(json_pointer_bool(&passed, "/case_checks/dual-seal-profile-v0/btc_dual_spend_input_matches_anchor"));
        assert!(json_pointer_bool(&passed, "/case_checks/dual-seal-profile-v0/btc_dual_sealed_tx_matches_anchor"));
        assert!(json_pointer_bool(&passed, "/case_checks/dual-seal-profile-v0/btc_dual_sealed_utxo_commitment_matches_tuple"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_block_hash_matches_header"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_root_matches_header"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_branch_verifies_txid"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_confirmations_match_heights"));
        assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/spv_proof_hash_matches_material"));
        assert!(json_pointer_bool(
            &passed,
            "/case_checks/btc-transaction-commitment-profile-v0/service_builder_tx_skeleton_hash_matches_handoff"
        ));

        let single_tx_btc = test_btc_profile_material(EXPECTED_BTC_TX_COMMITMENT_PROFILE, 0x77);
        let single_tx_material = test_btc_single_tx_block_spv_material(0x77, 7, &single_tx_btc);
        let mut single_tx_block = spv_report.clone();
        single_tx_block["cases"][0]["btc_txid"] = single_tx_material["btc_txid"].clone();
        single_tx_block["cases"][0]["btc_wtxid"] = single_tx_material["btc_wtxid"].clone();
        single_tx_block["cases"][0]["btc_tx_hex"] = single_tx_material["btc_tx_hex"].clone();
        single_tx_block["cases"][0]["btc_transaction_binding"] = single_tx_material["btc_transaction_binding"].clone();
        single_tx_block["cases"][0]["btc_block_hash"] = single_tx_material["btc_block_hash"].clone();
        single_tx_block["cases"][0]["btc_block_header"] = single_tx_material["btc_block_header"].clone();
        single_tx_block["cases"][0]["btc_merkle_proof"] = single_tx_material["btc_merkle_proof"].clone();
        single_tx_block["cases"][0]["spv_proof_hash"] = single_tx_material["spv_proof_hash"].clone();
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&single_tx_block).unwrap()).unwrap();
        let passed_single_tx_block = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&passed_single_tx_block, "/status"), Some("passed"));
        assert!(json_pointer_bool(
            &passed_single_tx_block,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_proof_branch_valid"
        ));
        assert!(json_pointer_bool(
            &passed_single_tx_block,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_branch_verifies_txid"
        ));

        let mut single_tx_nonzero_index = single_tx_block.clone();
        single_tx_nonzero_index["cases"][0]["btc_merkle_proof"]["tx_index"] = json!(1);
        single_tx_nonzero_index["cases"][0]["spv_proof_hash"] =
            json!(btc_spv_proof_material_hash(&single_tx_nonzero_index["cases"][0]).unwrap());
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&single_tx_nonzero_index).unwrap())
            .unwrap();
        let failed_single_tx_nonzero_index = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_single_tx_nonzero_index, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_single_tx_nonzero_index,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_proof_branch_valid"
        ));

        let mut stale_btc_commitment = spv_report.clone();
        stale_btc_commitment["cases"][0]["ckb_btc_commitment_hash"] = json!(test_hex32(0x19));
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_btc_commitment).unwrap())
            .unwrap();
        let failed_stale_btc_commitment = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_btc_commitment, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_btc_commitment,
            "/case_checks/btc-transaction-commitment-profile-v0/ckb_btc_commitment_hash_matches_handoff"
        ));

        let mut stale_tx_hex = spv_report.clone();
        stale_tx_hex["cases"][0]["btc_tx_hex"] = json!("0x01000000000000000000");
        let stale_tx_hex_hash = btc_spv_proof_material_hash(&stale_tx_hex["cases"][0]).unwrap();
        stale_tx_hex["cases"][0]["spv_proof_hash"] = json!(stale_tx_hex_hash);
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_tx_hex).unwrap()).unwrap();
        let failed_stale_tx_hex = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_tx_hex, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_stale_tx_hex, "/case_checks/btc-transaction-commitment-profile-v0/btc_tx_hex_valid"));

        let mut wrong_valid_tx_hex = spv_report.clone();
        let wrong_btc = test_btc_profile_material(EXPECTED_BTC_TX_COMMITMENT_PROFILE, 0x2a);
        wrong_valid_tx_hex["cases"][0]["btc_tx_hex"] = json!(wrong_btc.tx_hex);
        let wrong_valid_tx_hash = btc_spv_proof_material_hash(&wrong_valid_tx_hex["cases"][0]).unwrap();
        wrong_valid_tx_hex["cases"][0]["spv_proof_hash"] = json!(wrong_valid_tx_hash);
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&wrong_valid_tx_hex).unwrap()).unwrap();
        let failed_wrong_valid_tx_hex = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_wrong_valid_tx_hex, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_wrong_valid_tx_hex,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_matches_tx_hex"
        ));

        let mut stale_output_handoff = handoff.clone();
        stale_output_handoff["cases"][0]["expected_case_bindings"][EXPECTED_BTC_TX_COMMITMENT_PROFILE]["btc_amount_sats"] = json!(1);
        let stale_output_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_output_handoff);
        let mut stale_output_amount = spv_report.clone();
        stale_output_amount["request_handoff"]["bundle_hash"] = json!(stale_output_handoff_hash);
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_output_amount).unwrap()).unwrap();
        let failed_stale_output_amount =
            validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &stale_output_handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_output_amount, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_output_amount,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_transaction_binding_matches_handoff"
        ));
        assert!(!json_pointer_bool(
            &failed_stale_output_amount,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_transaction_output_matches_anchor"
        ));

        let mut stale_sealed_tx = spv_report.clone();
        let wrong_sealed_tx_hex = stale_sealed_tx["cases"][0]["btc_tx_hex"].clone();
        stale_sealed_tx["cases"][1]["btc_transaction_binding"]["sealed_btc_tx_hex"] = wrong_sealed_tx_hex;
        let stale_sealed_hash = btc_spv_proof_material_hash(&stale_sealed_tx["cases"][1]).unwrap();
        stale_sealed_tx["cases"][1]["spv_proof_hash"] = json!(stale_sealed_hash);
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_sealed_tx).unwrap()).unwrap();
        let failed_stale_sealed_tx = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_sealed_tx, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_sealed_tx,
            "/case_checks/btc-utxo-seal-profile-v0/btc_utxo_sealed_tx_matches_anchor"
        ));

        let mut stale_bound_btc_txid = spv_report.clone();
        let replacement_btc = test_btc_profile_material(EXPECTED_BTC_TX_COMMITMENT_PROFILE, 0x21);
        let replacement_material = test_btc_spv_material(0x21, 7, &replacement_btc);
        stale_bound_btc_txid["cases"][0]["btc_txid"] = replacement_material["btc_txid"].clone();
        stale_bound_btc_txid["cases"][0]["btc_wtxid"] = replacement_material["btc_wtxid"].clone();
        stale_bound_btc_txid["cases"][0]["btc_tx_hex"] = replacement_material["btc_tx_hex"].clone();
        stale_bound_btc_txid["cases"][0]["btc_transaction_binding"] = replacement_material["btc_transaction_binding"].clone();
        stale_bound_btc_txid["cases"][0]["btc_block_hash"] = replacement_material["btc_block_hash"].clone();
        stale_bound_btc_txid["cases"][0]["btc_block_header"] = replacement_material["btc_block_header"].clone();
        stale_bound_btc_txid["cases"][0]["btc_merkle_proof"] = replacement_material["btc_merkle_proof"].clone();
        stale_bound_btc_txid["cases"][0]["spv_proof_hash"] = replacement_material["spv_proof_hash"].clone();
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_bound_btc_txid).unwrap())
            .unwrap();
        let failed_stale_bound_btc_txid = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_bound_btc_txid, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_bound_btc_txid,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_matches_handoff_when_bound"
        ));

        let mut stale_block_header = spv_report.clone();
        stale_block_header["cases"][0]["btc_block_header"] = json!(format!("0x{}", "01".repeat(80)));
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_block_header).unwrap()).unwrap();
        let failed_stale_block_header = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_block_header, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_block_header,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_block_hash_matches_header"
        ));

        let mut stale_merkle_branch = spv_report.clone();
        stale_merkle_branch["cases"][0]["btc_merkle_proof"]["merkle_branch"][0] = json!(test_hex32(0x99));
        let stale_hash = btc_spv_proof_material_hash(&stale_merkle_branch["cases"][0]).unwrap();
        stale_merkle_branch["cases"][0]["spv_proof_hash"] = json!(stale_hash);
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_merkle_branch).unwrap()).unwrap();
        let failed_stale_merkle_branch = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_merkle_branch, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_merkle_branch,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_branch_verifies_txid"
        ));

        let mut stale_confirmation_height = spv_report.clone();
        stale_confirmation_height["cases"][0]["btc_merkle_proof"]["observed_tip_height"] = json!(900_001);
        let stale_height_hash = btc_spv_proof_material_hash(&stale_confirmation_height["cases"][0]).unwrap();
        stale_confirmation_height["cases"][0]["spv_proof_hash"] = json!(stale_height_hash);
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_confirmation_height).unwrap())
            .unwrap();
        let failed_stale_confirmation_height = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_confirmation_height, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_confirmation_height,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_confirmations_match_heights"
        ));

        let mut stale_spv_proof_hash = spv_report.clone();
        stale_spv_proof_hash["cases"][0]["spv_proof_hash"] = json!(test_hex32(0x88));
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_spv_proof_hash).unwrap())
            .unwrap();
        let failed_stale_spv_proof_hash = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_spv_proof_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_spv_proof_hash,
            "/case_checks/btc-transaction-commitment-profile-v0/spv_proof_hash_matches_material"
        ));

        let mut stale_live_binding = spv_report.clone();
        stale_live_binding["cases"][0]["ckb_live_tx_hash"] = json!(test_hex32(0x22));
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_live_binding).unwrap()).unwrap();
        let failed_stale_live_binding = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_live_binding, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_stale_live_binding,
            "/case_checks/btc-transaction-commitment-profile-v0/ckb_live_tx_hash_matches_handoff"
        ));

        let mut stale_handoff_scenario = handoff.clone();
        stale_handoff_scenario["cases"][0]["expected_scenarios"][EXPECTED_BTC_TX_COMMITMENT_PROFILE] =
            json!("generic-public-btc-proof");
        let stale_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_handoff_scenario);
        let mut spv_with_stale_handoff = spv_report.clone();
        spv_with_stale_handoff["request_handoff"]["bundle_hash"] = json!(stale_handoff_hash);
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&spv_with_stale_handoff).unwrap())
            .unwrap();
        let failed_stale_handoff = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &stale_handoff_scenario).unwrap();
        assert_eq!(json_pointer_str(&failed_stale_handoff, "/status"), Some("failed"));
        assert!(json_pointer_bool(&failed_stale_handoff, "/checks/request_handoff_bundle_hash_matches_current"));
        assert!(!json_pointer_bool(
            &failed_stale_handoff,
            "/case_checks/btc-transaction-commitment-profile-v0/scenario_matches_handoff"
        ));
        assert!(!json_pointer_bool(&failed_stale_handoff, "/checks/covered_scenarios_match_handoff"));

        let mut placeholder_generated_at = spv_report.clone();
        placeholder_generated_at["generated_at"] = json!("YYYY-MM-DDTHH:MM:SSZ");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&placeholder_generated_at).unwrap())
            .unwrap();
        let failed_generated_at = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_generated_at, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_generated_at, "/checks/generated_at_utc_timestamp"));

        let mut future_generated_at = spv_report.clone();
        future_generated_at["generated_at"] = json!("2999-01-01T00:00:00Z");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&future_generated_at).unwrap()).unwrap();
        let failed_future_generated_at = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_future_generated_at, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_future_generated_at, "/checks/generated_at_not_future"));

        let mut placeholder_provider = spv_report.clone();
        placeholder_provider["evidence_provider"] = json!("REPLACE_WITH_EXTERNAL_SPV_OPERATOR_OR_SERVICE");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&placeholder_provider).unwrap())
            .unwrap();
        let failed_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_provider, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_provider, "/checks/evidence_provider_identity"));

        let mut unknown_provider = spv_report.clone();
        unknown_provider["evidence_provider"] = json!("unknown-spv-provider");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&unknown_provider).unwrap()).unwrap();
        let failed_unknown_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_unknown_provider, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_unknown_provider, "/checks/evidence_provider_identity"));

        let mut local_provider = spv_report.clone();
        local_provider["evidence_provider"] = json!("local-devnet-spv-provider");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&local_provider).unwrap()).unwrap();
        let failed_local_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_local_provider, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_local_provider, "/checks/evidence_provider_identity"));

        let mut first_party_provider = spv_report.clone();
        first_party_provider["evidence_provider"] = json!("NovaSeal BTC SPV Desk");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&first_party_provider).unwrap())
            .unwrap();
        let failed_first_party_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_first_party_provider, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_first_party_provider, "/checks/evidence_provider_identity"));

        let mut owner_provider = spv_report.clone();
        owner_provider["evidence_provider"] = json!("a19q3 BTC SPV Desk");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&owner_provider).unwrap()).unwrap();
        let failed_owner_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_owner_provider, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_owner_provider, "/checks/evidence_provider_identity"));

        let mut placeholder_network = spv_report.clone();
        placeholder_network["network"] = json!("testnet-or-mainnet");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&placeholder_network).unwrap()).unwrap();
        let failed_network = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_network, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_network, "/checks/network_public"));

        let mut local_testnet_network = spv_report.clone();
        local_testnet_network["network"] = json!("local-testnet");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&local_testnet_network).unwrap())
            .unwrap();
        let failed_local_testnet = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_local_testnet, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_local_testnet, "/checks/network_public"));

        let mut top_level_extra = spv_report.clone();
        top_level_extra["unexpected_provider_field"] = Value::String("must-fail".to_string());
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&top_level_extra).unwrap()).unwrap();
        let failed_top_level = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_top_level, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_top_level, "/checks/top_level_fields_exact"));

        let mut case_extra = spv_report.clone();
        case_extra["cases"][0]["unexpected_case_field"] = Value::String("must-fail".to_string());
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&case_extra).unwrap()).unwrap();
        let failed_case = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_case, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_case, "/case_checks/btc-transaction-commitment-profile-v0/fields_exact"));

        let mut stale_scenario = spv_report.clone();
        stale_scenario["cases"][0]["scenario"] = json!("generic-public-btc-proof");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_scenario).unwrap()).unwrap();
        let failed_scenario = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_scenario, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_scenario, "/case_checks/btc-transaction-commitment-profile-v0/scenario_matches_expected"));

        let mut zero_btc_txid = spv_report.clone();
        zero_btc_txid["cases"][0]["btc_txid"] = json!(format!("0x{}", "00".repeat(32)));
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&zero_btc_txid).unwrap()).unwrap();
        let failed_zero_btc_txid = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_zero_btc_txid, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_zero_btc_txid,
            "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_non_placeholder"
        ));

        let mut zero_spv_out_point = spv_report.clone();
        zero_spv_out_point["cases"][0]["spv_client_cell_dep"]["out_point"] = json!(format!("0x{}:0", "00".repeat(32)));
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&zero_spv_out_point).unwrap()).unwrap();
        let failed_zero_spv_out_point = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_zero_spv_out_point, "/status"), Some("failed"));
        assert!(json_pointer_bool(
            &failed_zero_spv_out_point,
            "/case_checks/btc-transaction-commitment-profile-v0/spv_client_cell_dep_out_point_valid"
        ));
        assert!(!json_pointer_bool(
            &failed_zero_spv_out_point,
            "/case_checks/btc-transaction-commitment-profile-v0/spv_client_cell_dep_out_point_non_placeholder"
        ));

        let mut cell_dep_extra = spv_report.clone();
        cell_dep_extra["cases"][0]["spv_client_cell_dep"]["unexpected_cell_dep_field"] = Value::String("must-fail".to_string());
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&cell_dep_extra).unwrap()).unwrap();
        let failed_cell_dep = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_cell_dep, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_cell_dep,
            "/case_checks/btc-transaction-commitment-profile-v0/spv_client_cell_dep_fields_exact"
        ));

        let mut source_service_extra = spv_report.clone();
        source_service_extra["cases"][0]["source_service"]["unexpected_source_field"] = Value::String("must-fail".to_string());
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&source_service_extra).unwrap())
            .unwrap();
        let failed_source_service = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_source_service, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_source_service,
            "/case_checks/btc-transaction-commitment-profile-v0/source_service_fields_exact"
        ));

        let mut source_service_short_commit = spv_report.clone();
        source_service_short_commit["cases"][0]["source_service"]["commit"] = json!("0123456789abcdef");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&source_service_short_commit).unwrap())
            .unwrap();
        let failed_source_service_commit = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_source_service_commit, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_source_service_commit,
            "/case_checks/btc-transaction-commitment-profile-v0/source_service_commit_40_hex"
        ));

        let mut first_party_source_service = spv_report.clone();
        first_party_source_service["cases"][0]["source_service"]["name"] = json!("CellScript SPV Service");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&first_party_source_service).unwrap())
            .unwrap();
        let failed_first_party_source_service = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_first_party_source_service, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_first_party_source_service,
            "/case_checks/btc-transaction-commitment-profile-v0/source_service_name_identity"
        ));

        let mut source_service_zero_report_hash = spv_report.clone();
        source_service_zero_report_hash["cases"][0]["source_service"]["report_hash"] = json!(format!("0x{}", "00".repeat(32)));
        std::fs::write(
            proofs.join("public_btc_spv_evidence.json"),
            serde_json::to_vec_pretty(&source_service_zero_report_hash).unwrap(),
        )
        .unwrap();
        let failed_source_service_zero_report_hash =
            validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_source_service_zero_report_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(
            &failed_source_service_zero_report_hash,
            "/case_checks/btc-transaction-commitment-profile-v0/source_service_report_hash_non_placeholder"
        ));

        let mut handoff_extra = spv_report.clone();
        handoff_extra["request_handoff"]["unexpected_handoff_field"] = Value::String("must-fail".to_string());
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&handoff_extra).unwrap()).unwrap();
        let failed_handoff = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_handoff, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_handoff, "/checks/request_handoff_fields_exact"));

        let mut handoff_wrong_algorithm = spv_report.clone();
        handoff_wrong_algorithm["request_handoff"]["bundle_hash_algorithm"] = json!("sha256");
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&handoff_wrong_algorithm).unwrap())
            .unwrap();
        let failed_handoff_algorithm = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_handoff_algorithm, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_handoff_algorithm, "/checks/request_handoff_bundle_hash_algorithm"));

        let mut stale_required_profiles = spv_report.clone();
        stale_required_profiles["required_profiles"] = json!([EXPECTED_BTC_TX_COMMITMENT_PROFILE]);
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_required_profiles).unwrap())
            .unwrap();
        let failed_required_profiles = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_required_profiles, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_required_profiles, "/checks/required_profiles_field_exact"));

        let mut extra_profile_case = spv_report.clone();
        extra_profile_case["cases"].as_array_mut().unwrap().push(case_for("unexpected-profile-v0"));
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&extra_profile_case).unwrap()).unwrap();
        let failed_extra_profile = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_extra_profile, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_extra_profile, "/checks/required_profiles_covered_exact"));
        assert_eq!(json_array_strings(&failed_extra_profile, "/extra_profiles"), vec!["unexpected-profile-v0".to_string()]);

        let mut duplicate_profile_case = spv_report.clone();
        duplicate_profile_case["cases"].as_array_mut().unwrap().push(case_for(EXPECTED_BTC_TX_COMMITMENT_PROFILE));
        std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&duplicate_profile_case).unwrap())
            .unwrap();
        let failed_duplicate_profile = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
        assert_eq!(json_pointer_str(&failed_duplicate_profile, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_duplicate_profile, "/checks/required_profiles_covered_exact"));
    }

    #[test]
    fn external_attestations_require_exact_report_fields() {
        let temp = tempfile::tempdir().unwrap();
        let proofs = temp.path().join("proposals/novaseal/v0-mvp-skeleton/proofs");
        std::fs::create_dir_all(&proofs).unwrap();
        let artifact_hash = format!("0x{}", "aa".repeat(32));
        let source_tree_hash = format!("0x{}", "bb".repeat(32));
        let tcb_repo_commit = "0123456789abcdef0123456789abcdef01234567";
        let handoff = json!({
            "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
            "status": "passed",
            "cases": [
                {
                    "group": "public_shared_cell_dep_attestation",
                    "expected_values": {
                        "artifact_hash": artifact_hash,
                        "release.manifest_commit": tcb_repo_commit,
                        "release.package": "novaseal",
                        "release.version": EXPECTED_NOVASEAL_RELEASE_VERSION,
                        "runtime_verifier.dep_type": EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE,
                        "runtime_verifier.hash_type": EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE,
                        "runtime_verifier.ipc_abi": "cellscript-btc-bip340-ipc-v0",
                        "runtime_verifier.verifier_id": "btc.bip340.v0",
                    },
                },
                {
                    "group": "external_bip340_tcb_review_attestation",
                    "expected_values": {
                        "artifact_hash": artifact_hash,
                        "artifact_hash_algorithm": "sha256",
                        "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                        "review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                        "source_tree_sha256": source_tree_hash,
                        "verifier_id": "btc.bip340.v0",
                    },
                },
            ],
        });
        let handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &handoff);
        let public_attestation = json!({
            "schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
            "status": "attested",
            "network": "testnet",
            "attested_at": "2026-06-04T00:00:00Z",
            "attestor": "external-cell-dep-operator",
            "release": {
                "package": "novaseal",
                "version": "0.0.1-v0-mvp",
                "manifest_commit": tcb_repo_commit,
            },
            "notes": "external public CellDep attestation fixture",
            "request_handoff": {
                "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                "bundle_hash": handoff_hash,
                "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                "group": "public_shared_cell_dep_attestation",
            },
            "runtime_verifier": {
                "verifier_id": "btc.bip340.v0",
                "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                "artifact_hash": artifact_hash,
                "out_point": format!("0x{}:0", "11".repeat(32)),
                "data_hash": format!("0x{}", "22".repeat(32)),
                "dep_type": "code",
                "hash_type": "data1",
            },
        });
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_attestation).unwrap(),
        )
        .unwrap();
        let public_passed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_passed, "/status"), Some("passed"));
        assert!(json_pointer_bool(&public_passed, "/checks/top_level_fields_exact"));
        assert!(json_pointer_bool(&public_passed, "/checks/release_fields_exact"));
        assert!(json_pointer_bool(&public_passed, "/checks/release_version_matches_expected"));
        assert!(json_pointer_bool(&public_passed, "/checks/release_manifest_commit_present"));
        assert!(json_pointer_bool(&public_passed, "/checks/release_manifest_commit_matches_tcb"));
        assert!(json_pointer_bool(&public_passed, "/checks/network_public"));
        assert!(json_pointer_bool(&public_passed, "/checks/attested_at_utc_timestamp"));
        assert!(json_pointer_bool(&public_passed, "/checks/attested_at_not_future"));
        assert!(json_pointer_bool(&public_passed, "/checks/attestor_identity"));
        assert!(json_pointer_bool(&public_passed, "/checks/request_handoff_fields_exact"));
        assert!(json_pointer_bool(&public_passed, "/checks/request_handoff_bundle_hash_algorithm"));
        assert!(json_pointer_bool(&public_passed, "/checks/handoff_expected_values_exact"));
        assert!(json_pointer_bool(&public_passed, "/checks/release_version_matches_handoff"));
        assert!(json_pointer_bool(&public_passed, "/checks/release_manifest_commit_matches_handoff"));
        assert!(json_pointer_bool(&public_passed, "/checks/runtime_verifier_fields_exact"));
        assert!(json_pointer_bool(&public_passed, "/checks/artifact_hash_valid"));
        assert!(json_pointer_bool(&public_passed, "/checks/artifact_hash_non_placeholder"));
        assert!(json_pointer_bool(&public_passed, "/checks/artifact_hash_matches_handoff"));
        assert!(json_pointer_bool(&public_passed, "/checks/out_point_valid"));
        assert!(json_pointer_bool(&public_passed, "/checks/dep_type"));
        assert!(json_pointer_bool(&public_passed, "/checks/dep_type_matches_handoff"));
        assert!(json_pointer_bool(&public_passed, "/checks/hash_type"));
        assert!(json_pointer_bool(&public_passed, "/checks/hash_type_matches_expected"));
        assert!(json_pointer_bool(&public_passed, "/checks/hash_type_matches_handoff"));
        assert!(json_pointer_bool(&public_passed, "/checks/data_hash_valid"));

        let mut stale_public_handoff = handoff.clone();
        stale_public_handoff["cases"][0]["expected_values"]["release.version"] = json!("0.0.2");
        let stale_public_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_public_handoff);
        let mut public_with_stale_handoff = public_attestation.clone();
        public_with_stale_handoff["request_handoff"]["bundle_hash"] = json!(stale_public_handoff_hash);
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_with_stale_handoff).unwrap(),
        )
        .unwrap();
        let public_stale_handoff_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &stale_public_handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_stale_handoff_failed, "/status"), Some("failed"));
        assert!(json_pointer_bool(&public_stale_handoff_failed, "/checks/request_handoff_bundle_hash_matches_current"));
        assert!(!json_pointer_bool(&public_stale_handoff_failed, "/checks/release_version_matches_handoff"));

        let mut public_placeholder_attested_at = public_attestation.clone();
        public_placeholder_attested_at["attested_at"] = json!("YYYY-MM-DDTHH:MM:SSZ");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_placeholder_attested_at).unwrap(),
        )
        .unwrap();
        let public_attested_at_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_attested_at_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_attested_at_failed, "/checks/attested_at_utc_timestamp"));

        let mut public_future_attested_at = public_attestation.clone();
        public_future_attested_at["attested_at"] = json!("2999-01-01T00:00:00Z");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_future_attested_at).unwrap(),
        )
        .unwrap();
        let public_future_attested_at_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_future_attested_at_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_future_attested_at_failed, "/checks/attested_at_not_future"));

        let mut public_placeholder_network = public_attestation.clone();
        public_placeholder_network["network"] = json!("testnet-or-mainnet");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_placeholder_network).unwrap(),
        )
        .unwrap();
        let public_network_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_network_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_network_failed, "/checks/network_public"));

        let mut public_private_network = public_attestation.clone();
        public_private_network["network"] = json!("ckb-private-mainnet");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_private_network).unwrap(),
        )
        .unwrap();
        let public_private_network_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_private_network_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_private_network_failed, "/checks/network_public"));

        let mut public_invalid_out_point = public_attestation.clone();
        public_invalid_out_point["runtime_verifier"]["out_point"] = json!(format!("0x{}:not-an-index", "11".repeat(32)));
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_invalid_out_point).unwrap(),
        )
        .unwrap();
        let public_out_point_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_out_point_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_out_point_failed, "/checks/out_point_valid"));

        let mut public_invalid_dep_type = public_attestation.clone();
        public_invalid_dep_type["runtime_verifier"]["dep_type"] = json!("dep_group");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_invalid_dep_type).unwrap(),
        )
        .unwrap();
        let public_dep_type_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_dep_type_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_dep_type_failed, "/checks/dep_type"));

        let mut public_invalid_hash_type = public_attestation.clone();
        public_invalid_hash_type["runtime_verifier"]["hash_type"] = json!("invalid-hash-type");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_invalid_hash_type).unwrap(),
        )
        .unwrap();
        let public_hash_type_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_hash_type_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_hash_type_failed, "/checks/hash_type"));

        let mut public_stale_hash_type = public_attestation.clone();
        public_stale_hash_type["runtime_verifier"]["hash_type"] = json!("type");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_stale_hash_type).unwrap(),
        )
        .unwrap();
        let public_stale_hash_type_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_stale_hash_type_failed, "/status"), Some("failed"));
        assert!(json_pointer_bool(&public_stale_hash_type_failed, "/checks/hash_type"));
        assert!(!json_pointer_bool(&public_stale_hash_type_failed, "/checks/hash_type_matches_expected"));

        let mut public_invalid_data_hash = public_attestation.clone();
        public_invalid_data_hash["runtime_verifier"]["data_hash"] = json!("0xnot-a-32-byte-hash");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_invalid_data_hash).unwrap(),
        )
        .unwrap();
        let public_data_hash_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_data_hash_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_data_hash_failed, "/checks/data_hash_valid"));

        let mut public_invalid_artifact_hash = public_attestation.clone();
        public_invalid_artifact_hash["runtime_verifier"]["artifact_hash"] = json!("0xnot-a-32-byte-hash");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_invalid_artifact_hash).unwrap(),
        )
        .unwrap();
        let public_artifact_hash_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_artifact_hash_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_artifact_hash_failed, "/checks/artifact_hash_valid"));

        let mut public_zero_artifact_hash = public_attestation.clone();
        public_zero_artifact_hash["runtime_verifier"]["artifact_hash"] = json!(format!("0x{}", "00".repeat(32)));
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_zero_artifact_hash).unwrap(),
        )
        .unwrap();
        let public_zero_artifact_hash_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_zero_artifact_hash_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_zero_artifact_hash_failed, "/checks/artifact_hash_non_placeholder"));

        let mut public_stale_manifest_commit = public_attestation.clone();
        public_stale_manifest_commit["release"]["manifest_commit"] = json!("fedcba9876543210fedcba9876543210fedcba98");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_stale_manifest_commit).unwrap(),
        )
        .unwrap();
        let public_manifest_commit_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_manifest_commit_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_manifest_commit_failed, "/checks/release_manifest_commit_matches_tcb"));

        let mut public_stale_release_version = public_attestation.clone();
        public_stale_release_version["release"]["version"] = json!("0.0.2");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_stale_release_version).unwrap(),
        )
        .unwrap();
        let public_release_version_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_release_version_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_release_version_failed, "/checks/release_version_matches_expected"));

        let mut public_placeholder_attestor = public_attestation.clone();
        public_placeholder_attestor["attestor"] = json!("REPLACE_WITH_DEPLOYER_OR_RELEASE_SIGNER");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_placeholder_attestor).unwrap(),
        )
        .unwrap();
        let public_attestor_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_attestor_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_attestor_failed, "/checks/attestor_identity"));

        let mut public_local_attestor = public_attestation.clone();
        public_local_attestor["attestor"] = json!("local-devnet-deployer");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_local_attestor).unwrap(),
        )
        .unwrap();
        let public_local_attestor_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_local_attestor_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_local_attestor_failed, "/checks/attestor_identity"));

        let mut public_first_party_attestor = public_attestation.clone();
        public_first_party_attestor["attestor"] = json!("NovaSeal Release Bot");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_first_party_attestor).unwrap(),
        )
        .unwrap();
        let public_first_party_attestor_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_first_party_attestor_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_first_party_attestor_failed, "/checks/attestor_identity"));

        let mut public_extra = public_attestation.clone();
        public_extra["unexpected_provider_field"] = Value::String("must-fail".to_string());
        std::fs::write(proofs.join("public_shared_cell_dep_attestation.json"), serde_json::to_vec_pretty(&public_extra).unwrap())
            .unwrap();
        let public_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_failed, "/checks/top_level_fields_exact"));

        let mut public_nested_extra = public_attestation.clone();
        public_nested_extra["runtime_verifier"]["unexpected_runtime_field"] = Value::String("must-fail".to_string());
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_nested_extra).unwrap(),
        )
        .unwrap();
        let public_nested_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_nested_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_nested_failed, "/checks/runtime_verifier_fields_exact"));

        let mut public_release_string = public_attestation.clone();
        public_release_string["release"] = Value::String("novaseal-btc-bip340-v0".to_string());
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_release_string).unwrap(),
        )
        .unwrap();
        let public_release_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_release_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_release_failed, "/checks/release_fields_exact"));

        let mut public_handoff_extra = public_attestation.clone();
        public_handoff_extra["request_handoff"]["unexpected_handoff_field"] = Value::String("must-fail".to_string());
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_handoff_extra).unwrap(),
        )
        .unwrap();
        let public_handoff_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_handoff_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_handoff_failed, "/checks/request_handoff_fields_exact"));

        let mut public_handoff_wrong_algorithm = public_attestation.clone();
        public_handoff_wrong_algorithm["request_handoff"]["bundle_hash_algorithm"] = json!("sha256");
        std::fs::write(
            proofs.join("public_shared_cell_dep_attestation.json"),
            serde_json::to_vec_pretty(&public_handoff_wrong_algorithm).unwrap(),
        )
        .unwrap();
        let public_handoff_algorithm_failed = validate_public_attestation(
            temp.path(),
            PUBLIC_CELLDEP_ATTESTATION,
            Some(&artifact_hash),
            Some(tcb_repo_commit),
            &handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&public_handoff_algorithm_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&public_handoff_algorithm_failed, "/checks/request_handoff_bundle_hash_algorithm"));

        let external_review = json!({
            "schema": "novaseal-bip340-external-tcb-review-attestation-v0.1",
            "status": "accepted",
            "artifact_hash": artifact_hash,
            "artifact_hash_algorithm": "sha256",
            "source_tree_sha256": source_tree_hash,
            "verifier_id": "btc.bip340.v0",
            "ipc_abi": "cellscript-btc-bip340-ipc-v0",
            "reviewer": "external-tcb-reviewer",
            "review_date": "2026-06-04",
            "review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
            "report_uri": "https://audits.nervos.org/novaseal-bip340-tcb-review",
            "notes": "external review fixture",
            "request_handoff": {
                "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                "bundle_hash": handoff_hash,
                "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                "group": "external_bip340_tcb_review_attestation",
            },
        });
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&external_review).unwrap(),
        )
        .unwrap();
        let review_passed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_passed, "/status"), Some("passed"));
        assert!(json_pointer_bool(&review_passed, "/checks/top_level_fields_exact"));
        assert!(json_pointer_bool(&review_passed, "/checks/request_handoff_fields_exact"));
        assert!(json_pointer_bool(&review_passed, "/checks/request_handoff_bundle_hash_algorithm"));
        assert!(json_pointer_bool(&review_passed, "/checks/handoff_expected_values_exact"));
        assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_valid"));
        assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_non_placeholder"));
        assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_matches_handoff"));
        assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_algorithm"));
        assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_algorithm_matches_handoff"));
        assert!(json_pointer_bool(&review_passed, "/checks/source_tree_sha256_valid"));
        assert!(json_pointer_bool(&review_passed, "/checks/source_tree_sha256_non_placeholder"));
        assert!(json_pointer_bool(&review_passed, "/checks/source_tree_sha256_matches_current_tcb"));
        assert!(json_pointer_bool(&review_passed, "/checks/source_tree_sha256_matches_handoff"));
        assert!(json_pointer_bool(&review_passed, "/checks/verifier_id"));
        assert!(json_pointer_bool(&review_passed, "/checks/verifier_id_matches_handoff"));
        assert!(json_pointer_bool(&review_passed, "/checks/ipc_abi"));
        assert!(json_pointer_bool(&review_passed, "/checks/ipc_abi_matches_handoff"));
        assert!(json_pointer_bool(&review_passed, "/checks/reviewer_identity"));
        assert!(json_pointer_bool(&review_passed, "/checks/review_date_utc_date"));
        assert!(json_pointer_bool(&review_passed, "/checks/review_date_not_future"));
        assert!(json_pointer_bool(&review_passed, "/checks/report_uri_https"));
        assert!(json_pointer_bool(&review_passed, "/checks/review_scope_exact"));
        assert!(json_pointer_bool(&review_passed, "/checks/review_scope_matches_handoff"));

        let mut stale_review_handoff = handoff.clone();
        stale_review_handoff["cases"][1]["expected_values"]["source_tree_sha256"] = json!(format!("0x{}", "cc".repeat(32)));
        let stale_review_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_review_handoff);
        let mut review_with_stale_handoff = external_review.clone();
        review_with_stale_handoff["request_handoff"]["bundle_hash"] = json!(stale_review_handoff_hash);
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_with_stale_handoff).unwrap(),
        )
        .unwrap();
        let review_stale_handoff_failed = validate_external_review(
            temp.path(),
            EXTERNAL_TCB_ATTESTATION,
            Some(&artifact_hash),
            Some(&source_tree_hash),
            &stale_review_handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&review_stale_handoff_failed, "/status"), Some("failed"));
        assert!(json_pointer_bool(&review_stale_handoff_failed, "/checks/request_handoff_bundle_hash_matches_current"));
        assert!(!json_pointer_bool(&review_stale_handoff_failed, "/checks/source_tree_sha256_matches_handoff"));

        let mut stale_review_ipc_handoff = handoff.clone();
        stale_review_ipc_handoff["cases"][1]["expected_values"]["ipc_abi"] = json!("cellscript-btc-bip340-ipc-v1");
        let stale_review_ipc_handoff_hash =
            novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_review_ipc_handoff);
        let mut review_with_stale_ipc_handoff = external_review.clone();
        review_with_stale_ipc_handoff["request_handoff"]["bundle_hash"] = json!(stale_review_ipc_handoff_hash);
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_with_stale_ipc_handoff).unwrap(),
        )
        .unwrap();
        let review_stale_ipc_handoff_failed = validate_external_review(
            temp.path(),
            EXTERNAL_TCB_ATTESTATION,
            Some(&artifact_hash),
            Some(&source_tree_hash),
            &stale_review_ipc_handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&review_stale_ipc_handoff_failed, "/status"), Some("failed"));
        assert!(json_pointer_bool(&review_stale_ipc_handoff_failed, "/checks/request_handoff_bundle_hash_matches_current"));
        assert!(!json_pointer_bool(&review_stale_ipc_handoff_failed, "/checks/ipc_abi_matches_handoff"));

        let mut stale_review_verifier_handoff = handoff.clone();
        stale_review_verifier_handoff["cases"][1]["expected_values"]["verifier_id"] = json!("btc.bip340.v1");
        let stale_review_verifier_handoff_hash =
            novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_review_verifier_handoff);
        let mut review_with_stale_verifier_handoff = external_review.clone();
        review_with_stale_verifier_handoff["request_handoff"]["bundle_hash"] = json!(stale_review_verifier_handoff_hash);
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_with_stale_verifier_handoff).unwrap(),
        )
        .unwrap();
        let review_stale_verifier_handoff_failed = validate_external_review(
            temp.path(),
            EXTERNAL_TCB_ATTESTATION,
            Some(&artifact_hash),
            Some(&source_tree_hash),
            &stale_review_verifier_handoff,
        )
        .unwrap();
        assert_eq!(json_pointer_str(&review_stale_verifier_handoff_failed, "/status"), Some("failed"));
        assert!(json_pointer_bool(&review_stale_verifier_handoff_failed, "/checks/request_handoff_bundle_hash_matches_current"));
        assert!(!json_pointer_bool(&review_stale_verifier_handoff_failed, "/checks/verifier_id_matches_handoff"));

        let mut review_placeholder_date = external_review.clone();
        review_placeholder_date["review_date"] = json!("YYYY-MM-DD");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_placeholder_date).unwrap(),
        )
        .unwrap();
        let review_date_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_date_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_date_failed, "/checks/review_date_utc_date"));

        let mut review_future_date = external_review.clone();
        review_future_date["review_date"] = json!("2999-01-01");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_future_date).unwrap(),
        )
        .unwrap();
        let review_future_date_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_future_date_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_future_date_failed, "/checks/review_date_not_future"));

        let mut review_stale_source_tree = external_review.clone();
        review_stale_source_tree["source_tree_sha256"] = json!(format!("0x{}", "cc".repeat(32)));
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_stale_source_tree).unwrap(),
        )
        .unwrap();
        let review_stale_source_tree_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_stale_source_tree_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_stale_source_tree_failed, "/checks/source_tree_sha256_matches_current_tcb"));

        let mut review_invalid_artifact_hash = external_review.clone();
        review_invalid_artifact_hash["artifact_hash"] = json!("0xnot-a-32-byte-hash");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_invalid_artifact_hash).unwrap(),
        )
        .unwrap();
        let review_invalid_artifact_hash_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_invalid_artifact_hash_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_invalid_artifact_hash_failed, "/checks/artifact_hash_valid"));

        let mut review_zero_artifact_hash = external_review.clone();
        review_zero_artifact_hash["artifact_hash"] = json!(format!("0x{}", "00".repeat(32)));
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_zero_artifact_hash).unwrap(),
        )
        .unwrap();
        let review_zero_artifact_hash_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_zero_artifact_hash_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_zero_artifact_hash_failed, "/checks/artifact_hash_non_placeholder"));

        let mut review_zero_source_tree = external_review.clone();
        review_zero_source_tree["source_tree_sha256"] = json!(format!("0x{}", "00".repeat(32)));
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_zero_source_tree).unwrap(),
        )
        .unwrap();
        let review_zero_source_tree_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_zero_source_tree_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_zero_source_tree_failed, "/checks/source_tree_sha256_non_placeholder"));

        let mut review_placeholder_reviewer = external_review.clone();
        review_placeholder_reviewer["reviewer"] = json!("REPLACE_WITH_EXTERNAL_REVIEWER");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_placeholder_reviewer).unwrap(),
        )
        .unwrap();
        let review_reviewer_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_reviewer_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_reviewer_failed, "/checks/reviewer_identity"));

        let mut review_unknown_reviewer = external_review.clone();
        review_unknown_reviewer["reviewer"] = json!("unknown-reviewer");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_unknown_reviewer).unwrap(),
        )
        .unwrap();
        let review_unknown_reviewer_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_unknown_reviewer_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_unknown_reviewer_failed, "/checks/reviewer_identity"));

        let mut review_local_reviewer = external_review.clone();
        review_local_reviewer["reviewer"] = json!("local-devnet-reviewer");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_local_reviewer).unwrap(),
        )
        .unwrap();
        let review_local_reviewer_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_local_reviewer_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_local_reviewer_failed, "/checks/reviewer_identity"));

        let mut review_first_party_reviewer = external_review.clone();
        review_first_party_reviewer["reviewer"] = json!("CellScript TCB Review Team");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_first_party_reviewer).unwrap(),
        )
        .unwrap();
        let review_first_party_reviewer_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_first_party_reviewer_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_first_party_reviewer_failed, "/checks/reviewer_identity"));

        let mut review_placeholder_uri = external_review.clone();
        review_placeholder_uri["report_uri"] = json!("REPLACE_WITH_EXTERNAL_REVIEW_REPORT_OR_COMMIT_URI");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_placeholder_uri).unwrap(),
        )
        .unwrap();
        let review_uri_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_uri_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_uri_failed, "/checks/report_uri_https"));

        let mut review_example_uri = external_review.clone();
        review_example_uri["report_uri"] = json!("https://audits.nervos.example.org/novaseal-bip340-tcb-review");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_example_uri).unwrap(),
        )
        .unwrap();
        let review_example_uri_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_example_uri_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_example_uri_failed, "/checks/report_uri_https"));

        let mut review_private_uri = external_review.clone();
        review_private_uri["report_uri"] = json!("https://192.168.1.1/novaseal-bip340-tcb-review");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_private_uri).unwrap(),
        )
        .unwrap();
        let review_private_uri_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_private_uri_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_private_uri_failed, "/checks/report_uri_https"));

        let mut review_extra = external_review.clone();
        review_extra["unexpected_provider_field"] = Value::String("must-fail".to_string());
        std::fs::write(proofs.join("bip340_external_tcb_review_attestation.json"), serde_json::to_vec_pretty(&review_extra).unwrap())
            .unwrap();
        let review_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_failed, "/checks/top_level_fields_exact"));

        let mut review_scope_string = external_review.clone();
        review_scope_string["review_scope"] = Value::String("BIP340 runtime verifier TCB".to_string());
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_scope_string).unwrap(),
        )
        .unwrap();
        let review_scope_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_scope_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_scope_failed, "/checks/review_scope_exact"));

        let mut review_scope_incomplete = external_review.clone();
        review_scope_incomplete["review_scope"] = json!(["BIP340 verifier core"]);
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_scope_incomplete).unwrap(),
        )
        .unwrap();
        let review_scope_incomplete_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_scope_incomplete_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_scope_incomplete_failed, "/checks/review_scope_exact"));

        let mut review_handoff_extra = external_review.clone();
        review_handoff_extra["request_handoff"]["unexpected_handoff_field"] = Value::String("must-fail".to_string());
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_handoff_extra).unwrap(),
        )
        .unwrap();
        let review_handoff_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_handoff_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_handoff_failed, "/checks/request_handoff_fields_exact"));

        let mut review_handoff_wrong_algorithm = external_review.clone();
        review_handoff_wrong_algorithm["request_handoff"]["bundle_hash_algorithm"] = json!("sha256");
        std::fs::write(
            proofs.join("bip340_external_tcb_review_attestation.json"),
            serde_json::to_vec_pretty(&review_handoff_wrong_algorithm).unwrap(),
        )
        .unwrap();
        let review_handoff_algorithm_failed =
            validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
                .unwrap();
        assert_eq!(json_pointer_str(&review_handoff_algorithm_failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&review_handoff_algorithm_failed, "/checks/request_handoff_bundle_hash_algorithm"));
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
        assert!(stateful_local_acceptance_passed(&report));

        report["business_scenario_coverage"]["status"] = Value::String("failed".to_string());
        assert!(!stateful_acceptance_passed(&report));
        assert!(!stateful_local_acceptance_passed(&report));

        report["business_scenario_coverage"]["status"] = Value::String("passed".to_string());
        report["status"] = Value::String("local_devnet_passed_external_endpoint_required".to_string());
        report["blocker_count"] = json!(1);
        report["local_blocker_count"] = json!(0);
        report["acceptance_blocker_count"] = json!(1);
        assert!(!stateful_acceptance_passed(&report));
        assert!(stateful_local_acceptance_passed(&report));

        report["local_blocker_count"] = json!(1);
        assert!(!stateful_local_acceptance_passed(&report));
    }

    #[test]
    fn live_acceptance_blockers_include_unrun_devnet_and_external_endpoint_gaps() {
        let scenarios = vec![json!({
            "name": "fiber_candidate_settlement",
            "status": "ready_to_wire_live_devnet",
            "live_devnet_rpc_executed": false,
            "stateful_lifecycle_executed": false,
        })];
        let external_endpoint_coverage = json!({
            "status": "failed",
            "btc": {"status": "external_required"},
            "fiber": {"status": "failed"},
        });

        let blockers = stateful_live_acceptance_blockers(&scenarios, false, false, &external_endpoint_coverage);

        assert_eq!(blockers.len(), 4);
        assert_eq!(json_pointer_str(&blockers[0], "/scenario"), Some("fiber_candidate_settlement"));
        assert!(!json_pointer_bool(&blockers[0], "/live_devnet_rpc_executed"));
        assert!(blockers.iter().any(|blocker| json_pointer_str(blocker, "/dimension") == Some("profile_coverage")));
        assert!(blockers.iter().any(|blocker| json_pointer_str(blocker, "/dimension") == Some("business_scenario_coverage")));
        let endpoint =
            blockers.iter().find(|blocker| json_pointer_str(blocker, "/dimension") == Some("external_endpoint_coverage")).unwrap();
        assert_eq!(json_pointer_str(endpoint, "/btc_status"), Some("external_required"));
        assert_eq!(json_pointer_str(endpoint, "/fiber_status"), Some("failed"));
    }

    #[test]
    fn stateful_acceptance_status_stays_pending_when_external_endpoint_is_required() {
        let external_endpoint_coverage = json!({
            "status": "external_required",
            "btc": {"status": "external_required"},
            "fiber": {"status": "passed"},
        });

        let status = stateful_acceptance_status(0, 1, true, true, true, &external_endpoint_coverage);

        assert_eq!(status, "local_devnet_passed_external_endpoint_required");

        let status = stateful_acceptance_status(0, 0, true, true, true, &external_endpoint_coverage);

        assert_eq!(status, "local_devnet_passed_external_endpoint_required");
    }

    fn write_fiber_workflow_fixture_files(repo_root: &Path, fiber_repo: &Path, suite: &str) {
        let evidence_file = fiber_repo.join(format!("tests/bruno/e2e/{suite}/step.bru"));
        let stdout_log = repo_root.join(format!("target/novaseal-fiber-node-experiments/{suite}/bruno.stdout"));
        let stderr_log = repo_root.join(format!("target/novaseal-fiber-node-experiments/{suite}/bruno.stderr"));
        std::fs::create_dir_all(evidence_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(stdout_log.parent().unwrap()).unwrap();
        std::fs::create_dir_all(stderr_log.parent().unwrap()).unwrap();
        std::fs::write(evidence_file, "meta { name: step }\n").unwrap();
        std::fs::write(stdout_log, "Bruno suite passed\n").unwrap();
        std::fs::write(stderr_log, "").unwrap();
    }

    fn git_available() -> bool {
        std::process::Command::new("git").arg("--version").output().is_ok()
    }

    fn run_fixture_git(fiber_repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git").current_dir(fiber_repo).args(args).output().unwrap();
        assert!(output.status.success(), "git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
    }

    fn run_fixture_git_with_identity(fiber_repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(fiber_repo)
            .args(["-c", "user.name=CellScript Test", "-c", "user.email=cellscript@example.invalid"])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
    }

    fn fixture_git_stdout(fiber_repo: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git").current_dir(fiber_repo).args(args).output().unwrap();
        assert!(output.status.success(), "git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn ensure_clean_fixture_repo(repo: &Path) -> String {
        let init = std::process::Command::new("git").arg("init").arg("--initial-branch=main").arg(repo).output().unwrap();
        if !init.status.success() {
            let fallback = std::process::Command::new("git").arg("init").arg(repo).output().unwrap();
            assert!(fallback.status.success(), "git init failed: {}", String::from_utf8_lossy(&fallback.stderr));
            run_fixture_git(repo, &["checkout", "-B", "main"]);
        }
        run_fixture_git(repo, &["add", "."]);
        run_fixture_git_with_identity(repo, &["commit", "-m", "fixture"]);
        let status = fixture_git_stdout(repo, &["status", "--porcelain"]);
        assert!(status.is_empty(), "fixture repo should be clean after commit, got status:\n{status}");
        fixture_git_stdout(repo, &["rev-parse", "HEAD"])
    }

    fn ensure_clean_fiber_fixture_repo(fiber_repo: &Path) -> String {
        if fiber_repo.join(".git").exists() {
            let status = fixture_git_stdout(fiber_repo, &["status", "--porcelain"]);
            assert!(status.is_empty(), "fixture Fiber repo should stay clean, got status:\n{status}");
            return fixture_git_stdout(fiber_repo, &["rev-parse", "HEAD"]);
        }

        let init = std::process::Command::new("git").arg("init").arg("--initial-branch=develop").arg(fiber_repo).output().unwrap();
        if !init.status.success() {
            let fallback = std::process::Command::new("git").arg("init").arg(fiber_repo).output().unwrap();
            assert!(fallback.status.success(), "git init failed: {}", String::from_utf8_lossy(&fallback.stderr));
            run_fixture_git(fiber_repo, &["checkout", "-B", "develop"]);
        }
        run_fixture_git(fiber_repo, &["remote", "add", "origin", EXPECTED_FIBER_REPO_ORIGIN]);
        run_fixture_git(fiber_repo, &["add", "."]);
        run_fixture_git_with_identity(fiber_repo, &["commit", "-m", "fiber fixture"]);
        let status = fixture_git_stdout(fiber_repo, &["status", "--porcelain"]);
        assert!(status.is_empty(), "fixture Fiber repo should be clean after commit, got status:\n{status}");
        fixture_git_stdout(fiber_repo, &["rev-parse", "HEAD"])
    }

    fn fiber_workflow_fixture(
        repo_root: &Path,
        fiber_repo: &Path,
        suite: &str,
        mapped_profiles: &[&str],
        fiber_repo_info: &Value,
    ) -> Value {
        write_fiber_workflow_fixture_files(repo_root, fiber_repo, suite);
        let mut execution = json!({
            "status": "passed",
            "started_node": true,
            "command": ["npm", "exec", "--", "@usebruno/cli", "run", format!("e2e/{suite}"), "-r", "--env", "test"],
            "returncode": 0,
            "duration_seconds": 1.0,
            "stdout_log": format!("target/novaseal-fiber-node-experiments/{suite}/bruno.stdout"),
            "stderr_log": format!("target/novaseal-fiber-node-experiments/{suite}/bruno.stderr"),
            "fiber_repo": fiber_repo_info,
        });
        if suite == "cross-chain-hub" {
            let bruno_cwd = "target/novaseal-fiber-node-experiments/cross-chain-hub/bruno-worktree";
            let patch = "e2e/cross-chain-hub/10-node1-add-fiber-invoice.bru";
            let patch_file = repo_root.join(bruno_cwd).join(patch);
            std::fs::create_dir_all(patch_file.parent().unwrap()).unwrap();
            std::fs::write(patch_file, "meta { name: patched }\n").unwrap();
            execution["bruno_cwd"] = Value::String(bruno_cwd.to_string());
            execution["bruno_compatibility_patches"] = json!([patch]);
        }
        json!({
            "suite": suite,
            "status": "passed",
            "present": true,
            "mapped_profiles": mapped_profiles,
            "expected_terms": {
                "term-a": true,
                "term-b": true,
            },
            "rpc_methods": ["open_channel"],
            "evidence_files": [format!("tests/bruno/e2e/{suite}/step.bru")],
            "execution": execution,
        })
    }

    fn complete_fiber_node_execution_report(repo_root: &Path, fiber_repo: &Path) -> Value {
        for (suite, _) in EXPECTED_FIBER_WORKFLOWS {
            write_fiber_workflow_fixture_files(repo_root, fiber_repo, suite);
        }
        let commit = ensure_clean_fiber_fixture_repo(fiber_repo);
        let fiber_repo_info = json!({
            "path": fiber_repo.display().to_string(),
            "origin": EXPECTED_FIBER_REPO_ORIGIN,
            "branch": "develop",
            "commit": commit,
            "dirty": false,
        });
        let workflows = EXPECTED_FIBER_WORKFLOWS
            .iter()
            .map(|(suite, profiles)| fiber_workflow_fixture(repo_root, fiber_repo, suite, profiles, &fiber_repo_info))
            .collect::<Vec<_>>();
        json!({
            "schema": EXPECTED_FIBER_NODE_EXECUTION_SCHEMA,
            "status": "passed",
            "fiber_repo": fiber_repo_info,
            "devnet_contract": {
                "runnable_devnet_contract_present": true,
            },
            "workflow_coverage": {
                "required_count": EXPECTED_FIBER_WORKFLOWS.len(),
                "present_count": EXPECTED_FIBER_WORKFLOWS.len(),
                "executed_count": EXPECTED_FIBER_WORKFLOWS.len(),
                "passed_execution_count": EXPECTED_FIBER_WORKFLOWS.len(),
                "all_required_workflows_present": true,
                "all_required_workflows_executed_passed": true,
                "partial_execution_passed": false,
            },
            "profiles_covered": EXPECTED_FIBER_NODE_PROFILES,
            "workflows": workflows,
        })
    }

    #[test]
    fn fiber_node_execution_requires_exact_suite_profile_and_execution_contract() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path().join("cellscript");
        let fiber_repo = temp.path().join("fiber");
        std::fs::create_dir_all(&repo_root).unwrap();
        std::fs::create_dir_all(&fiber_repo).unwrap();

        let passed = fiber_node_execution_summary(&repo_root, Some(&complete_fiber_node_execution_report(&repo_root, &fiber_repo)));
        assert!(json_pointer_bool(&passed, "/all_required_workflows_executed_passed"));
        assert!(json_pointer_bool(&passed, "/checks/workflow_suites_exact"));
        assert!(json_pointer_bool(&passed, "/checks/profiles_covered_exact"));
        assert!(json_pointer_bool(&passed, "/checks/fiber_repo_exists"));
        assert!(json_pointer_bool(&passed, "/checks/fiber_repo_git_provenance_verified"));
        assert!(json_pointer_bool(&passed, "/fiber_repo_git_provenance/checks/origin_matches_report"));
        assert!(json_pointer_bool(&passed, "/fiber_repo_git_provenance/checks/commit_matches_report"));
        assert!(json_pointer_bool(&passed, "/fiber_repo_git_provenance/checks/clean_tree"));
        assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/evidence_files_exist"));
        assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_logs_exist"));
        assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_started_node"));
        assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_command_exact"));
        assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_returncode_zero"));
        assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_duration_positive"));
        assert!(json_pointer_bool(&passed, "/workflow_checks/cross-chain-hub/bruno_compatibility_patch_files_exist"));
        assert!(json_pointer_bool(&passed, "/checks/reported_partial_execution_semantics"));

        let mut contradictory_partial = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        contradictory_partial["workflow_coverage"]["partial_execution_passed"] = json!(true);
        let failed_contradictory_partial = fiber_node_execution_summary(&repo_root, Some(&contradictory_partial));
        assert!(!json_pointer_bool(&failed_contradictory_partial, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_contradictory_partial, "/checks/reported_partial_execution_semantics"));

        let mut extra_suite = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        let mut unexpected_suite = extra_suite["workflows"][0].clone();
        unexpected_suite["suite"] = json!("unexpected-suite");
        unexpected_suite["mapped_profiles"] = json!([EXPECTED_FIBER_CANDIDATE_PROFILE]);
        extra_suite["workflows"].as_array_mut().unwrap().push(unexpected_suite);
        let failed_extra_suite = fiber_node_execution_summary(&repo_root, Some(&extra_suite));
        assert!(!json_pointer_bool(&failed_extra_suite, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_extra_suite, "/checks/workflow_suites_exact"));

        let mut wrong_profile = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        wrong_profile["workflows"][0]["mapped_profiles"] = json!([EXPECTED_FUNGIBLE_XUDT_PROFILE]);
        let failed_profile = fiber_node_execution_summary(&repo_root, Some(&wrong_profile));
        assert!(!json_pointer_bool(&failed_profile, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_profile, "/workflow_checks/open-use-close-a-channel/mapped_profiles_exact"));

        let mut dirty_repo = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        dirty_repo["fiber_repo"]["dirty"] = Value::Bool(true);
        let failed_dirty_repo = fiber_node_execution_summary(&repo_root, Some(&dirty_repo));
        assert!(!json_pointer_bool(&failed_dirty_repo, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_dirty_repo, "/checks/clean_expected_fiber_repo"));
        assert!(!json_pointer_bool(&failed_dirty_repo, "/checks/fiber_repo_git_provenance_verified"));
        assert!(!json_pointer_bool(&failed_dirty_repo, "/fiber_repo_git_provenance/checks/dirty_matches_report"));

        let mut forged_commit = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        forged_commit["fiber_repo"]["commit"] = json!("0123456789abcdef0123456789abcdef01234567");
        let failed_forged_commit = fiber_node_execution_summary(&repo_root, Some(&forged_commit));
        assert!(!json_pointer_bool(&failed_forged_commit, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_forged_commit, "/checks/fiber_repo_git_provenance_verified"));
        assert!(!json_pointer_bool(&failed_forged_commit, "/fiber_repo_git_provenance/checks/commit_matches_report"));

        let mut stale_execution_commit = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        stale_execution_commit["workflows"][0]["execution"]["fiber_repo"]["commit"] =
            json!("0123456789abcdef0123456789abcdef01234567");
        let failed_stale_execution_commit = fiber_node_execution_summary(&repo_root, Some(&stale_execution_commit));
        assert!(!json_pointer_bool(&failed_stale_execution_commit, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(
            &failed_stale_execution_commit,
            "/workflow_checks/open-use-close-a-channel/execution_fiber_repo_matches_report"
        ));

        let mut missing_execution_repo = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        missing_execution_repo["workflows"][0]["execution"].as_object_mut().unwrap().remove("fiber_repo");
        let failed_missing_execution_repo = fiber_node_execution_summary(&repo_root, Some(&missing_execution_repo));
        assert!(!json_pointer_bool(&failed_missing_execution_repo, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(
            &failed_missing_execution_repo,
            "/workflow_checks/open-use-close-a-channel/execution_fiber_repo_matches_report"
        ));

        let mut missing_logs = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        missing_logs["workflows"][0]["execution"]["stdout_log"] = Value::String(String::new());
        let failed_logs = fiber_node_execution_summary(&repo_root, Some(&missing_logs));
        assert!(!json_pointer_bool(&failed_logs, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_logs, "/workflow_checks/open-use-close-a-channel/execution_logs_present"));

        let mut assumed_nodes_running = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        assumed_nodes_running["workflows"][0]["execution"]["started_node"] = Value::Bool(false);
        let failed_assumed_nodes_running = fiber_node_execution_summary(&repo_root, Some(&assumed_nodes_running));
        assert!(!json_pointer_bool(&failed_assumed_nodes_running, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_assumed_nodes_running, "/workflow_checks/open-use-close-a-channel/execution_started_node"));

        let mut wrong_command = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        wrong_command["workflows"][0]["execution"]["command"] =
            json!(["npm", "exec", "--", "@usebruno/cli", "run", "e2e/invoice-ops", "-r", "--env", "test"]);
        let failed_wrong_command = fiber_node_execution_summary(&repo_root, Some(&wrong_command));
        assert!(!json_pointer_bool(&failed_wrong_command, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_wrong_command, "/workflow_checks/open-use-close-a-channel/execution_command_exact"));

        let mut nonzero_returncode = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        nonzero_returncode["workflows"][0]["execution"]["returncode"] = json!(1);
        let failed_nonzero_returncode = fiber_node_execution_summary(&repo_root, Some(&nonzero_returncode));
        assert!(!json_pointer_bool(&failed_nonzero_returncode, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_nonzero_returncode, "/workflow_checks/open-use-close-a-channel/execution_returncode_zero"));

        let mut zero_duration = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        zero_duration["workflows"][0]["execution"]["duration_seconds"] = json!(0.0);
        let failed_zero_duration = fiber_node_execution_summary(&repo_root, Some(&zero_duration));
        assert!(!json_pointer_bool(&failed_zero_duration, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_zero_duration, "/workflow_checks/open-use-close-a-channel/execution_duration_positive"));

        let mut missing_evidence_file = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        missing_evidence_file["workflows"][0]["evidence_files"] = json!(["tests/bruno/e2e/open-use-close-a-channel/missing.bru"]);
        let failed_evidence = fiber_node_execution_summary(&repo_root, Some(&missing_evidence_file));
        assert!(!json_pointer_bool(&failed_evidence, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_evidence, "/workflow_checks/open-use-close-a-channel/evidence_files_exist"));

        let mut missing_patch_file = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        let cross_chain_hub = missing_patch_file["workflows"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|workflow| json_pointer_str(workflow, "/suite") == Some("cross-chain-hub"))
            .unwrap();
        cross_chain_hub["execution"]["bruno_compatibility_patches"] = json!(["e2e/cross-chain-hub/missing-patch.bru"]);
        let failed_patch = fiber_node_execution_summary(&repo_root, Some(&missing_patch_file));
        assert!(!json_pointer_bool(&failed_patch, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_patch, "/workflow_checks/cross-chain-hub/bruno_compatibility_patch_files_exist"));

        let mut unsafe_empty_patch_metadata = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        unsafe_empty_patch_metadata["workflows"][0]["execution"]["bruno_cwd"] = Value::String("/tmp/fiber/tests/bruno".to_string());
        unsafe_empty_patch_metadata["workflows"][0]["execution"]["bruno_compatibility_patches"] = json!([]);
        let failed_unsafe_empty_patch_metadata = fiber_node_execution_summary(&repo_root, Some(&unsafe_empty_patch_metadata));
        assert!(!json_pointer_bool(&failed_unsafe_empty_patch_metadata, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(
            &failed_unsafe_empty_patch_metadata,
            "/workflow_checks/open-use-close-a-channel/bruno_compatibility_patch_files_exist"
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let mut symlink_evidence = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
            let outside = temp.path().join("outside-evidence.bru");
            std::fs::write(&outside, "meta { name: outside }\n").unwrap();
            let rel_path = "tests/bruno/e2e/open-use-close-a-channel/symlink.bru";
            let symlink_path = fiber_repo.join(rel_path);
            std::fs::create_dir_all(symlink_path.parent().unwrap()).unwrap();
            symlink(&outside, &symlink_path).unwrap();
            symlink_evidence["workflows"][0]["evidence_files"] = json!([rel_path]);

            let failed_symlink = fiber_node_execution_summary(&repo_root, Some(&symlink_evidence));
            assert!(!json_pointer_bool(&failed_symlink, "/all_required_workflows_executed_passed"));
            assert!(!json_pointer_bool(&failed_symlink, "/workflow_checks/open-use-close-a-channel/evidence_files_exist"));
            std::fs::remove_file(&symlink_path).unwrap();

            let mut symlink_bruno_root = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
            let cross_chain_hub = symlink_bruno_root["workflows"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|workflow| json_pointer_str(workflow, "/suite") == Some("cross-chain-hub"))
                .unwrap();
            let bruno_cwd = json_pointer_str(cross_chain_hub, "/execution/bruno_cwd").unwrap().to_string();
            let patch = json_pointer_str(cross_chain_hub, "/execution/bruno_compatibility_patches/0").unwrap().to_string();
            let bruno_root = repo_root.join(&bruno_cwd);
            std::fs::remove_dir_all(&bruno_root).unwrap();
            let outside_bruno_root = temp.path().join("outside-bruno-root");
            let outside_patch = outside_bruno_root.join(&patch);
            std::fs::create_dir_all(outside_patch.parent().unwrap()).unwrap();
            std::fs::write(outside_patch, "meta { name: outside-patched }\n").unwrap();
            symlink(&outside_bruno_root, &bruno_root).unwrap();

            let failed_symlink_bruno_root = fiber_node_execution_summary(&repo_root, Some(&symlink_bruno_root));
            assert!(!json_pointer_bool(&failed_symlink_bruno_root, "/all_required_workflows_executed_passed"));
            assert!(!json_pointer_bool(
                &failed_symlink_bruno_root,
                "/workflow_checks/cross-chain-hub/bruno_compatibility_patch_files_exist"
            ));
        }
    }

    #[test]
    fn fiber_candidate_profile_docs_match_live_execution_boundary() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let report = validate_fiber_candidate_profile_package(repo_root).unwrap();
        assert_eq!(json_pointer_str(&report, "/status"), Some("passed"));
        assert!(json_pointer_bool(&report, "/checks/docs_fiber_execution_claim_current"));
    }

    #[test]
    fn external_endpoint_coverage_requires_public_btc_spv_evidence() {
        let ckb_live = json!({"required_live_checks_passed": true});
        let fiber = json!({
            "all_required_workflows_executed_passed": true,
            "workflow_coverage": {
                "all_required_workflows_executed": true,
                "all_required_workflows_executed_passed": true,
            },
            "fiber_repo": {
                "origin": EXPECTED_FIBER_REPO_ORIGIN,
                "commit": "0123456789abcdef0123456789abcdef01234567",
                "dirty": false,
            },
            "fiber_repo_git_provenance": {
                "verified": true,
            },
        });
        let btc_missing = json!({
            "status": "external_required",
            "reason": "missing public BTC SPV evidence",
            "required_report": PUBLIC_BTC_SPV_EVIDENCE,
            "template": PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE,
        });

        let coverage = external_endpoint_coverage_summary(&btc_missing, &fiber, &ckb_live, &ckb_live, &ckb_live, &ckb_live);

        assert_eq!(json_pointer_str(&coverage, "/status"), Some("external_required"));
        assert_eq!(json_pointer_str(&coverage, "/btc/status"), Some("external_required"));
        assert_eq!(json_pointer_str(&coverage, "/fiber/status"), Some("passed"));
        assert!(!json_pointer_bool(&coverage, "/production_complete"));
        assert!(json_pointer_bool(&coverage, "/checks/btc_ckb_lifecycle_passed"));
        assert!(!json_pointer_bool(&coverage, "/checks/public_btc_spv_evidence_passed"));
        assert!(json_pointer_bool(&coverage, "/checks/fiber_git_provenance_verified"));
        assert!(json_pointer_bool(&coverage, "/checks/fiber_node_workflows_passed"));

        let ckb_missing = json!({"required_live_checks_passed": false});
        let failed_local_btc = external_endpoint_coverage_summary(&btc_missing, &fiber, &ckb_missing, &ckb_live, &ckb_live, &ckb_live);
        assert_eq!(json_pointer_str(&failed_local_btc, "/status"), Some("failed"));
        assert_eq!(json_pointer_str(&failed_local_btc, "/btc/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_local_btc, "/checks/btc_ckb_lifecycle_passed"));
        assert!(!json_pointer_bool(&failed_local_btc, "/production_complete"));

        let mut missing_fiber_provenance = fiber;
        missing_fiber_provenance.as_object_mut().unwrap().remove("fiber_repo_git_provenance");
        let failed_fiber =
            external_endpoint_coverage_summary(&btc_missing, &missing_fiber_provenance, &ckb_live, &ckb_live, &ckb_live, &ckb_live);
        assert_eq!(json_pointer_str(&failed_fiber, "/fiber/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_fiber, "/checks/fiber_git_provenance_verified"));
        assert!(!json_pointer_bool(&failed_fiber, "/checks/fiber_node_workflows_passed"));
    }

    #[test]
    fn all_profile_production_matrix_keeps_btc_and_fiber_evidence_separate() {
        let profile_certification = json!({
            "local_checks": {
                "conformance_gate_passed": true,
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
                "checks": {
                    "core_profile_live_stateful": true,
                    "agreement_profile_live_stateful": true,
                }
            },
            "business_scenario_coverage": {
                "checks": {
                    "btc_transaction_commitment_transition_live": true,
                    "btc_utxo_seal_closure_live": true,
                    "dual_seal_finality_live": true,
                    "fiber_candidate_path_live": true,
                    "fungible_xudt_value_flow_live": true,
                    "rwa_receipt_lifecycle_live": true,
                }
            },
            "external_endpoint_coverage": {
                "fiber": { "status": "passed" }
            },
        });
        let passed = json!({"status": "passed"});
        let btc_missing = json!({"status": "external_required"});

        let matrix = build_profile_production_completeness(
            &profile_certification,
            &stateful_acceptance,
            &passed,
            &passed,
            &btc_missing,
            &passed,
        );
        let profile_status = |profile: &str| {
            matrix
                .get("profiles")
                .and_then(Value::as_array)
                .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/profile") == Some(profile)))
                .and_then(|row| json_pointer_str(row, "/status"))
                .unwrap_or("missing")
        };

        assert_eq!(json_pointer_str(&matrix, "/status"), Some("external_required"));
        assert!(json_pointer_bool(&matrix, "/local_complete"));
        assert!(!json_pointer_bool(&matrix, "/production_complete"));
        assert_eq!(profile_status("fiber-candidate-profile-v0"), "passed");
        assert_eq!(profile_status("btc-transaction-commitment-profile-v0"), "external_required");
        assert_eq!(profile_status("btc-utxo-seal-profile-v0"), "external_required");
        assert_eq!(profile_status("dual-seal-profile-v0"), "external_required");
        assert_eq!(profile_status("rwa-receipt-profile-v0"), "passed");
        assert!(json_array_strings(&matrix, "/missing_external_evidence").iter().any(|item| item == "public_btc_spv_evidence"));
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
            "## Implemented Guards\npublic/shared CellDep\npublic BTC SPV\nRWA legal/registry review evidence\nexternal BIP340\n## Not Implemented\n## Risk Posture\n",
        )
        .unwrap();
        std::fs::write(
            agreement_docs.join("AUDIT_STATUS.md"),
            "## Claim Classification\n## Fixture Honesty\nexternal production attestations, public BTC SPV evidence, and RWA legal/registry review evidence still required\n## Production Statement Boundary\n",
        )
        .unwrap();
        std::fs::write(core_docs.join("RISCV_VERIFIER_SHELL.md"), "## Unsafe Boundary\nsyscall register ABI only\n").unwrap();
        std::fs::write(
            riscv_src.join("main.rs"),
            "// SAFETY: test syscall boundary\nunsafe {\n}\n// SAFETY: second syscall boundary\nunsafe {\n}\n",
        )
        .unwrap();
        let repo_commit = ensure_clean_fixture_repo(temp.path());
        let tcb_source = source_tree_hash_with_options(temp.path(), BIP340_TCB_SOURCE_HASH_PATHS, true).unwrap();
        let tcb_files = json_array_strings(&tcb_source, "/files").into_iter().map(|path| json!({ "path": path })).collect::<Vec<_>>();

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
            "repo_commit": repo_commit,
            "source_inventory": {
                "source_tree_sha256": json_pointer_str(&tcb_source, "/sha256"),
                "total_files": json_pointer_i64(&tcb_source, "/file_count"),
                "files": tcb_files,
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
    fn bip340_tcb_source_inventory_must_match_current_safe_tree() {
        let temp = tempfile::tempdir().unwrap();
        let riscv_src = temp.path().join(CORE_ROOT).join("verifier/novaseal_btc_verifier_riscv/src");
        std::fs::create_dir_all(&riscv_src).unwrap();
        std::fs::write(riscv_src.join("main.rs"), "fn main() {}\n").unwrap();
        let repo_commit = ensure_clean_fixture_repo(temp.path());
        let source = source_tree_hash_with_options(temp.path(), BIP340_TCB_SOURCE_HASH_PATHS, true).unwrap();
        let files = json_array_strings(&source, "/files").into_iter().map(|path| json!({ "path": path })).collect::<Vec<_>>();
        let tcb = json!({
            "repo_commit": repo_commit,
            "source_inventory": {
                "source_tree_sha256": json_pointer_str(&source, "/sha256"),
                "total_files": json_pointer_i64(&source, "/file_count"),
                "files": files,
            },
        });

        let passed = validate_bip340_tcb_source_inventory(temp.path(), &tcb).unwrap();
        assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));

        let mut stale_hash = tcb.clone();
        stale_hash["source_inventory"]["source_tree_sha256"] = json!(format!("0x{}", "00".repeat(32)));
        let failed_hash = validate_bip340_tcb_source_inventory(temp.path(), &stale_hash).unwrap();
        assert_eq!(json_pointer_str(&failed_hash, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_hash, "/checks/source_tree_sha256_matches_current"));

        let mut stale_commit = tcb.clone();
        stale_commit["repo_commit"] = json!("0000000000000000000000000000000000000000");
        let failed_commit = validate_bip340_tcb_source_inventory(temp.path(), &stale_commit).unwrap();
        assert_eq!(json_pointer_str(&failed_commit, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_commit, "/checks/repo_commit_matches_current_head"));

        let mut missing_commit = tcb.clone();
        missing_commit.as_object_mut().unwrap().remove("repo_commit");
        let failed_missing_commit = validate_bip340_tcb_source_inventory(temp.path(), &missing_commit).unwrap();
        assert_eq!(json_pointer_str(&failed_missing_commit, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_missing_commit, "/checks/repo_commit_present"));

        let mut missing_file = tcb;
        missing_file["source_inventory"]["files"] = json!([]);
        let failed_files = validate_bip340_tcb_source_inventory(temp.path(), &missing_file).unwrap();
        assert_eq!(json_pointer_str(&failed_files, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed_files, "/checks/source_tree_file_list_matches_current"));
    }

    #[cfg(unix)]
    #[test]
    fn bip340_tcb_source_inventory_rejects_symlinked_source() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let riscv_src = temp.path().join(CORE_ROOT).join("verifier/novaseal_btc_verifier_riscv/src");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&riscv_src).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(riscv_src.join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(outside.join("linked.rs"), "pub fn outside() {}\n").unwrap();
        symlink(outside.join("linked.rs"), riscv_src.join("linked.rs")).unwrap();
        let source = source_tree_hash_with_options(temp.path(), BIP340_TCB_SOURCE_HASH_PATHS, true).unwrap();
        let tcb = json!({
            "source_inventory": {
                "source_tree_sha256": json_pointer_str(&source, "/sha256"),
                "total_files": json_pointer_i64(&source, "/file_count"),
                "files": json_array_strings(&source, "/files")
                    .into_iter()
                    .map(|path| json!({ "path": path }))
                    .collect::<Vec<_>>(),
            },
        });

        let failed = validate_bip340_tcb_source_inventory(temp.path(), &tcb).unwrap();

        assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
        assert!(!json_pointer_bool(&failed, "/checks/current_source_tree_valid"));
        assert!(!json_pointer_bool(&failed, "/checks/source_tree_invalid_paths_empty"));
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
                "wallet_lock_alignment_passed": true,
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
    fn production_status_requires_statement_eligibility_even_when_gates_pass() {
        assert_eq!(production_gate_status(true, true, true, false), "production_ready");
        assert_eq!(production_gate_status(false, true, true, false), "production_statement_ineligible");
        assert_eq!(production_gate_status(false, false, true, true), "local_production_prep_ready_external_attestation_required");
        assert_eq!(production_gate_status(false, false, false, true), "failed");
    }

    #[test]
    fn v1_readiness_rejects_production_claim_when_statement_ineligible() {
        let profile_certification = json!({
            "status": "passed",
            "production_statement_eligible": false,
            "production_statement_blockers": ["manual_production_statement_missing"],
            "local_checks": {
                "conformance_gate_passed": true,
                "wallet_vector_detail_passed": true,
                "wallet_lock_alignment_passed": true,
                "profile_operator_fixture_detail_passed": true,
                "service_builder_fixture_detail_passed": true,
                "btc_spv_evidence_adapter_passed": true,
                "external_attestation_adapter_passed": true,
                "external_evidence_handoff_passed": true,
                "local_bip340_tcb_review_passed": true,
            },
            "security_audit_coverage": { "status": "passed" },
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
            "status": "passed",
            "blocker_count": 0,
            "live_devnet_rpc_executed": true,
            "stateful_lifecycle_executed": true,
            "profile_coverage": {
                "status": "passed",
                "covered_profiles": [
                    { "status": "passed" },
                    { "status": "passed" }
                ]
            },
            "business_scenario_coverage": {
                "status": "passed",
                "checks": {
                    "agreement_originate_live": true,
                    "agreement_repay_live": true,
                    "agreement_claim_live": true,
                    "agreement_negative_business_cases_preserve_live_state": true,
                    "btc_transaction_commitment_transition_live": true,
                    "btc_utxo_seal_closure_live": true,
                    "dual_seal_finality_live": true,
                    "fungible_xudt_value_flow_live": true,
                    "rwa_receipt_lifecycle_live": true,
                    "fiber_candidate_path_live": true
                }
            },
            "external_endpoint_coverage": {
                "status": "passed",
                "production_complete": true
            },
        });
        let gates = vec![
            gate(
                "external_btc_fiber_endpoint_acceptance",
                "passed",
                "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage",
                Value::Null,
            ),
            gate(
                "all_profiles_production_completeness",
                "passed",
                "target/novaseal-production-gates.json#/profile_production_completeness",
                Value::Null,
            ),
            gate("public_shared_cell_dep_pinning_attestation", "passed", PUBLIC_CELLDEP_ATTESTATION, Value::Null),
            gate("external_bip340_runtime_verifier_tcb_review_attestation", "passed", EXTERNAL_TCB_ATTESTATION, Value::Null),
            gate("public_btc_spv_evidence", "passed", PUBLIC_BTC_SPV_EVIDENCE, Value::Null),
            gate("rwa_legal_registry_review_evidence", "passed", RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, Value::Null),
        ];

        let readiness = build_v1_readiness(&profile_certification, &stateful_acceptance, &gates, true, true);

        assert_eq!(json_pointer_str(&readiness, "/status"), Some("production_statement_ineligible"));
        assert!(json_pointer_bool(&readiness, "/local_v1_ready"));
        assert!(!json_pointer_bool(&readiness, "/production_ready"));
        assert!(json_pointer_bool(&readiness, "/production_gates_passed"));
        assert!(!json_pointer_bool(&readiness, "/production_statement_eligible"));
        assert!(json_array_strings(&readiness, "/failed_dimensions").is_empty());
        assert_eq!(json_array_strings(&readiness, "/external_blockers"), vec!["manual_production_statement_missing".to_string()]);
    }

    #[test]
    fn v1_readiness_allows_local_ready_when_only_external_endpoint_evidence_remains() {
        let profile_certification = json!({
            "status": "passed",
            "production_statement_eligible": false,
            "production_statement_blockers": [
                "public_shared_cell_dep_attested",
                "external_bip340_tcb_review_attested",
                "public_btc_spv_evidence_attested",
                "rwa_legal_registry_review_attested"
            ],
            "local_checks": {
                "conformance_gate_passed": true,
                "wallet_vector_detail_passed": true,
                "wallet_lock_alignment_passed": true,
                "profile_operator_fixture_detail_passed": true,
                "service_builder_fixture_detail_passed": true,
                "btc_spv_evidence_adapter_passed": true,
                "external_attestation_adapter_passed": true,
                "external_evidence_handoff_passed": true,
                "local_bip340_tcb_review_passed": true,
            },
            "security_audit_coverage": { "status": "passed" },
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
            "status": "local_devnet_passed_external_endpoint_required",
            "blocker_count": 1,
            "local_blocker_count": 0,
            "acceptance_blocker_count": 1,
            "live_devnet_rpc_executed": true,
            "stateful_lifecycle_executed": true,
            "profile_coverage": {
                "status": "passed",
                "covered_profiles": [
                    { "status": "passed" },
                    { "status": "passed" }
                ]
            },
            "business_scenario_coverage": {
                "status": "passed",
                "checks": {
                    "agreement_originate_live": true,
                    "agreement_repay_live": true,
                    "agreement_claim_live": true,
                    "agreement_negative_business_cases_preserve_live_state": true,
                    "btc_transaction_commitment_transition_live": true,
                    "btc_utxo_seal_closure_live": true,
                    "dual_seal_finality_live": true,
                    "fungible_xudt_value_flow_live": true,
                    "rwa_receipt_lifecycle_live": true,
                    "fiber_candidate_path_live": true
                }
            },
            "external_endpoint_coverage": {
                "status": "external_required",
                "production_complete": false
            },
        });
        let gates = vec![
            gate(
                "external_btc_fiber_endpoint_acceptance",
                "external_required",
                "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage",
                Value::Null,
            ),
            gate(
                "all_profiles_production_completeness",
                "external_required",
                "target/novaseal-production-gates.json#/profile_production_completeness",
                Value::Null,
            ),
            gate("public_shared_cell_dep_pinning_attestation", "external_required", PUBLIC_CELLDEP_ATTESTATION, Value::Null),
            gate(
                "external_bip340_runtime_verifier_tcb_review_attestation",
                "external_required",
                EXTERNAL_TCB_ATTESTATION,
                Value::Null,
            ),
            gate("public_btc_spv_evidence", "external_required", PUBLIC_BTC_SPV_EVIDENCE, Value::Null),
            gate("rwa_legal_registry_review_evidence", "external_required", RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, Value::Null),
        ];

        let readiness = build_v1_readiness(&profile_certification, &stateful_acceptance, &gates, true, false);

        assert_eq!(json_pointer_str(&readiness, "/status"), Some("local_v1_ready_external_attestation_required"));
        assert!(json_pointer_bool(&readiness, "/local_v1_ready"));
        assert!(!json_pointer_bool(&readiness, "/production_ready"));
        assert_eq!(json_pointer_str(&readiness, "/planned_profile_matrix/status"), Some("passed"));
        assert!(json_array_strings(&readiness, "/planned_profile_matrix/missing").is_empty());
        assert_eq!(
            json_array_strings(&readiness, "/external_blockers"),
            vec![
                "public_shared_cell_dep_attested".to_string(),
                "external_bip340_tcb_review_attested".to_string(),
                "public_btc_spv_evidence_attested".to_string(),
                "rwa_legal_registry_review_attested".to_string(),
            ]
        );
    }

    #[test]
    fn v1_readiness_requires_wallet_lock_digest_alignment_for_local_ready() {
        let profile_certification = json!({
            "status": "passed",
            "production_statement_eligible": false,
            "production_statement_blockers": [
                "public_shared_cell_dep_attested",
                "external_bip340_tcb_review_attested",
                "public_btc_spv_evidence_attested",
                "rwa_legal_registry_review_attested"
            ],
            "local_checks": {
                "conformance_gate_passed": true,
                "wallet_vector_detail_passed": true,
                "wallet_lock_alignment_passed": false,
                "profile_operator_fixture_detail_passed": true,
                "service_builder_fixture_detail_passed": true,
                "btc_spv_evidence_adapter_passed": true,
                "external_attestation_adapter_passed": true,
                "external_evidence_handoff_passed": true,
                "local_bip340_tcb_review_passed": true,
            },
            "security_audit_coverage": { "status": "passed" },
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
            "status": "local_devnet_passed_external_endpoint_required",
            "blocker_count": 1,
            "local_blocker_count": 0,
            "acceptance_blocker_count": 1,
            "live_devnet_rpc_executed": true,
            "stateful_lifecycle_executed": true,
            "profile_coverage": {
                "status": "passed",
                "covered_profiles": [
                    { "status": "passed" },
                    { "status": "passed" }
                ]
            },
            "business_scenario_coverage": {
                "status": "passed",
                "checks": {
                    "agreement_originate_live": true,
                    "agreement_repay_live": true,
                    "agreement_claim_live": true,
                    "agreement_negative_business_cases_preserve_live_state": true,
                    "btc_transaction_commitment_transition_live": true,
                    "btc_utxo_seal_closure_live": true,
                    "dual_seal_finality_live": true,
                    "fungible_xudt_value_flow_live": true,
                    "rwa_receipt_lifecycle_live": true,
                    "fiber_candidate_path_live": true
                }
            },
            "external_endpoint_coverage": {
                "status": "external_required",
                "production_complete": false
            },
        });
        let gates = vec![
            gate(
                "external_btc_fiber_endpoint_acceptance",
                "external_required",
                "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage",
                Value::Null,
            ),
            gate(
                "all_profiles_production_completeness",
                "external_required",
                "target/novaseal-production-gates.json#/profile_production_completeness",
                Value::Null,
            ),
            gate("public_shared_cell_dep_pinning_attestation", "external_required", PUBLIC_CELLDEP_ATTESTATION, Value::Null),
            gate(
                "external_bip340_runtime_verifier_tcb_review_attestation",
                "external_required",
                EXTERNAL_TCB_ATTESTATION,
                Value::Null,
            ),
            gate("public_btc_spv_evidence", "external_required", PUBLIC_BTC_SPV_EVIDENCE, Value::Null),
            gate("rwa_legal_registry_review_evidence", "external_required", RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, Value::Null),
        ];

        let readiness = build_v1_readiness(&profile_certification, &stateful_acceptance, &gates, false, false);

        assert_eq!(json_pointer_str(&readiness, "/status"), Some("planned_profiles_incomplete"));
        assert!(!json_pointer_bool(&readiness, "/local_v1_ready"));
        assert!(json_array_strings(&readiness, "/failed_dimensions")
            .iter()
            .any(|dimension| dimension == "wallet_lock_digest_alignment"));
        assert!(json_array_strings(&readiness, "/planned_profile_matrix/missing")
            .iter()
            .any(|profile| profile == "seal_profile_btc_key_signature"));
    }

    #[test]
    fn planned_matrix_counts_fungible_package_but_keeps_value_flow_missing() {
        let profile_certification = json!({
            "status": "passed",
            "production_statement_eligible": false,
            "local_checks": {
                "wallet_vector_detail_passed": true,
                "wallet_lock_alignment_passed": true,
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
                    "dual_seal_finality_live": false,
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
        let dual_seal_flow_status = matrix
            .pointer("/business_scenarios")
            .and_then(Value::as_array)
            .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("dual_seal_finality")))
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
        assert_eq!(dual_seal_profile_status, Some("missing"));
        assert_eq!(dual_seal_flow_status, Some("missing"));
        assert_eq!(fungible_profile_status, Some("passed"));
        assert_eq!(fungible_flow_status, Some("missing"));
        assert_eq!(rwa_profile_status, Some("passed"));
        assert_eq!(rwa_flow_status, Some("missing"));
        assert_eq!(fiber_profile_status, Some("passed"));
        assert_eq!(fiber_flow_status, Some("missing"));
        assert!(!missing.iter().any(|id| id == "seal_profile_btc_transaction_commitment"));
        assert!(!missing.iter().any(|id| id == "seal_profile_btc_utxo_seal"));
        assert!(missing.iter().any(|id| id == "seal_profile_dual_seal"));
        assert!(!missing.iter().any(|id| id == "object_profile_fungible_xudt"));
        assert!(!missing.iter().any(|id| id == "object_profile_rwa_receipt"));
        assert!(!missing.iter().any(|id| id == "future_fiber_test_path"));
        assert!(missing.iter().any(|id| id == "btc_transaction_commitment_transition"));
        assert!(missing.iter().any(|id| id == "btc_utxo_seal_closure"));
        assert!(missing.iter().any(|id| id == "dual_seal_finality"));
        assert!(missing.iter().any(|id| id == "fungible_xudt_value_flow"));
        assert!(missing.iter().any(|id| id == "rwa_receipt_lifecycle"));
        assert!(missing.iter().any(|id| id == "fiber_candidate_path"));
        assert_eq!(json_array_strings(&matrix, "/boundary/remaining_items"), missing);
        assert!(json_pointer_str(&matrix, "/boundary/not_implemented_yet")
            .is_some_and(|text| text.contains("btc_transaction_commitment_transition")));
    }

    #[test]
    fn planned_matrix_boundary_has_no_stale_missing_text_when_all_rows_pass() {
        let profile_certification = json!({
            "status": "passed",
            "production_statement_eligible": false,
            "local_checks": {
                "conformance_gate_passed": true,
                "wallet_vector_detail_passed": true,
                "wallet_lock_alignment_passed": true,
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
                "status": "passed",
                "checks": {
                    "agreement_originate_live": true,
                    "agreement_repay_live": true,
                    "agreement_claim_live": true,
                    "agreement_negative_business_cases_preserve_live_state": true,
                    "btc_transaction_commitment_transition_live": true,
                    "btc_utxo_seal_closure_live": true,
                    "dual_seal_finality_live": true,
                    "fungible_xudt_value_flow_live": true,
                    "rwa_receipt_lifecycle_live": true,
                    "fiber_candidate_path_live": true
                }
            },
        });

        let matrix = build_planned_profile_matrix(&profile_certification, &stateful_acceptance);
        let not_implemented_yet = json_pointer_str(&matrix, "/boundary/not_implemented_yet").unwrap_or_default();

        assert_eq!(json_pointer_str(&matrix, "/status"), Some("passed"));
        assert!(json_array_strings(&matrix, "/missing").is_empty());
        assert!(json_array_strings(&matrix, "/boundary/remaining_items").is_empty());
        assert!(not_implemented_yet.starts_with("none;"));
        assert!(!not_implemented_yet.contains("fresh live devnet reports proving"));
    }

    #[test]
    fn tx_hash_value_is_real_rejects_placeholder_and_accepts_real_hash() {
        let real_hash = test_hex32(0xab);
        assert!(tx_hash_value_is_real(&Value::String(real_hash.clone())));
        assert!(!tx_hash_value_is_real(&Value::String("0x".to_string() + &"00".repeat(32))));
        assert!(!tx_hash_value_is_real(&Value::Null));
        assert!(!tx_hash_value_is_real(&Value::String("not-a-hash".to_string())));
    }

    #[test]
    fn is_real_tx_hash_rejects_zero_hash_placeholder() {
        let real_hash = test_hex32(0x01);
        assert!(is_real_tx_hash(&real_hash));
        assert!(!is_real_tx_hash(&format!("0x{}", "00".repeat(32))));
        assert!(!is_real_tx_hash("0xdead"));
        assert!(!is_real_tx_hash("not-even-hex"));
    }

    #[test]
    fn btc_dual_seal_binding_requires_exact_prevout_and_sealed_tuple() {
        let btc = test_btc_profile_material(EXPECTED_DUAL_SEAL_PROFILE, 0x44);
        let valid_case = json!({
            "btc_txid": btc.txid.clone(),
            "btc_wtxid": btc.wtxid.clone(),
            "btc_tx_hex": btc.tx_hex.clone(),
            "btc_transaction_binding": btc.binding.clone(),
        });
        let valid_checks = validate_btc_transaction_binding(EXPECTED_DUAL_SEAL_PROFILE, &valid_case, &btc.expected_binding);
        assert!(json_pointer_bool(&valid_checks, "/btc_txid_matches_tx_hex"));
        assert!(json_pointer_bool(&valid_checks, "/dual_spend_input_matches_anchor"));
        assert!(json_pointer_bool(&valid_checks, "/dual_sealed_tx_matches_anchor"));
        assert!(json_pointer_bool(&valid_checks, "/dual_sealed_utxo_commitment_matches_tuple"));

        let wrong_prevout_tx_hex = test_bitcoin_tx_hex(&[(test_hex32(0xa1), 1)], &[(31_000, vec![0x51, 0x44])]);
        let wrong_prevout_tx = parse_bitcoin_tx_hex(&wrong_prevout_tx_hex).unwrap();
        let wrong_prevout_case = json!({
            "btc_txid": wrong_prevout_tx.txid,
            "btc_wtxid": wrong_prevout_tx.wtxid,
            "btc_tx_hex": wrong_prevout_tx_hex,
            "btc_transaction_binding": valid_case["btc_transaction_binding"].clone(),
        });
        let wrong_prevout_checks =
            validate_btc_transaction_binding(EXPECTED_DUAL_SEAL_PROFILE, &wrong_prevout_case, &btc.expected_binding);
        assert!(json_pointer_bool(&wrong_prevout_checks, "/btc_txid_matches_tx_hex"));
        assert!(!json_pointer_bool(&wrong_prevout_checks, "/dual_spend_input_matches_anchor"));

        let mut stale_commitment_binding = valid_case["btc_transaction_binding"].clone();
        stale_commitment_binding["sealed_utxo_commitment_hash"] = json!(test_hex32(0xa2));
        let stale_commitment_case = json!({
            "btc_txid": json_pointer_str(&valid_case, "/btc_txid").unwrap(),
            "btc_wtxid": json_pointer_str(&valid_case, "/btc_wtxid").unwrap(),
            "btc_tx_hex": json_pointer_str(&valid_case, "/btc_tx_hex").unwrap(),
            "btc_transaction_binding": stale_commitment_binding,
        });
        let stale_commitment_checks =
            validate_btc_transaction_binding(EXPECTED_DUAL_SEAL_PROFILE, &stale_commitment_case, &btc.expected_binding);
        assert!(!json_pointer_bool(&stale_commitment_checks, "/binding_matches_handoff"));
        assert!(json_pointer_bool(&stale_commitment_checks, "/dual_sealed_tx_matches_anchor"));
        assert!(!json_pointer_bool(&stale_commitment_checks, "/dual_sealed_utxo_commitment_matches_tuple"));
    }

    #[test]
    fn btc_transaction_parser_rejects_noncanonical_compact_size_encodings() {
        let prevout = test_hex32(0x42);
        let mut input_count_noncanonical = test_bitcoin_tx_bytes(&[(prevout.clone(), 0)], &[(10_000, vec![0x51])]);
        input_count_noncanonical.splice(4..5, [0xfd, 0x01, 0x00]);
        assert!(
            parse_bitcoin_tx(&input_count_noncanonical).is_none(),
            "BTC evidence parser must reject non-minimal input-count CompactSize"
        );

        let mut script_len_noncanonical = Vec::new();
        script_len_noncanonical.extend_from_slice(&2u32.to_le_bytes());
        push_test_compact_size(&mut script_len_noncanonical, 1);
        script_len_noncanonical.extend_from_slice(&bitcoin_internal_hash_from_display(&prevout).unwrap());
        script_len_noncanonical.extend_from_slice(&0u32.to_le_bytes());
        script_len_noncanonical.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
        push_test_compact_size(&mut script_len_noncanonical, 1);
        script_len_noncanonical.extend_from_slice(&10_000u64.to_le_bytes());
        script_len_noncanonical.push(0xfd);
        script_len_noncanonical.extend_from_slice(&1u16.to_le_bytes());
        script_len_noncanonical.push(0x51);
        script_len_noncanonical.extend_from_slice(&0u32.to_le_bytes());
        assert!(
            parse_bitcoin_tx(&script_len_noncanonical).is_none(),
            "BTC evidence parser must reject non-minimal script length CompactSize"
        );
    }

    #[test]
    fn btc_transaction_parser_rejects_unknown_segwit_flags() {
        let prevout = test_hex32(0x43);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&[0x00, 0x02]);
        push_test_compact_size(&mut bytes, 1);
        bytes.extend_from_slice(&bitcoin_internal_hash_from_display(&prevout).unwrap());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        push_test_compact_size(&mut bytes, 0);
        bytes.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
        push_test_compact_size(&mut bytes, 1);
        bytes.extend_from_slice(&10_000u64.to_le_bytes());
        push_test_compact_size(&mut bytes, 1);
        bytes.push(0x51);
        push_test_compact_size(&mut bytes, 0);
        bytes.extend_from_slice(&0u32.to_le_bytes());

        assert!(parse_bitcoin_tx(&bytes).is_none(), "unsupported witness flags must not be accepted as public BTC evidence");
    }

    #[test]
    fn external_identity_rejects_local_only_names() {
        assert!(is_external_identity("NervosExternalAuditLtd"));
        for identity in [
            "REPLACE_WITH_EXTERNAL_REVIEWER",
            "unknown-reviewer",
            "NovaSeal Release Bot",
            "CellScript Team",
            "a19q3 Release Desk",
            "A19Q3 External Evidence",
            "self-attested-reviewer",
            "first-party-spv-service",
            "local-reviewer",
            "devnet-provider",
            "regtest-spv-service",
            "simnet-attestor",
            "fake-registry",
            "internal-review-team",
            "mock-custodian",
        ] {
            assert!(!is_external_identity(identity), "{identity}");
        }
    }

    #[test]
    fn report_uri_must_use_public_https_host() {
        assert!(is_https_report_uri("https://audits.nervos.org/novaseal-bip340-tcb-review"));
        assert!(is_https_report_uri("https://audits.nervos.org:443/novaseal-bip340-tcb-review"));
        assert!(is_https_report_uri("https://audits.nervos.org?report=novaseal"));

        for uri in [
            "http://audits.nervos.org/novaseal-bip340-tcb-review",
            "https://localhost/novaseal-bip340-tcb-review",
            "https://127.0.0.1/novaseal-bip340-tcb-review",
            "https://10.0.0.7/novaseal-bip340-tcb-review",
            "https://100.64.0.1/novaseal-bip340-tcb-review",
            "https://169.254.1.1/novaseal-bip340-tcb-review",
            "https://172.20.0.1/novaseal-bip340-tcb-review",
            "https://192.168.1.1/novaseal-bip340-tcb-review",
            "https://192.0.2.1/novaseal-bip340-tcb-review",
            "https://198.18.0.1/novaseal-bip340-tcb-review",
            "https://198.51.100.1/novaseal-bip340-tcb-review",
            "https://203.0.113.1/novaseal-bip340-tcb-review",
            "https://[::1]/novaseal-bip340-tcb-review",
            "https://[fc00::1]/novaseal-bip340-tcb-review",
            "https://[fe80::1]/novaseal-bip340-tcb-review",
            "https://[2001:db8::1]/novaseal-bip340-tcb-review",
            "https://audits.nervos.org:0/novaseal-bip340-tcb-review",
            "https://audits.nervos.org:65536/novaseal-bip340-tcb-review",
            "https://reviewer@audits.nervos.org/novaseal-bip340-tcb-review",
            "https://audits.nervos.local/novaseal-bip340-tcb-review",
            "https://audits.nervos.test/novaseal-bip340-tcb-review",
            "https://123.456/novaseal-bip340-tcb-review",
        ] {
            assert!(!is_https_report_uri(uri), "{uri}");
        }
    }

    #[test]
    fn planned_profile_package_cannot_pass_without_live_lifecycle_when_business_scenario_requires_it() {
        let stateful = json!({
            "status": "passed",
            "blocker_count": 0,
            "live_devnet_rpc_executed": true,
            "stateful_lifecycle_executed": true,
            "profile_coverage": {"status": "passed", "covered_profiles": []},
            "business_scenario_coverage": {"status": "passed", "checks": {}},
        });
        let profile_cert = json!({
            "status": "passed",
            "local_checks": {"conformance_gate_passed": true},
            "planned_profile_packages": {"fungible_xudt": {"status": "passed"}},
        });

        let matrix = build_planned_profile_matrix(&profile_cert, &stateful);
        assert_eq!(json_pointer_str(&matrix, "/status"), Some("incomplete"));
        let xudt_profile_row = matrix["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| json_pointer_str(row, "/id") == Some("object_profile_fungible_xudt"))
            .unwrap();
        assert_eq!(json_pointer_str(xudt_profile_row, "/status"), Some("passed"));
        let xudt_scenario_row = matrix["business_scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| json_pointer_str(row, "/id") == Some("fungible_xudt_value_flow"))
            .unwrap();
        assert_eq!(json_pointer_str(xudt_scenario_row, "/status"), Some("missing"));
    }
}
