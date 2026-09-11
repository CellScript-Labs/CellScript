//! Frozen inventory validation for the 0.30 business-capability corpus.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MANIFEST: &str = "tests/fixtures/business_corpus.json";
const SCHEMA: &str = "cellscript-business-corpus-v1";
const CRYPTO_MATRIX: &str = "tests/fixtures/cryptographic_capability_matrix.json";
const CRYPTO_MATRIX_SCHEMA: &str = "cellscript-cryptographic-capability-matrix-v1";
const CRYPTO_MATRIX_DOCUMENTATION: &str = "docs/CELLSCRIPT_0_30_CRYPTOGRAPHIC_CAPABILITY_MATRIX.md";
const CRYPTO_RESOURCE_BUDGETS: &str = "tests/fixtures/cryptographic_resource_budgets.json";
const CRYPTO_RESOURCE_BUDGETS_SCHEMA: &str = "cellscript-cryptographic-resource-budgets-v1";
const REQUIRED_FAMILIES: [&str; 8] = [
    "authorization",
    "committed_state",
    "external_verifier",
    "fungible_asset",
    "multi_script_composition",
    "nft_dob",
    "order_amm",
    "temporal",
];
const REQUIRED_LAYERS: [&str; 10] = [
    "builder_signing",
    "ckb_vm",
    "deployment_identity",
    "independent_review",
    "measurements",
    "metadata_checker",
    "node_admission",
    "parser_type_ir",
    "simulator",
    "stateful",
];
const REQUIRED_ANCHOR_CAPABILITIES: [&str; 7] = [
    "authenticated_cell_dep",
    "bounded_group_inputs",
    "bounded_output_plan",
    "fungible_conservation",
    "lock_authorization",
    "multiple_type_and_lock_groups",
    "persistent_multi_action_policy",
];
const REQUIRED_RELEASE_GATES: [&str; 7] = ["backend", "ci", "deployment", "dev", "independent_review", "node_admission", "release"];
const REQUIRED_CRYPTO_CAPABILITIES: [&str; 10] = [
    "canonical-script-hash",
    "ckb-blake2b-256",
    "exact-bip340-verifier",
    "exact-script-handle",
    "raw-transaction-hash",
    "sha256-and-sha256d",
    "sha256d-merkle-opening",
    "standard-multisig-lock",
    "trusted-external-delegation",
    "zero-lock-sighash-all",
];
const REQUIRED_CRYPTO_DOMAINS: [&str; 12] = [
    "address",
    "authenticated-opening",
    "bip340-public-key-encoding",
    "bip340-signature-encoding",
    "bounded-witness-bytes",
    "commitment-root",
    "exact-script-handle",
    "raw-hash",
    "script",
    "script-hash",
    "sighash-all-digest",
    "verification-result",
];
const REQUIRED_CRYPTO_RELEASE_GATES: [&str; 3] = ["independent_review", "release_gate", "selected_network_deployment"];
const REQUIRED_CRYPTO_RESOURCE_PROFILES: [&str; 8] = [
    "bip340-verifier-fixed-envelope",
    "bounded-witness-blake2b-max-bytes",
    "canonical-script-hash-max-args",
    "exact-script-handle-fixed-receipt",
    "sha256d-merkle-max-depth",
    "standard-multisig-lifecycle",
    "trusted-external-fixed-exec",
    "zero-lock-sighash-max-shape",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    schema: String,
    status: String,
    claim: String,
    families: Vec<Family>,
    anchor: Anchor,
    capability_ledger: String,
    capability_ledger_sha256: String,
    scenario_evidence_manifest: String,
    scenario_evidence_sha256: String,
    cryptographic_capability_matrix: String,
    release_requirements: BTreeMap<String, String>,
    #[serde(default)]
    evidence_files: Vec<String>,
    #[serde(default)]
    inventory_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CryptographicCapabilityMatrix {
    schema: String,
    status: String,
    scope: String,
    claim: String,
    documentation: String,
    resource_budget_manifest: String,
    domains: Vec<CryptographicDomain>,
    capabilities: Vec<CryptographicCapability>,
    deferred: Vec<String>,
    release_requirements: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CryptographicDomain {
    id: String,
    source_type: String,
    enforcement: String,
    evidence: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CryptographicCapability {
    id: String,
    classification: String,
    status: String,
    apis: Vec<String>,
    algorithms: Vec<String>,
    input_domains: Vec<String>,
    output_domain: String,
    bounds: Vec<String>,
    failure_codes: Vec<u16>,
    witness_owner: String,
    families: Vec<String>,
    ckb_vm_evidence: Vec<String>,
    checker_evidence: Vec<String>,
    measurement_status: String,
    resource_profiles: Vec<String>,
    measurement_evidence: Vec<String>,
    dependency_identity_evidence: Vec<String>,
    boundary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CryptographicResourceBudgets {
    schema: String,
    measurement_backend: String,
    profiles: Vec<CryptographicResourceProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CryptographicResourceProfile {
    id: String,
    status: String,
    capabilities: Vec<String>,
    maximum_shape: Vec<String>,
    evidence: Vec<String>,
    measurement_policy: String,
    measured: Option<ResourceMetrics>,
    budgets: ResourceMetrics,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ResourceMetrics {
    cycles: u64,
    elf_bytes: u64,
    max_stack_frame_bytes: u64,
    witness_bytes: u64,
    transaction_bytes: u64,
    dependency_bytes: u64,
}

fn report_u64(value: &Value, pointer: &str) -> Result<u64> {
    value.pointer(pointer).and_then(Value::as_u64).with_context(|| format!("NovaSeal resource report is missing integer {pointer}"))
}

fn require_report_flag(value: &Value, pointer: &str) -> Result<()> {
    if value.pointer(pointer).and_then(Value::as_bool) != Some(true) {
        bail!("NovaSeal resource report requires {pointer}=true");
    }
    Ok(())
}

fn validate_report_digest(value: &Value, pointer: &str) -> Result<()> {
    let digest =
        value.pointer(pointer).and_then(Value::as_str).with_context(|| format!("NovaSeal resource report is missing {pointer}"))?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("NovaSeal resource report {pointer} must be a lowercase or uppercase 32-byte hexadecimal digest");
    }
    Ok(())
}

fn validate_report_artifact(report: &Value, key: &str, report_path: &Path) -> Result<u64> {
    let logical = report
        .pointer(&format!("/{key}/path"))
        .and_then(Value::as_str)
        .with_context(|| format!("NovaSeal resource report is missing /{key}/path"))?;
    let logical_path = Path::new(logical);
    if logical_path.is_absolute() || logical_path.components().any(|component| !matches!(component, Component::Normal(_))) {
        bail!("NovaSeal resource report /{key}/path must be a normalized package-relative path");
    }
    let package_root = report_path
        .parent()
        .and_then(Path::parent)
        .context("NovaSeal resource report must be located under the package target directory")?;
    let artifact =
        fs::read(package_root.join(logical_path)).with_context(|| format!("failed to read reproduced NovaSeal artifact {logical}"))?;
    let actual_size = artifact.len() as u64;
    let recorded_size = report_u64(report, &format!("/{key}/size_bytes"))?;
    if actual_size != recorded_size {
        bail!("NovaSeal resource report /{key}/size_bytes is stale: recorded {recorded_size}, reproduced {actual_size}");
    }
    let recorded_sha256 =
        report.pointer(&format!("/{key}/sha256")).and_then(Value::as_str).context("validated report artifact digest must exist")?;
    let actual_sha256 = hex::encode(Sha256::digest(&artifact));
    if actual_sha256 != recorded_sha256.to_ascii_lowercase() {
        bail!("NovaSeal resource report /{key}/sha256 does not match the reproduced artifact");
    }
    Ok(actual_size)
}

/// Validates the release-reproduced parent-lock/child-verifier resource shape
/// against the exact BIP340 profile recorded in the frozen 0.30 corpus.
pub fn validate_novaseal_resource_report(root: &Path, report_path: &Path, lowering_path: &Path) -> Result<()> {
    let budget_bytes =
        fs::read(root.join(CRYPTO_RESOURCE_BUDGETS)).with_context(|| format!("failed to read {CRYPTO_RESOURCE_BUDGETS}"))?;
    let budgets: CryptographicResourceBudgets =
        serde_json::from_slice(&budget_bytes).with_context(|| format!("invalid {CRYPTO_RESOURCE_BUDGETS}"))?;
    let profile = budgets
        .profiles
        .iter()
        .find(|profile| profile.id == "bip340-verifier-fixed-envelope")
        .context("cryptographic resource budgets omit the BIP340 profile")?;
    if profile.status != "passed" || profile.measurement_policy != "exact" {
        bail!("the BIP340 resource profile must be passed with exact measurement policy");
    }
    let measured = profile.measured.as_ref().context("the passed BIP340 resource profile has no measurements")?;

    let report: Value = serde_json::from_slice(
        &fs::read(report_path).with_context(|| format!("failed to read NovaSeal resource report {}", report_path.display()))?,
    )
    .with_context(|| format!("invalid NovaSeal resource report {}", report_path.display()))?;
    if report["schema"] != "novaseal-parent-lock-ckb-vm-report-v0.1" {
        bail!("NovaSeal resource report has an unexpected schema");
    }
    for pointer in [
        "/summary/parent_lock_ckb_vm_executed",
        "/summary/parent_spawn_executed",
        "/summary/child_verifier_ckb_vm_executed",
        "/summary/consensus_packed_tx_constructed",
        "/summary/resolved_script_verifier_executed",
        "/summary/resolved_script_verifier_matched_expected",
        "/summary/full_transaction_executed",
        "/summary/full_transaction_verifier_matched_expected",
    ] {
        require_report_flag(&report, pointer)?;
    }
    let total_cases = report_u64(&report, "/summary/total_cases")?;
    if total_cases != 4
        || report_u64(&report, "/summary/matched_expected")? != total_cases
        || report_u64(&report, "/summary/mismatched")? != 0
        || report_u64(&report, "/summary/expected_accept")? != 1
        || report_u64(&report, "/summary/expected_reject")? != 3
    {
        bail!("NovaSeal resource report does not contain the complete four-case parent-lock matrix");
    }
    validate_report_digest(&report, "/parent_elf/sha256")?;
    validate_report_digest(&report, "/child_elf/sha256")?;
    let parent_elf_bytes = validate_report_artifact(&report, "parent_elf", report_path)?;
    let child_elf_bytes = validate_report_artifact(&report, "child_elf", report_path)?;

    let cases = report["cases"].as_array().context("NovaSeal resource report cases must be an array")?;
    let case_ids = cases.iter().filter_map(|case| case["id"].as_str()).collect::<BTreeSet<_>>();
    let required_case_ids = [
        "parent_authority_hash_mismatch_reject",
        "parent_signature_bitflip_reject",
        "parent_valid_signature_accept",
        "parent_wrong_pubkey_valid_signature_reject",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if case_ids != required_case_ids
        || cases.iter().any(|case| {
            case["matched_expected"] != true
                || case["ipc_blob_matches_expected"] != true
                || case.pointer("/transaction_shape/checks/cell_dep0_is_spawn_target") != Some(&Value::Bool(true))
                || case.pointer("/resolved_transaction/cell_dep0_data_hash_matches_child_elf") != Some(&Value::Bool(true))
                || case.pointer("/resolved_transaction/parent_lock_dep_type_hash_matches_script_code_hash") != Some(&Value::Bool(true))
        })
    {
        bail!("NovaSeal resource report does not contain the exact checked parent-lock case set");
    }
    let witness_bytes = cases
        .iter()
        .map(|case| report_u64(case, "/transaction_shape/witness_size_bytes"))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .context("NovaSeal resource report has no cases")?;
    let executed_child_cases = cases
        .iter()
        .filter(|case| case.pointer("/syscall_trace/child_verifier_runs").and_then(Value::as_u64) == Some(1))
        .collect::<Vec<_>>();
    if executed_child_cases.len() != 2
        || executed_child_cases
            .iter()
            .any(|case| case.pointer("/syscall_trace/parent_pipe_words").and_then(Value::as_array).map(Vec::len) != Some(18))
        || report_u64(&report, "/summary/pipe_write_calls")? != 36
        || report_u64(&report, "/summary/spawn_calls")? != 2
        || report_u64(&report, "/summary/wait_calls")? != 2
    {
        bail!("NovaSeal resource report does not execute two exact 18-word BIP340 IPC envelopes");
    }

    let lowering: Value = serde_json::from_slice(
        &fs::read(lowering_path).with_context(|| format!("failed to read NovaSeal parent lowering {}", lowering_path.display()))?,
    )
    .with_context(|| format!("invalid NovaSeal parent lowering {}", lowering_path.display()))?;
    if lowering["schema"] != "cellscript-verified-lowering-record-v8" {
        bail!("NovaSeal parent lowering has an unexpected schema");
    }
    if report_u64(&lowering, "/artifact_size_bytes")? != parent_elf_bytes {
        bail!("NovaSeal parent lowering artifact size does not match the reproduced parent ELF");
    }
    let parent_max_frame = lowering["entries"]
        .as_array()
        .context("NovaSeal parent lowering entries must be an array")?
        .iter()
        .map(|entry| report_u64(entry, "/frame_size_bytes"))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .context("NovaSeal parent lowering has no entries")?;
    let child_max_stack = report_u64(&report, "/summary/child_max_stack_bytes")?;
    let actual = ResourceMetrics {
        cycles: report_u64(&report, "/summary/full_transaction_verifier_max_cycles")?,
        elf_bytes: parent_elf_bytes,
        max_stack_frame_bytes: parent_max_frame.max(child_max_stack),
        witness_bytes,
        transaction_bytes: report_u64(&report, "/summary/max_consensus_tx_size_bytes")?,
        dependency_bytes: child_elf_bytes,
    };
    for (name, actual_value, expected_value) in [
        ("cycles", actual.cycles, measured.cycles),
        ("ELF bytes", actual.elf_bytes, measured.elf_bytes),
        ("stack bytes", actual.max_stack_frame_bytes, measured.max_stack_frame_bytes),
        ("witness bytes", actual.witness_bytes, measured.witness_bytes),
        ("transaction bytes", actual.transaction_bytes, measured.transaction_bytes),
        ("dependency bytes", actual.dependency_bytes, measured.dependency_bytes),
    ] {
        if actual_value != expected_value {
            bail!("NovaSeal BIP340 {name} measurement is stale: recorded {expected_value}, reproduced {actual_value}");
        }
    }
    validate_resource_metrics(&actual, &profile.budgets, "reproduced NovaSeal BIP340 resource profile")?;
    println!(
        "{}",
        serde_json::json!({
            "schema": "cellscript-novaseal-cryptographic-resource-check-v1",
            "profile": profile.id,
            "measurement_policy": profile.measurement_policy,
            "measured": actual,
            "report": report_path,
            "lowering": lowering_path,
        })
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Family {
    id: String,
    scenarios: Vec<String>,
    required_capabilities: Vec<String>,
    sources: Vec<String>,
    transaction_fixtures: Vec<String>,
    positive_cases: Vec<String>,
    adversarial_cases: Vec<String>,
    reference: ReferenceBoundary,
    evidence_layers: BTreeMap<String, EvidenceLayer>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceBoundary {
    kind: String,
    identity: String,
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceLayer {
    status: String,
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Anchor {
    id: String,
    sources: Vec<String>,
    transaction_fixture: String,
    execution_test: String,
    protocol_bundle_evidence: Vec<String>,
    builder_evidence: Vec<String>,
    required_capabilities: Vec<String>,
    positive_cases: Vec<String>,
    adversarial_cases: Vec<String>,
    artifacts: u64,
    script_groups: u64,
}

fn collect_evidence(
    corpus: &Corpus,
    matrix: &CryptographicCapabilityMatrix,
    budgets: &CryptographicResourceBudgets,
) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for family in &corpus.families {
        paths.extend(family.sources.iter().cloned());
        paths.extend(family.transaction_fixtures.iter().cloned());
        paths.extend(family.reference.paths.iter().cloned());
        for layer in family.evidence_layers.values() {
            paths.extend(layer.paths.iter().cloned());
        }
    }
    paths.extend(corpus.anchor.sources.iter().cloned());
    paths.insert(corpus.anchor.transaction_fixture.clone());
    paths.insert(corpus.anchor.execution_test.clone());
    paths.extend(corpus.anchor.protocol_bundle_evidence.iter().cloned());
    paths.extend(corpus.anchor.builder_evidence.iter().cloned());
    paths.insert(corpus.cryptographic_capability_matrix.clone());
    paths.insert(matrix.documentation.clone());
    paths.insert(matrix.resource_budget_manifest.clone());
    for domain in &matrix.domains {
        paths.extend(domain.evidence.iter().cloned());
    }
    for capability in &matrix.capabilities {
        paths.extend(capability.ckb_vm_evidence.iter().cloned());
        paths.extend(capability.checker_evidence.iter().cloned());
        paths.extend(capability.measurement_evidence.iter().cloned());
        paths.extend(capability.dependency_identity_evidence.iter().cloned());
    }
    for profile in &budgets.profiles {
        paths.extend(profile.evidence.iter().cloned());
    }
    paths
}

fn load_crypto_matrix(root: &Path, corpus: &Corpus) -> Result<CryptographicCapabilityMatrix> {
    if corpus.cryptographic_capability_matrix != CRYPTO_MATRIX {
        bail!("business corpus cryptographic capability matrix must be {CRYPTO_MATRIX}");
    }
    let bytes = fs::read(root.join(CRYPTO_MATRIX)).with_context(|| format!("failed to read {CRYPTO_MATRIX}"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid {CRYPTO_MATRIX}"))
}

fn load_crypto_resource_budgets(root: &Path, matrix: &CryptographicCapabilityMatrix) -> Result<CryptographicResourceBudgets> {
    if matrix.resource_budget_manifest != CRYPTO_RESOURCE_BUDGETS {
        bail!("cryptographic capability matrix resource budget manifest must be {CRYPTO_RESOURCE_BUDGETS}");
    }
    let bytes = fs::read(root.join(CRYPTO_RESOURCE_BUDGETS)).with_context(|| format!("failed to read {CRYPTO_RESOURCE_BUDGETS}"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid {CRYPTO_RESOURCE_BUDGETS}"))
}

fn validate_resource_metrics(measured: &ResourceMetrics, budgets: &ResourceMetrics, context: &str) -> Result<()> {
    if measured.cycles == 0 || measured.elf_bytes == 0 || measured.max_stack_frame_bytes == 0 || measured.transaction_bytes == 0 {
        bail!("{context} must record positive cycles, ELF, stack, and transaction measurements");
    }
    for (name, value, budget) in [
        ("cycles", measured.cycles, budgets.cycles),
        ("ELF bytes", measured.elf_bytes, budgets.elf_bytes),
        ("stack frame bytes", measured.max_stack_frame_bytes, budgets.max_stack_frame_bytes),
        ("witness bytes", measured.witness_bytes, budgets.witness_bytes),
        ("transaction bytes", measured.transaction_bytes, budgets.transaction_bytes),
        ("dependency bytes", measured.dependency_bytes, budgets.dependency_bytes),
    ] {
        if value > budget {
            bail!("{context} {name} measurement {value} exceeds budget {budget}");
        }
    }
    Ok(())
}

fn validate_crypto_resource_budgets(
    budgets: &CryptographicResourceBudgets,
    capability_ids: &BTreeSet<&str>,
    release: bool,
) -> Result<BTreeMap<String, String>> {
    if budgets.schema != CRYPTO_RESOURCE_BUDGETS_SCHEMA {
        bail!("cryptographic resource budget schema must be {CRYPTO_RESOURCE_BUDGETS_SCHEMA}");
    }
    if budgets.measurement_backend.trim().is_empty() {
        bail!("cryptographic resource budget measurement backend must be explicit");
    }
    let profile_ids = budgets.profiles.iter().map(|profile| profile.id.as_str()).collect::<BTreeSet<_>>();
    if profile_ids != REQUIRED_CRYPTO_RESOURCE_PROFILES.into_iter().collect()
        || budgets.profiles.len() != REQUIRED_CRYPTO_RESOURCE_PROFILES.len()
    {
        bail!("cryptographic resource budget manifest must contain every required profile exactly once");
    }
    let mut owners = BTreeMap::new();
    for profile in &budgets.profiles {
        if !matches!(profile.status.as_str(), "passed" | "release-candidate-required") {
            bail!("cryptographic resource profile {} has an invalid status", profile.id);
        }
        if !matches!(profile.measurement_policy.as_str(), "exact" | "bounded-signature-dependent-cycles") {
            bail!("cryptographic resource profile {} has an invalid measurement policy", profile.id);
        }
        validate_nonempty(&profile.capabilities, &format!("cryptographic resource profile {} capabilities", profile.id))?;
        validate_nonempty(&profile.maximum_shape, &format!("cryptographic resource profile {} maximum shape", profile.id))?;
        validate_nonempty(&profile.evidence, &format!("cryptographic resource profile {} evidence", profile.id))?;
        if profile.budgets.cycles == 0
            || profile.budgets.elf_bytes == 0
            || profile.budgets.max_stack_frame_bytes == 0
            || profile.budgets.transaction_bytes == 0
        {
            bail!("cryptographic resource profile {} budgets must be positive", profile.id);
        }
        match (&profile.status[..], &profile.measured) {
            ("passed", Some(measured)) => {
                validate_resource_metrics(measured, &profile.budgets, &format!("cryptographic resource profile {}", profile.id))?
            }
            ("release-candidate-required", None) => {}
            ("passed", None) => bail!("passed cryptographic resource profile {} must record measurements", profile.id),
            ("release-candidate-required", Some(_)) => {
                bail!("pending cryptographic resource profile {} must not present candidate measurements as passed", profile.id)
            }
            _ => unreachable!(),
        }
        if release && profile.status != "passed" {
            bail!("release cryptographic resource profile {} still requires candidate measurements", profile.id);
        }
        for capability in &profile.capabilities {
            if !capability_ids.contains(capability.as_str()) {
                bail!("cryptographic resource profile {} references unknown capability {capability}", profile.id);
            }
            if owners.insert(capability.clone(), profile.id.clone()).is_some() {
                bail!("cryptographic capability {capability} is assigned to more than one resource profile");
            }
        }
    }
    if owners.keys().map(String::as_str).collect::<BTreeSet<_>>() != *capability_ids {
        bail!("cryptographic resource profiles must cover every capability exactly once");
    }
    Ok(owners)
}

fn validate_crypto_matrix(
    matrix: &CryptographicCapabilityMatrix,
    budgets: &CryptographicResourceBudgets,
    release: bool,
) -> Result<()> {
    if matrix.schema != CRYPTO_MATRIX_SCHEMA {
        bail!("cryptographic capability matrix schema must be {CRYPTO_MATRIX_SCHEMA}");
    }
    if !matches!(matrix.status.as_str(), "candidate" | "accepted") {
        bail!("cryptographic capability matrix status must be candidate or accepted");
    }
    if matrix.scope.trim().is_empty() || matrix.claim.trim().is_empty() {
        bail!("cryptographic capability matrix scope and claim must be explicit");
    }
    if matrix.documentation != CRYPTO_MATRIX_DOCUMENTATION {
        bail!("cryptographic capability matrix documentation must be {CRYPTO_MATRIX_DOCUMENTATION}");
    }
    validate_nonempty(&matrix.deferred, "cryptographic capability matrix deferred boundaries")?;

    let domain_ids = matrix.domains.iter().map(|domain| domain.id.as_str()).collect::<BTreeSet<_>>();
    if domain_ids != REQUIRED_CRYPTO_DOMAINS.into_iter().collect() || matrix.domains.len() != REQUIRED_CRYPTO_DOMAINS.len() {
        bail!("cryptographic capability matrix must contain each required value domain exactly once");
    }
    for domain in &matrix.domains {
        if domain.source_type.trim().is_empty() || domain.enforcement.trim().is_empty() {
            bail!("cryptographic value domain {} must define its source type and enforcement", domain.id);
        }
        validate_nonempty(&domain.evidence, &format!("cryptographic value domain {} evidence", domain.id))?;
    }

    let capability_ids = matrix.capabilities.iter().map(|capability| capability.id.as_str()).collect::<BTreeSet<_>>();
    if capability_ids != REQUIRED_CRYPTO_CAPABILITIES.into_iter().collect()
        || matrix.capabilities.len() != REQUIRED_CRYPTO_CAPABILITIES.len()
    {
        bail!("cryptographic capability matrix must contain each required portfolio capability exactly once");
    }
    let resource_profile_owners = validate_crypto_resource_budgets(budgets, &capability_ids, release)?;
    let family_ids = REQUIRED_FAMILIES.into_iter().collect::<BTreeSet<_>>();
    let mut covered_families = BTreeSet::new();
    for capability in &matrix.capabilities {
        if !matches!(capability.classification.as_str(), "native" | "checked-identity" | "exact-standard-lock" | "trusted-external") {
            bail!("cryptographic capability {} has an invalid classification", capability.id);
        }
        if !matches!(capability.status.as_str(), "executable" | "composition-boundary") {
            bail!("cryptographic capability {} has an invalid status", capability.id);
        }
        validate_nonempty(&capability.apis, &format!("cryptographic capability {} APIs", capability.id))?;
        validate_nonempty(&capability.algorithms, &format!("cryptographic capability {} algorithms", capability.id))?;
        validate_nonempty(&capability.input_domains, &format!("cryptographic capability {} input domains", capability.id))?;
        validate_nonempty(&capability.bounds, &format!("cryptographic capability {} bounds", capability.id))?;
        validate_nonempty(&capability.families, &format!("cryptographic capability {} families", capability.id))?;
        validate_nonempty(&capability.ckb_vm_evidence, &format!("cryptographic capability {} CKB-VM evidence", capability.id))?;
        validate_nonempty(&capability.checker_evidence, &format!("cryptographic capability {} checker evidence", capability.id))?;
        if !matches!(capability.measurement_status.as_str(), "passed" | "release-candidate-required") {
            bail!("cryptographic capability {} has an invalid measurement status", capability.id);
        }
        validate_nonempty(&capability.resource_profiles, &format!("cryptographic capability {} resource profiles", capability.id))?;
        validate_nonempty(
            &capability.measurement_evidence,
            &format!("cryptographic capability {} measurement evidence", capability.id),
        )?;
        validate_nonempty(
            &capability.dependency_identity_evidence,
            &format!("cryptographic capability {} dependency identity evidence", capability.id),
        )?;
        if capability.failure_codes.is_empty() || capability.witness_owner.trim().is_empty() || capability.boundary.trim().is_empty() {
            bail!("cryptographic capability {} must define failures, witness ownership, and its proof boundary", capability.id);
        }
        let expected_profile =
            resource_profile_owners.get(&capability.id).expect("resource profile validation covers every cryptographic capability");
        if capability.resource_profiles.len() != 1 || capability.resource_profiles[0] != *expected_profile {
            bail!("cryptographic capability {} must reference its exact resource profile", capability.id);
        }
        let profile = budgets.profiles.iter().find(|profile| profile.id == *expected_profile).expect("validated resource profile");
        if capability.measurement_status != profile.status {
            bail!("cryptographic capability {} measurement status disagrees with resource profile", capability.id);
        }
        if !capability.input_domains.iter().all(|domain| domain_ids.contains(domain.as_str()))
            || !domain_ids.contains(capability.output_domain.as_str())
        {
            bail!("cryptographic capability {} references an unknown value domain", capability.id);
        }
        for family in &capability.families {
            if !family_ids.contains(family.as_str()) {
                bail!("cryptographic capability {} references unknown business family {family}", capability.id);
            }
            covered_families.insert(family.as_str());
        }
        if release && capability.measurement_status != "passed" {
            bail!("release cryptographic capability {} still requires maximum-bound measurements", capability.id);
        }
    }
    if covered_families != family_ids {
        bail!("cryptographic capability matrix does not cover every business family");
    }

    let release_gates = matrix.release_requirements.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if release_gates != REQUIRED_CRYPTO_RELEASE_GATES.into_iter().collect() {
        bail!("cryptographic capability matrix must classify every release requirement");
    }
    for (gate, status) in &matrix.release_requirements {
        if !matches!(status.as_str(), "passed" | "pending" | "not-authorized" | "waived")
            || (status == "waived" && gate != "independent_review")
        {
            bail!("cryptographic capability matrix release requirement {gate} has an invalid status");
        }
        if release && status != "passed" && !(gate == "independent_review" && status == "waived") {
            bail!("release cryptographic capability matrix is incomplete: {gate} is {status}");
        }
    }
    if release && matrix.status != "accepted" {
        bail!("release cryptographic capability matrix status must be accepted");
    }
    Ok(())
}

fn inventory_digest(root: &Path, paths: &BTreeSet<String>) -> Result<String> {
    let mut digest = Sha256::new();
    for relative in paths {
        let bytes = fs::read(root.join(relative)).with_context(|| format!("failed to read corpus evidence {relative}"))?;
        digest.update((relative.len() as u64).to_le_bytes());
        digest.update(relative.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(format!("0x{}", hex::encode(digest.finalize())))
}

fn tracked_files(root: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("git")
        .args(["ls-files", "--recurse-submodules", "-z"])
        .current_dir(root)
        .output()
        .context("failed to enumerate tracked corpus evidence")?;
    if !output.status.success() {
        bail!("git ls-files --recurse-submodules failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| String::from_utf8(bytes.to_vec()).context("tracked corpus path is not UTF-8"))
        .collect()
}

fn validate_path(root: &Path, tracked: &BTreeSet<String>, relative: &str) -> Result<()> {
    let path = Path::new(relative);
    if path.is_absolute() || path.components().any(|component| !matches!(component, Component::Normal(_))) || relative.contains('\\') {
        bail!("corpus evidence path is not normalized and repository-relative: {relative}");
    }
    let full = root.join(path);
    let metadata = fs::symlink_metadata(&full).with_context(|| format!("corpus evidence is missing: {relative}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("corpus evidence must be a regular non-symlink file: {relative}");
    }
    let canonical = full.canonicalize().with_context(|| format!("failed to canonicalize corpus evidence: {relative}"))?;
    if !canonical.starts_with(root.canonicalize()?) {
        bail!("corpus evidence escapes the repository: {relative}");
    }
    if !tracked.contains(relative) {
        bail!("corpus evidence is not tracked by Git: {relative}");
    }
    Ok(())
}

fn validate_nonempty(values: &[String], label: &str) -> Result<()> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        bail!("{label} must contain nonempty entries");
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() {
        bail!("{label} contains duplicates");
    }
    Ok(())
}

fn validate(
    root: &Path,
    corpus: &Corpus,
    ledger: &crate::capability_ledger::CapabilityLedger,
    scenario_evidence: &crate::scenario_evidence::ScenarioEvidence,
    matrix: &CryptographicCapabilityMatrix,
    budgets: &CryptographicResourceBudgets,
    release: bool,
) -> Result<()> {
    if corpus.schema != SCHEMA {
        bail!("business corpus schema must be {SCHEMA}");
    }
    if !matches!(corpus.status.as_str(), "candidate" | "accepted") {
        bail!("business corpus status must be candidate or accepted");
    }
    if corpus.claim.trim().is_empty() {
        bail!("business corpus claim must be explicit");
    }
    if corpus.capability_ledger != crate::capability_ledger::MANIFEST {
        bail!("business corpus capability ledger must be {}", crate::capability_ledger::MANIFEST);
    }
    crate::capability_ledger::validate(root, ledger, release)?;
    let ledger_digest = format!("0x{}", hex::encode(Sha256::digest(fs::read(root.join(&corpus.capability_ledger))?)));
    if corpus.capability_ledger_sha256 != ledger_digest {
        bail!("business corpus capability ledger digest is stale; expected {ledger_digest}; run check-business-corpus --write");
    }
    if corpus.scenario_evidence_manifest != crate::scenario_evidence::MANIFEST {
        bail!("business corpus scenario evidence must be {}", crate::scenario_evidence::MANIFEST);
    }
    crate::scenario_evidence::validate(root, scenario_evidence, release)?;
    let scenario_digest = format!("0x{}", hex::encode(Sha256::digest(fs::read(root.join(&corpus.scenario_evidence_manifest))?)));
    if corpus.scenario_evidence_sha256 != scenario_digest {
        bail!("business corpus scenario evidence digest is stale; expected {scenario_digest}; run check-business-corpus --write");
    }
    validate_crypto_matrix(matrix, budgets, release)?;
    let family_ids = corpus.families.iter().map(|family| family.id.as_str()).collect::<BTreeSet<_>>();
    if family_ids != REQUIRED_FAMILIES.into_iter().collect() || corpus.families.len() != REQUIRED_FAMILIES.len() {
        bail!("business corpus must contain each required family exactly once");
    }
    for family in &corpus.families {
        validate_nonempty(&family.scenarios, &format!("{} scenarios", family.id))?;
        validate_nonempty(&family.required_capabilities, &format!("{} required_capabilities", family.id))?;
        validate_nonempty(&family.sources, &format!("{} sources", family.id))?;
        validate_nonempty(&family.transaction_fixtures, &format!("{} transaction_fixtures", family.id))?;
        validate_nonempty(&family.positive_cases, &format!("{} positive_cases", family.id))?;
        validate_nonempty(&family.adversarial_cases, &format!("{} adversarial_cases", family.id))?;
        if !matches!(family.reference.kind.as_str(), "matched-rust" | "pinned-standard-script" | "exact-trusted-external")
            || family.reference.identity.trim().is_empty()
        {
            bail!("{} has an invalid reference boundary", family.id);
        }
        validate_nonempty(&family.reference.paths, &format!("{} reference paths", family.id))?;
        let layers = family.evidence_layers.keys().map(String::as_str).collect::<BTreeSet<_>>();
        if layers != REQUIRED_LAYERS.into_iter().collect() {
            bail!("{} must classify every required evidence layer exactly once", family.id);
        }
        for (name, layer) in &family.evidence_layers {
            if !matches!(layer.status.as_str(), "passed" | "not-applicable" | "pending" | "release-candidate-required") {
                bail!("{} evidence layer {name} has an invalid status", family.id);
            }
            if layer.status == "passed" {
                validate_nonempty(&layer.paths, &format!("{} passed {name} evidence", family.id))?;
            }
            if release && !matches!(layer.status.as_str(), "passed" | "not-applicable") {
                bail!("release corpus is incomplete: {} evidence layer {name} is {}", family.id, layer.status);
            }
        }
    }

    if corpus.anchor.id != "authenticated-partial-settlement"
        || corpus.anchor.artifacts < 4
        || corpus.anchor.script_groups < 5
        || corpus.anchor.sources.len() < 4
    {
        bail!("business corpus anchor does not meet the multi-artifact composition boundary");
    }
    validate_nonempty(&corpus.anchor.positive_cases, "anchor positive_cases")?;
    validate_nonempty(&corpus.anchor.adversarial_cases, "anchor adversarial_cases")?;
    validate_nonempty(&corpus.anchor.protocol_bundle_evidence, "anchor protocol_bundle_evidence")?;
    validate_nonempty(&corpus.anchor.builder_evidence, "anchor builder_evidence")?;
    let capabilities = corpus.anchor.required_capabilities.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if !REQUIRED_ANCHOR_CAPABILITIES.into_iter().all(|capability| capabilities.contains(capability)) {
        bail!("business corpus anchor is missing a required cross-feature capability");
    }

    let release_gates = corpus.release_requirements.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if release_gates != REQUIRED_RELEASE_GATES.into_iter().collect() {
        bail!("business corpus release_requirements must classify every required gate");
    }
    for (gate, status) in &corpus.release_requirements {
        if !matches!(status.as_str(), "passed" | "pending" | "not-authorized" | "waived")
            || (status == "waived" && gate != "independent_review")
        {
            bail!("business corpus release requirement {gate} has an invalid status");
        }
        if release && status != "passed" && !(gate == "independent_review" && status == "waived") {
            bail!("release corpus is incomplete: release requirement {gate} is {status}");
        }
    }
    if release && corpus.status != "accepted" {
        bail!("release corpus status must be accepted");
    }

    let evidence = collect_evidence(corpus, matrix, budgets);
    let declared = corpus.evidence_files.iter().cloned().collect::<BTreeSet<_>>();
    if declared.len() != corpus.evidence_files.len() || declared != evidence {
        bail!("business corpus evidence_files is stale; run check-business-corpus --write");
    }
    let tracked = tracked_files(root)?;
    for relative in &evidence {
        validate_path(root, &tracked, relative)?;
    }
    let digest = inventory_digest(root, &evidence)?;
    if corpus.inventory_sha256 != digest {
        bail!("business corpus inventory digest is stale; expected {digest}; run check-business-corpus --write");
    }
    println!(
        "{}",
        serde_json::json!({
            "schema": corpus.schema,
            "status": corpus.status,
            "families": corpus.families.len(),
            "evidence_files": evidence.len(),
            "inventory_sha256": digest,
            "release_ready": release,
        })
    );
    Ok(())
}

pub fn run(root: &Path, write: bool, release: bool) -> Result<()> {
    if write && release {
        bail!("--write and --release cannot be used together");
    }
    let path = root.join(MANIFEST);
    let bytes = fs::read(&path).with_context(|| format!("failed to read {MANIFEST}"))?;
    let mut value: Value = serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {MANIFEST}"))?;
    let mut corpus: Corpus = serde_json::from_value(value.clone()).with_context(|| format!("invalid {MANIFEST}"))?;
    let ledger = crate::capability_ledger::load(root)?;
    let scenario_evidence = crate::scenario_evidence::load(root)?;
    let matrix = load_crypto_matrix(root, &corpus)?;
    let budgets = load_crypto_resource_budgets(root, &matrix)?;
    if write {
        let evidence = collect_evidence(&corpus, &matrix, &budgets);
        let ledger_digest = Sha256::digest(fs::read(root.join(&corpus.capability_ledger))?);
        value["capability_ledger_sha256"] = Value::String(format!("0x{}", hex::encode(ledger_digest)));
        let scenario_digest = Sha256::digest(fs::read(root.join(&corpus.scenario_evidence_manifest))?);
        value["scenario_evidence_sha256"] = Value::String(format!("0x{}", hex::encode(scenario_digest)));
        value["evidence_files"] = serde_json::to_value(evidence.iter().collect::<Vec<_>>())?;
        value["inventory_sha256"] = Value::String(inventory_digest(root, &evidence)?);
        let mut output = serde_json::to_vec_pretty(&value)?;
        output.push(b'\n');
        fs::write(&path, output).with_context(|| format!("failed to update {MANIFEST}"))?;
        corpus = serde_json::from_value(value)?;
    }
    validate(root, &corpus, &ledger, &scenario_evidence, &matrix, &budgets, release)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix() -> CryptographicCapabilityMatrix {
        serde_json::from_str(include_str!("../../../tests/fixtures/cryptographic_capability_matrix.json"))
            .expect("checked-in cryptographic capability matrix")
    }

    fn resource_budgets() -> CryptographicResourceBudgets {
        serde_json::from_str(include_str!("../../../tests/fixtures/cryptographic_resource_budgets.json"))
            .expect("checked-in cryptographic resource budgets")
    }

    #[test]
    fn cryptographic_matrix_rejects_removed_rows_unknown_domains_and_incomplete_release() {
        let mut missing = matrix();
        missing.capabilities.pop();
        let budgets = resource_budgets();
        assert!(validate_crypto_matrix(&missing, &budgets, false)
            .unwrap_err()
            .to_string()
            .contains("each required portfolio capability"));

        let mut unknown_domain = matrix();
        unknown_domain.capabilities[0].output_domain = "unclassified-bytes".to_string();
        assert!(validate_crypto_matrix(&unknown_domain, &budgets, false).unwrap_err().to_string().contains("unknown value domain"));

        let candidate = matrix();
        assert!(validate_crypto_matrix(&candidate, &budgets, true).unwrap_err().to_string().contains("incomplete"));

        let mut invalid_waiver = matrix();
        invalid_waiver.release_requirements.insert("release_gate".to_string(), "waived".to_string());
        assert!(validate_crypto_matrix(&invalid_waiver, &budgets, false).unwrap_err().to_string().contains("invalid status"));
    }

    #[test]
    fn cryptographic_resource_budgets_reject_missing_profiles_and_overruns() {
        let matrix = matrix();
        let mut missing = resource_budgets();
        missing.profiles.pop();
        assert!(validate_crypto_matrix(&matrix, &missing, false)
            .unwrap_err()
            .to_string()
            .contains("every required profile exactly once"));

        let mut overrun = resource_budgets();
        let profile = &mut overrun.profiles[0];
        profile.measured.as_mut().expect("passed profile measurement").cycles = profile.budgets.cycles + 1;
        assert!(validate_crypto_matrix(&matrix, &overrun, false).unwrap_err().to_string().contains("exceeds budget"));
    }
}
