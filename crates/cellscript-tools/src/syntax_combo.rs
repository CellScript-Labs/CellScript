//! Rust runner for the matrix-driven CellScript syntax-combination audit.
//!
//! The deterministic case declarations are frozen in
//! `tests/syntax_combo/cases.json`. Runtime behaviour, seed annotations,
//! compiler execution, metadata oracles, shrinking, and report generation
//! remain implemented here so the gate has no Python dependency.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use blake2b_ref::Blake2bBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use time::format_description;
use time::OffsetDateTime;
use wait_timeout::ChildExt;

use crate::shared::{python_json_compact, python_json_pretty};

const DEFAULT_SEED: u64 = 20_260_503;

#[derive(Clone, Debug, Deserialize)]
struct Expected {
    phase: String,
    #[serde(default)]
    contains: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Oracle {
    action: Option<String>,
    #[serde(default)]
    consume_bindings: Vec<String>,
    #[serde(default)]
    create_bindings: Vec<String>,
    #[serde(default)]
    locked_outputs: Vec<String>,
    #[serde(default)]
    create_fields: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    obligation_contains: Vec<String>,
    validity_type: Option<String>,
    #[serde(default)]
    validity_tiers: Vec<String>,
    borrow_scope: Option<String>,
    borrow_view_type: Option<String>,
    capability_operation: Option<String>,
    capability_type: Option<String>,
    payload_enum: Option<String>,
    protocol_role_action: Option<String>,
    protocol_role: Option<String>,
    protocol_role_source: Option<String>,
    protocol_role_conflict: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct AuditCase {
    name: String,
    source: String,
    expected: Expected,
    #[serde(default)]
    oracle: Oracle,
    #[serde(default = "generated_origin")]
    origin: String,
}

fn generated_origin() -> String {
    "generated".to_owned()
}

impl AuditCase {
    fn case_id(&self) -> String {
        let input = format!("{}\n{}", self.name, self.source);
        let mut state = Blake2bBuilder::new(6).build();
        state.update(input.as_bytes());
        let mut digest = [0_u8; 6];
        state.finalize(&mut digest);
        hex::encode(digest)
    }
}

#[derive(Debug, Deserialize)]
struct Manifest {
    cases: Vec<AuditCase>,
    governance_release_matrix: Value,
    bug_class_contracts: Vec<Value>,
}

struct CommandOutput {
    success: bool,
    output: String,
}

fn run_cmd(root: &Path, argv: &[String], timeout: Duration) -> Result<CommandOutput> {
    let (program, args) = argv.split_first().context("audit command is empty")?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run {}", argv.join(" ")))?;
    let stdout = child.stdout.take().context("child stdout was not piped")?;
    let stderr = child.stderr.take().context("child stderr was not piped")?;
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut reader = stdout;
        let _ = reader.read_to_end(&mut bytes);
        bytes
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut reader = stderr;
        let _ = reader.read_to_end(&mut bytes);
        bytes
    });
    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            child.kill().with_context(|| format!("failed to kill timed-out command {}", argv.join(" ")))?;
            let _ = child.wait();
            bail!("command timed out after {}s: {}", timeout.as_secs(), argv.join(" "));
        }
    };
    let mut bytes = stdout_reader.join().unwrap_or_default();
    bytes.extend(stderr_reader.join().unwrap_or_default());
    Ok(CommandOutput { success: status.success(), output: String::from_utf8_lossy(&bytes).into_owned() })
}

fn compact(root: &Path, text: &str, limit: usize) -> String {
    let text = text.replace(&root.display().to_string(), "$ROOT");
    if text.chars().count() <= limit {
        return text;
    }
    let prefix: String = text.chars().take(limit).collect();
    format!("{prefix}\n...<truncated>...")
}

fn cellc_bin(root: &Path) -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("CELLC_BIN") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
        bail!("missing required tool: {}", path.display());
    }
    let target_dir = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root.join("target"));
    let target_dir = if target_dir.is_absolute() { target_dir } else { root.join(target_dir) };
    let candidate = target_dir.join("debug/cellc");
    if candidate.is_file() {
        return Ok(candidate);
    }
    let build =
        run_cmd(root, &["cargo".into(), "build".into(), "--locked".into(), "--bin".into(), "cellc".into()], Duration::from_secs(120))?;
    if !build.success {
        bail!("{}", compact(root, &build.output, 4_000));
    }
    Ok(candidate)
}

fn parse_seed(root: &Path, path: &Path) -> Result<AuditCase> {
    let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut expected = Expected { phase: "accept".to_owned(), contains: Vec::new() };
    let mut oracle = Oracle::default();
    for line in text.lines() {
        let Some(payload) = line.trim().strip_prefix("// audit:") else {
            continue;
        };
        let Some((key, value)) = payload.trim().split_once('=') else {
            continue;
        };
        let value = value.trim().to_owned();
        match key.trim() {
            "phase" => expected.phase = value,
            "contains" => expected.contains.push(value),
            "validity_type" => oracle.validity_type = Some(value),
            "validity_tier" => oracle.validity_tiers.push(value),
            "borrow_scope" => oracle.borrow_scope = Some(value),
            "borrow_view_type" => oracle.borrow_view_type = Some(value),
            "capability_operation" => oracle.capability_operation = Some(value),
            "capability_type" => oracle.capability_type = Some(value),
            "payload_enum" => oracle.payload_enum = Some(value),
            "protocol_role_action" => oracle.protocol_role_action = Some(value),
            "protocol_role" => oracle.protocol_role = Some(value),
            "protocol_role_source" => oracle.protocol_role_source = Some(value),
            "protocol_role_conflict" => oracle.protocol_role_conflict = Some(value.eq_ignore_ascii_case("true")),
            _ => {}
        }
    }
    let stem = path.file_stem().and_then(|value| value.to_str()).context("seed path has no UTF-8 stem")?;
    Ok(AuditCase {
        name: format!("seed-{stem}"),
        source: text,
        expected,
        oracle,
        origin: path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/"),
    })
}

/// Minimal implementation of CPython's MT19937 integer-seed path and
/// `_randbelow`, used solely to preserve historical deep-audit case IDs.
struct PythonRandom {
    state: [u32; 624],
    index: usize,
}

impl PythonRandom {
    fn new(seed: u64) -> Self {
        let key = [seed as u32, (seed >> 32) as u32];
        let key = if key[1] == 0 { &key[..1] } else { &key[..] };
        let mut state = [0_u32; 624];
        state[0] = 19_650_218;
        for index in 1..624 {
            state[index] = 1_812_433_253_u32.wrapping_mul(state[index - 1] ^ (state[index - 1] >> 30)).wrapping_add(index as u32);
        }
        let (mut i, mut j) = (1_usize, 0_usize);
        for _ in 0..624.max(key.len()) {
            state[i] =
                (state[i] ^ (state[i - 1] ^ (state[i - 1] >> 30)).wrapping_mul(1_664_525)).wrapping_add(key[j]).wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= 624 {
                state[0] = state[623];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..623 {
            state[i] = (state[i] ^ (state[i - 1] ^ (state[i - 1] >> 30)).wrapping_mul(1_566_083_941)).wrapping_sub(i as u32);
            i += 1;
            if i >= 624 {
                state[0] = state[623];
                i = 1;
            }
        }
        state[0] = 0x8000_0000;
        Self { state, index: 624 }
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= 624 {
            for index in 0..624 {
                let value = (self.state[index] & 0x8000_0000) | (self.state[(index + 1) % 624] & 0x7fff_ffff);
                self.state[index] = self.state[(index + 397) % 624] ^ (value >> 1) ^ if value & 1 == 0 { 0 } else { 0x9908_b0df };
            }
            self.index = 0;
        }
        let mut value = self.state[self.index];
        self.index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^= value >> 18;
        value
    }

    fn below(&mut self, upper: usize) -> usize {
        let bits = usize::BITS as usize - upper.leading_zeros() as usize;
        loop {
            let value = (self.next_u32() >> (32 - bits)) as usize;
            if value < upper {
                return value;
            }
        }
    }

    fn choice(&mut self, upper: usize) -> usize {
        self.below(upper)
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let selected = self.below(index + 1);
            values.swap(index, selected);
        }
    }
}

fn module_source(module_name: &str, body: &str) -> String {
    let base = format!(
        "module cellscript::audit::{module_name}\n\nresource Coin has store, create, consume, replace, burn, relock {{\n    amount: u64,\n    nonce: u64,\n}}\n\nreceipt Voucher -> Coin has create, consume, burn {{\n    amount: u64,\n    nonce: u64,\n    holder: Address,\n}}\n\nresource Wallet has store, create, consume, replace, burn, relock {{\n    owner: Address,\n}}\n"
    );
    format!("{base}\n{}\n", body.trim())
}

fn seeded_deep_cases(seed: u64) -> Vec<AuditCase> {
    let mut rng = PythonRandom::new(seed);
    let suffix = format!("{:x}", seed & 0xffff_ffff);
    let mut fields = vec!["amount", "nonce"];
    rng.shuffle(&mut fields);
    let transfer_fields = fields.iter().map(|field| format!("                        {field}")).collect::<Vec<_>>().join("\n");
    let helpers = ["std::cell::preserve_type", "std::cell::same_lock", "std::cell::preserve_lock", "std::cell::preserve_capacity"];
    let helper = helpers[rng.choice(helpers.len())];
    let rejects = [
        (
            "require_block_lifecycle",
            format!(
                "action seeded_reject_lifecycle_{suffix}(coin: Coin, to: Address) -> next_coin: Coin {{\n    verification\n    require {{\n        std::lifecycle::transfer(coin, next_coin, to) {{\n            amount\n            nonce\n        }}\n    }}\n}}"
            ),
            vec!["require block".to_owned(), "verifier-boundary syntax".to_owned()],
        ),
        (
            "unknown_stdlib",
            format!(
                "action seeded_reject_unknown_{suffix}(coin_before: Coin) -> coin_after: Coin {{\n    verification\n    std::cell::teleport(coin_after, coin_before)\n}}"
            ),
            vec!["unknown stdlib pattern".to_owned()],
        ),
        (
            "transfer_missing_field",
            format!(
                "action seeded_reject_missing_{suffix}(coin: Coin, to: Address) -> next_coin: Coin {{\n    verification\n    std::lifecycle::transfer(coin, next_coin, to) {{\n        amount\n    }}\n}}"
            ),
            vec!["missing nonce".to_owned()],
        ),
    ];
    let reject = &rejects[rng.choice(rejects.len())];
    vec![
        AuditCase {
            name: format!("seeded-deep-transfer-{suffix}"),
            source: module_source(
                &format!("seeded_deep_transfer_{suffix}"),
                &format!(
                    "action seeded_transfer_{suffix}(coin: Coin, to: Address) -> next_coin: Coin {{\n    verification\n    std::lifecycle::transfer(coin, next_coin, to) {{\n{transfer_fields}\n    }}\n}}"
                ),
            ),
            expected: Expected { phase: "accept".into(), contains: Vec::new() },
            oracle: Oracle {
                action: Some(format!("seeded_transfer_{suffix}")),
                consume_bindings: vec!["coin".into()],
                create_bindings: vec!["next_coin".into()],
                locked_outputs: vec!["next_coin".into()],
                create_fields: BTreeMap::from([("next_coin".into(), fields.iter().map(ToString::to_string).collect())]),
                obligation_contains: vec!["create-output-lock".into(), "consume-input:Coin:coin".into()],
                ..Oracle::default()
            },
            origin: "seeded:deep/stdlib-lifecycle".into(),
        },
        AuditCase {
            name: format!("seeded-deep-cell-helper-{suffix}"),
            source: module_source(
                &format!("seeded_deep_cell_helper_{suffix}"),
                &format!(
                    "action seeded_helper_{suffix}(coin_before: Coin) -> coin_after: Coin {{\n    verification\n    {helper}(coin_after, coin_before)\n}}"
                ),
            ),
            expected: Expected { phase: "accept".into(), contains: Vec::new() },
            oracle: Oracle { action: Some(format!("seeded_helper_{suffix}")), ..Oracle::default() },
            origin: "seeded:deep/cell-helper".into(),
        },
        AuditCase {
            name: format!("seeded-deep-reject-{}-{suffix}", reject.0),
            source: module_source(&format!("seeded_deep_reject_{}_{suffix}", reject.0), &reject.1),
            expected: Expected { phase: "reject_compile".into(), contains: reject.2.clone() },
            oracle: Oracle::default(),
            origin: "seeded:deep/reject".into(),
        },
    ]
}

fn load_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join("tests/syntax_combo/cases.json");
    serde_json::from_slice(&fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?)
        .with_context(|| format!("failed to decode {}", path.display()))
}

fn mode_table<'a>(matrix: &'a toml::Value, mode: &str) -> Option<&'a toml::value::Table> {
    matrix.get("mode")?.get(mode)?.as_table()
}

fn load_cases(
    root: &Path,
    manifest: &Manifest,
    matrix: &toml::Value,
    mode: &str,
    budget: Option<usize>,
    seed: u64,
) -> Result<Vec<AuditCase>> {
    // The manifest preserves Python's declaration order: 24 generated cases,
    // followed by 22 CI matrix cases and 3 deep-only matrix cases. Some of the
    // generated edge cases intentionally carry a `matrix:edge/*` provenance,
    // so origin filtering would incorrectly remove them from quick mode.
    let static_count = match mode {
        "quick" => 24,
        "ci" => 46,
        _ => manifest.cases.len(),
    };
    let mut cases: Vec<_> = manifest.cases.iter().take(static_count).cloned().collect();
    if matches!(mode, "deep" | "repro") {
        cases.extend(seeded_deep_cases(seed));
    }
    let default_budget = mode_table(matrix, if matches!(mode, "quick" | "ci") { mode } else { "deep" })
        .and_then(|table| table.get("budget"))
        .and_then(toml::Value::as_integer)
        .map(|value| value as usize)
        .unwrap_or(cases.len());
    let limit = budget.unwrap_or(default_budget);
    cases.truncate(limit.min(cases.len()));

    let seeds = root.join("tests/syntax_combo/seeds");
    if seeds.is_dir() {
        let mut paths = fs::read_dir(&seeds)?.filter_map(std::result::Result::ok).map(|entry| entry.path()).collect::<Vec<_>>();
        paths.sort();
        let mut existing: BTreeSet<String> = cases.iter().map(|case| case.name.clone()).collect();
        for path in paths {
            if path.extension().and_then(|value| value.to_str()) != Some("cell") || !path.is_file() {
                continue;
            }
            let case = parse_seed(root, &path)?;
            if existing.insert(case.name.clone()) {
                cases.push(case);
            }
        }
    }
    Ok(cases)
}

fn output_matches(text: &str, needles: &[String]) -> bool {
    let lowered = text.to_lowercase();
    needles.iter().all(|needle| lowered.contains(&needle.to_lowercase()))
}

fn failure(
    root: &Path,
    case: &AuditCase,
    phase: &str,
    code: &str,
    summary: impl Into<String>,
    run_dir: &Path,
    output: &str,
) -> Result<Value> {
    let shrink_dir = run_dir.join("shrink");
    fs::create_dir_all(&shrink_dir)?;
    let shrink_path = shrink_dir.join(format!("{}.cell", case.case_id()));
    let compact_source =
        case.source.lines().filter(|line| !line.trim().is_empty() && !line.trim().starts_with("//")).collect::<Vec<_>>().join("\n");
    fs::write(&shrink_path, format!("{compact_source}\n"))?;
    Ok(json!({
        "case": case.case_id(),
        "name": case.name,
        "origin": case.origin,
        "phase": phase,
        "code": code,
        "summary": summary.into(),
        "shrunk": shrink_path.strip_prefix(run_dir).unwrap_or(&shrink_path).to_string_lossy().replace('\\', "/"),
        "output": compact(root, output, 1_200),
    }))
}

fn find_action<'a>(metadata: &'a Value, name: &str) -> Option<&'a Value> {
    metadata.get("actions")?.as_array()?.iter().find(|action| action.get("name").and_then(Value::as_str) == Some(name))
}

fn push_failure(
    failures: &mut Vec<Value>,
    root: &Path,
    case: &AuditCase,
    run_dir: &Path,
    code: &str,
    summary: impl Into<String>,
) -> Result<()> {
    failures.push(failure(root, case, "metadata", code, summary, run_dir, "")?);
    Ok(())
}

fn validate_metadata(root: &Path, case: &AuditCase, metadata_path: &Path, run_dir: &Path) -> Result<Vec<Value>> {
    let metadata: Value = match fs::read(metadata_path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()) {
        Some(metadata) => metadata,
        None => {
            return Ok(vec![failure(root, case, "metadata", "SCA-META-JSON", "metadata JSON decode failed", run_dir, "")?]);
        }
    };
    let mut failures = Vec::new();
    let required = ["actions", "compiler_version", "constraints", "lowering", "runtime", "target_profile"];
    let missing = required.iter().filter(|key| metadata.get(**key).is_none()).copied().collect::<Vec<_>>();
    if !missing.is_empty() {
        push_failure(&mut failures, root, case, run_dir, "SCA-META-KEYS", format!("metadata missing keys: {}", missing.join(", ")))?;
    }
    if metadata.pointer("/target_profile/name").and_then(Value::as_str) != Some("ckb") {
        push_failure(&mut failures, root, case, run_dir, "SCA-META-PROFILE", "metadata target_profile.name is not ckb")?;
    }

    let oracle = &case.oracle;
    if let Some(operation) = &oracle.capability_operation {
        let registry = metadata.get("capability_registry").unwrap_or(&Value::Null);
        if registry.get("capability_set_version").and_then(Value::as_u64) != Some(1)
            || registry.get("entailment_version").and_then(Value::as_u64) != Some(1)
        {
            push_failure(
                &mut failures,
                root,
                case,
                run_dir,
                "SCA-META-CAPABILITY-VERSION",
                "capability registry versions are not set to v1",
            )?;
        }
        let canonical = json!(["store", "create", "consume", "destroy", "replace", "burn", "relock", "retarget_type", "read_ref"]);
        if registry.get("capabilities") != Some(&canonical) {
            push_failure(&mut failures, root, case, run_dir, "SCA-META-CAPABILITY-REGISTRY", "capability registry is not canonical")?;
        }
        let proofs = metadata
            .pointer("/runtime/capability_proofs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|proof| {
                proof.get("operation").and_then(Value::as_str) == Some(operation)
                    && oracle
                        .capability_type
                        .as_deref()
                        .is_none_or(|kind| proof.get("type_name").and_then(Value::as_str) == Some(kind))
            })
            .collect::<Vec<_>>();
        if proofs.is_empty() {
            push_failure(
                &mut failures,
                root,
                case,
                run_dir,
                "SCA-META-CAPABILITY-PROOF",
                format!("missing capability proof for {operation}"),
            )?;
        } else {
            let proof = proofs[0];
            let fields = ["required", "provided", "entailed", "missing", "capability_set_version", "entailment_version"];
            if fields.iter().any(|field| proof.get(*field).is_none()) || proof.get("missing") != Some(&json!([])) {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-CAPABILITY-EVIDENCE",
                    "capability proof is missing required/provided/entailed/missing/version evidence",
                )?;
            }
        }
    }

    if let Some(enum_name) = &oracle.payload_enum {
        let layouts = metadata
            .get("enum_layouts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|layout| layout.get("name").and_then(Value::as_str) == Some(enum_name))
            .collect::<Vec<_>>();
        if layouts.is_empty() {
            push_failure(
                &mut failures,
                root,
                case,
                run_dir,
                "SCA-META-PAYLOAD-ENUM",
                format!("missing payload enum layout for {enum_name}"),
            )?;
        } else {
            let layout = layouts[0];
            let has_payload = layout
                .get("variants")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .flat_map(|variant| variant.get("fields").and_then(Value::as_array).into_iter().flatten())
                .next()
                .is_some();
            if layout.get("generic").and_then(Value::as_bool) != Some(false)
                || layout.get("layout").and_then(Value::as_str) != Some("packed-tagged-union-v1")
                || layout.get("tag_width_bytes").and_then(Value::as_u64) != Some(1)
                || layout.get("encoded_size_bytes").and_then(Value::as_u64).unwrap_or(0) <= 1
                || !has_payload
            {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-PAYLOAD-ENUM-LAYOUT",
                    "payload enum metadata is missing its concrete fixed-width tagged-union contract",
                )?;
            }
        }
    }

    if let Some(action_name) = &oracle.protocol_role_action {
        if let Some(action) = find_action(&metadata, action_name) {
            let candidates = action.get("protocol_role_candidates").and_then(Value::as_array).cloned().unwrap_or_default();
            if candidates.is_empty() {
                push_failure(&mut failures, root, case, run_dir, "SCA-META-PROTOCOL-ROLE", "missing attributed role candidates")?;
            } else {
                let selected = &candidates[0];
                if selected.get("role").and_then(Value::as_str) != oracle.protocol_role.as_deref()
                    || selected.get("source").and_then(Value::as_str) != oracle.protocol_role_source.as_deref()
                {
                    push_failure(
                        &mut failures,
                        root,
                        case,
                        run_dir,
                        "SCA-META-PROTOCOL-ROLE-PRECEDENCE",
                        "selected role/source does not match the audit oracle",
                    )?;
                }
                if candidates.iter().any(|candidate| {
                    candidate.get("evidence_tier").and_then(Value::as_str) != Some("metadata-only")
                        || candidate.get("authorization_proven").and_then(Value::as_bool) != Some(false)
                }) {
                    push_failure(
                        &mut failures,
                        root,
                        case,
                        run_dir,
                        "SCA-META-PROTOCOL-ROLE-OVERCLAIM",
                        "role candidates must remain metadata-only with authorization_proven=false",
                    )?;
                }
                let roles =
                    candidates.iter().filter_map(|candidate| candidate.get("role").and_then(Value::as_str)).collect::<BTreeSet<_>>();
                let conflict = roles.len() > 1;
                if oracle.protocol_role_conflict.is_some_and(|expected| expected != conflict) {
                    push_failure(
                        &mut failures,
                        root,
                        case,
                        run_dir,
                        "SCA-META-PROTOCOL-ROLE-CONFLICT",
                        format!("role conflict={conflict} does not match expected {:?}", oracle.protocol_role_conflict),
                    )?;
                }
                if action.get("proof_plan").and_then(Value::as_array).is_some_and(|plans| {
                    plans.iter().any(|plan| plan.get("category").and_then(Value::as_str) == Some("protocol-role"))
                }) {
                    push_failure(
                        &mut failures,
                        root,
                        case,
                        run_dir,
                        "SCA-META-PROTOCOL-ROLE-PROOFPLAN",
                        "ProtocolGraph roles must not appear as ProofPlan authorization evidence",
                    )?;
                }
            }
        } else {
            push_failure(
                &mut failures,
                root,
                case,
                run_dir,
                "SCA-META-PROTOCOL-ROLE-ACTION",
                format!("missing ProtocolGraph role action {action_name}"),
            )?;
        }
    }

    if let Some(scope) = &oracle.borrow_scope {
        let region = metadata
            .pointer("/runtime/borrow_regions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|region| region.get("scope_name").and_then(Value::as_str) == Some(scope));
        if let Some(region) = region {
            if oracle.borrow_view_type.as_deref().is_some_and(|view| region.get("view_type").and_then(Value::as_str) != Some(view)) {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-BORROW-VIEW",
                    "borrow view type does not match audit oracle",
                )?;
            }
            if region.get("storage").and_then(Value::as_str) != Some("none")
                || region.get("abi").and_then(Value::as_str) != Some("none")
                || region.get("evidence_tier").and_then(Value::as_str) != Some("checked-static")
            {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-BORROW-EVIDENCE",
                    "borrow region must declare storage=none, abi=none, and checked-static evidence",
                )?;
            }
            let prefix = format!("action:{scope}#borrow-region:");
            let plan = metadata
                .pointer("/runtime/proof_plan")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|plan| plan.get("origin").and_then(Value::as_str).is_some_and(|origin| origin.starts_with(&prefix)));
            if plan.and_then(|plan| plan.get("evidence_tier")).and_then(Value::as_str) != Some("checked-static") {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-BORROW-PROOFPLAN",
                    "borrow region is missing a checked-static ProofPlan record",
                )?;
            }
        } else {
            push_failure(
                &mut failures,
                root,
                case,
                run_dir,
                "SCA-META-BORROW-REGION",
                format!("missing borrow metadata for {scope}"),
            )?;
        }
    }

    if let Some(type_name) = &oracle.validity_type {
        let type_metadata = metadata
            .get("types")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|item| item.get("name").and_then(Value::as_str) == Some(type_name));
        if let Some(type_metadata) = type_metadata {
            let predicates = type_metadata.get("validity_predicates").and_then(Value::as_array).cloned().unwrap_or_default();
            if predicates.is_empty() {
                push_failure(&mut failures, root, case, run_dir, "SCA-META-VALIDITY", "validity metadata has no predicate records")?;
            }
            let canonical = [
                "checked-static",
                "checked-runtime",
                "runtime-helper-required",
                "builder-evidence-required",
                "metadata-only",
                "chain-evidence-required",
            ];
            let tiers =
                predicates.iter().filter_map(|predicate| predicate.get("evidence_tier").and_then(Value::as_str)).collect::<Vec<_>>();
            if tiers.iter().any(|tier| !canonical.contains(tier)) {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-VALIDITY-TIER",
                    "validity metadata contains non-canonical evidence tiers",
                )?;
            }
            for tier in &oracle.validity_tiers {
                if !tiers.contains(&tier.as_str()) {
                    push_failure(
                        &mut failures,
                        root,
                        case,
                        run_dir,
                        "SCA-META-VALIDITY-TIER",
                        format!("validity metadata is missing evidence tier '{tier}'"),
                    )?;
                }
            }
            let prefix = format!("validity:{type_name}#");
            let plan_count = metadata
                .pointer("/runtime/proof_plan")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|plan| plan.get("origin").and_then(Value::as_str).is_some_and(|origin| origin.starts_with(&prefix)))
                .count();
            if plan_count < predicates.len() {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-VALIDITY-PROOFPLAN",
                    format!("validity ProofPlan count {plan_count} is smaller than predicate count {}", predicates.len()),
                )?;
            }
        } else {
            push_failure(
                &mut failures,
                root,
                case,
                run_dir,
                "SCA-META-VALIDITY-TYPE",
                format!("missing type metadata for {type_name}"),
            )?;
        }
    }

    if let Some(action_name) = &oracle.action {
        let Some(action) = find_action(&metadata, action_name) else {
            push_failure(&mut failures, root, case, run_dir, "SCA-META-ACTION", format!("missing action metadata for {action_name}"))?;
            return Ok(failures);
        };
        let consume_bindings = action
            .get("consume_set")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("binding").and_then(Value::as_str))
            .collect::<Vec<_>>();
        let expected_consume = oracle.consume_bindings.iter().map(String::as_str).collect::<Vec<_>>();
        if !oracle.consume_bindings.is_empty() && consume_bindings != expected_consume {
            push_failure(&mut failures, root, case, run_dir, "SCA-META-CONSUME", "consume bindings do not match audit oracle")?;
        }
        if consume_bindings.iter().copied().collect::<BTreeSet<_>>().len() != consume_bindings.len() {
            push_failure(&mut failures, root, case, run_dir, "SCA-META-DUP-CONSUME", "duplicate consume binding")?;
        }
        let create_by_binding = action
            .get("create_set")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| Some((item.get("binding")?.as_str()?, item)))
            .collect::<BTreeMap<_, _>>();
        for binding in &oracle.create_bindings {
            if !create_by_binding.contains_key(binding.as_str()) {
                push_failure(&mut failures, root, case, run_dir, "SCA-META-CREATE", format!("missing create binding {binding}"))?;
            }
        }
        for binding in &oracle.locked_outputs {
            if create_by_binding.get(binding.as_str()).and_then(|item| item.get("has_lock")).and_then(Value::as_bool) != Some(true) {
                push_failure(&mut failures, root, case, run_dir, "SCA-META-LOCK", format!("create binding {binding} is not locked"))?;
            }
        }
        for (binding, fields) in &oracle.create_fields {
            let actual = create_by_binding
                .get(binding.as_str())
                .and_then(|item| item.get("fields"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            if actual != fields.iter().map(String::as_str).collect::<Vec<_>>() {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-FIELDS",
                    format!("create fields for {binding} do not match audit oracle"),
                )?;
            }
        }
        let obligations = python_json_compact(action.get("verifier_obligations").unwrap_or(&Value::Null))?;
        for needle in &oracle.obligation_contains {
            if !obligations.contains(needle) {
                push_failure(
                    &mut failures,
                    root,
                    case,
                    run_dir,
                    "SCA-META-OBLIGATION",
                    format!("missing obligation containing '{needle}'"),
                )?;
            }
        }
        if action
            .get("fail_closed_runtime_features")
            .is_some_and(|value| !value.as_array().is_some_and(Vec::is_empty) && !value.is_null())
        {
            push_failure(
                &mut failures,
                root,
                case,
                run_dir,
                "SCA-META-FAIL-CLOSED",
                "accepted audit case contains fail_closed_runtime_features",
            )?;
        }
    }
    Ok(failures)
}

fn audit_case(root: &Path, case: &AuditCase, run_dir: &Path, cellc: &Path) -> Result<(String, Vec<Value>)> {
    let case_id = case.case_id();
    let case_path = if case.expected.phase == "reject_parse" {
        run_dir.join("parse_reject").join(format!("{case_id}.cell"))
    } else {
        run_dir.join("cases").join(format!("{case_id}.cell"))
    };
    let fmt_path = run_dir.join("fmt").join(format!("{case_id}.cell"));
    let asm_path = run_dir.join("asm").join(format!("{case_id}.s"));
    let meta_path = run_dir.join("meta").join(format!("{case_id}.json"));
    for parent in [case_path.parent(), fmt_path.parent(), asm_path.parent(), meta_path.parent()].into_iter().flatten() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&case_path, &case.source)?;
    let cellc = cellc.display().to_string();
    let parse = run_cmd(root, &[cellc.clone(), "--parse".into(), case_path.display().to_string()], Duration::from_secs(20))?;
    if case.expected.phase == "reject_parse" {
        if parse.success {
            return Ok((
                "failed".into(),
                vec![failure(
                    root,
                    case,
                    "parse",
                    "SCA-PARSE-ACCEPTED",
                    "expected parse rejection, got success",
                    run_dir,
                    &parse.output,
                )?],
            ));
        }
        if !output_matches(&parse.output, &case.expected.contains) {
            return Ok((
                "failed".into(),
                vec![failure(
                    root,
                    case,
                    "parse",
                    "SCA-PARSE-DIAGNOSTIC",
                    "parse diagnostic missing expected tokens",
                    run_dir,
                    &parse.output,
                )?],
            ));
        }
        return Ok(("rejected".into(), Vec::new()));
    }
    if !parse.success {
        return Ok((
            "failed".into(),
            vec![failure(root, case, "parse", "SCA-PARSE-FAILED", "unexpected parse failure", run_dir, &parse.output)?],
        ));
    }

    if case.expected.phase == "accept" {
        fs::write(&fmt_path, &case.source)?;
        let formatted =
            run_cmd(root, &[cellc.clone(), "fmt".into(), "--json".into(), fmt_path.display().to_string()], Duration::from_secs(20))?;
        if !formatted.success {
            return Ok((
                "failed".into(),
                vec![failure(root, case, "fmt", "SCA-FMT-FAILED", "formatter failed", run_dir, &formatted.output)?],
            ));
        }
        let checked = run_cmd(
            root,
            &[cellc.clone(), "fmt".into(), "--check".into(), "--json".into(), fmt_path.display().to_string()],
            Duration::from_secs(20),
        )?;
        if !checked.success {
            return Ok((
                "failed".into(),
                vec![failure(
                    root,
                    case,
                    "fmt",
                    "SCA-FMT-NON-IDEMPOTENT",
                    "formatted source is not idempotent",
                    run_dir,
                    &checked.output,
                )?],
            ));
        }
        let reparsed = run_cmd(root, &[cellc.clone(), "--parse".into(), fmt_path.display().to_string()], Duration::from_secs(20))?;
        if !reparsed.success {
            return Ok((
                "failed".into(),
                vec![failure(root, case, "fmt", "SCA-FMT-PARSE", "formatted source does not parse", run_dir, &reparsed.output)?],
            ));
        }
    }

    let compiled = run_cmd(
        root,
        &[
            cellc.clone(),
            case_path.display().to_string(),
            "--target".into(),
            "riscv64-asm".into(),
            "--target-profile".into(),
            "ckb".into(),
            "--primitive-strict".into(),
            "0.15".into(),
            "-o".into(),
            asm_path.display().to_string(),
        ],
        Duration::from_secs(30),
    )?;
    if case.expected.phase == "reject_compile" {
        if compiled.success {
            return Ok((
                "failed".into(),
                vec![failure(
                    root,
                    case,
                    "compile",
                    "SCA-COMPILE-ACCEPTED",
                    "expected compile rejection, got success",
                    run_dir,
                    &compiled.output,
                )?],
            ));
        }
        if !output_matches(&compiled.output, &case.expected.contains) {
            return Ok((
                "failed".into(),
                vec![failure(
                    root,
                    case,
                    "compile",
                    "SCA-COMPILE-DIAGNOSTIC",
                    "compile diagnostic missing expected tokens",
                    run_dir,
                    &compiled.output,
                )?],
            ));
        }
        return Ok(("rejected".into(), Vec::new()));
    }
    if !compiled.success {
        return Ok((
            "failed".into(),
            vec![failure(root, case, "compile", "SCA-COMPILE-FAILED", "unexpected compile failure", run_dir, &compiled.output)?],
        ));
    }
    if fs::metadata(&asm_path).map_or(true, |metadata| metadata.len() == 0) {
        return Ok((
            "failed".into(),
            vec![failure(
                root,
                case,
                "codegen",
                "SCA-CODEGEN-EMPTY",
                "assembly output is missing or empty",
                run_dir,
                &compiled.output,
            )?],
        ));
    }
    let asm = fs::read_to_string(&asm_path)
        .unwrap_or_else(|_| String::from_utf8_lossy(&fs::read(&asm_path).unwrap_or_default()).into_owned());
    for obsolete in ["IrTransfer", "IrClaim", "IrSettle"] {
        if asm.contains(obsolete) {
            return Ok((
                "failed".into(),
                vec![failure(
                    root,
                    case,
                    "codegen",
                    "SCA-CODEGEN-OBSOLETE",
                    format!("assembly contains obsolete token {obsolete}"),
                    run_dir,
                    "",
                )?],
            ));
        }
    }
    let metadata = run_cmd(
        root,
        &[
            cellc,
            "metadata".into(),
            case_path.display().to_string(),
            "--target".into(),
            "riscv64-asm".into(),
            "--target-profile".into(),
            "ckb".into(),
            "-o".into(),
            meta_path.display().to_string(),
        ],
        Duration::from_secs(30),
    )?;
    if !metadata.success {
        return Ok((
            "failed".into(),
            vec![failure(root, case, "metadata", "SCA-META-FAILED", "metadata command failed", run_dir, &metadata.output)?],
        ));
    }
    let failures = validate_metadata(root, case, &meta_path, run_dir)?;
    if failures.is_empty() {
        Ok(("accepted".into(), failures))
    } else {
        Ok(("failed".into(), failures))
    }
}

fn rank(mode: &str) -> usize {
    match mode {
        "quick" => 0,
        "ci" => 1,
        "deep" => 2,
        "repro" => 3,
        _ => 0,
    }
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(ToOwned::to_owned).collect()
}

fn evaluate_bug_class_coverage(mode: &str, cases: &[AuditCase], contracts: &[Value]) -> Value {
    let names: BTreeSet<_> = cases.iter().map(|case| case.name.as_str()).collect();
    let origins: BTreeSet<_> = cases.iter().map(|case| case.origin.as_str()).collect();
    Value::Array(
        contracts
            .iter()
            .map(|contract| {
                let min_mode = contract.get("min_mode").and_then(Value::as_str).unwrap_or("quick");
                let required = rank(mode) >= rank(min_mode);
                let required_cases = string_array(contract.get("required_cases"));
                let required_origins = string_array(contract.get("required_origins"));
                let missing_cases = required_cases.iter().filter(|name| !names.contains(name.as_str())).cloned().collect::<Vec<_>>();
                let missing_origins =
                    required_origins.iter().filter(|origin| !origins.contains(origin.as_str())).cloned().collect::<Vec<_>>();
                let covered = missing_cases.is_empty() && missing_origins.is_empty();
                json!({
                    "id": contract.get("id").cloned().unwrap_or(Value::Null),
                    "name": contract.get("name").cloned().unwrap_or(Value::Null),
                    "status": if required { if covered { "covered" } else { "missing" } } else { "not_required_for_mode" },
                    "required": required,
                    "min_mode": min_mode,
                    "required_cases": required_cases,
                    "required_origins": required_origins,
                    "missing_cases": if required { missing_cases } else { Vec::new() },
                    "missing_origins": if required { missing_origins } else { Vec::new() },
                    "release_boundary": contract.get("release_boundary").cloned().unwrap_or(Value::Null),
                })
            })
            .collect(),
    )
}

fn governance_oracles(matrix: &toml::Value) -> Value {
    let configured = matrix.get("required_oracles");
    let flag = |name: &str| configured.and_then(|value| value.get(name)).and_then(toml::Value::as_bool).unwrap_or(false);
    json!({
        "parser": flag("parse"),
        "formatter_roundtrip": flag("formatter_roundtrip"),
        "type_effect": flag("type_effect"),
        "ir_metadata": flag("ir_metadata"),
        "codegen_assembly": flag("codegen_assembly"),
        "compact_report": flag("compact_report"),
    })
}

fn contract_failure(code: &str, summary: impl Into<String>) -> Value {
    json!({
        "case": "-",
        "name": "mode-contract",
        "origin": "tests/syntax_combo/matrix.toml",
        "phase": "contract",
        "code": code,
        "summary": summary.into(),
        "shrunk": "",
        "output": "",
    })
}

fn validate_mode_contract(mode: &str, matrix: &toml::Value, report: &Value) -> Vec<Value> {
    if mode == "repro" {
        return Vec::new();
    }
    let Some(config) = mode_table(matrix, mode) else {
        return Vec::new();
    };
    let mut failures = Vec::new();
    for (config_key, report_key, code) in [
        ("min_cases", "generated", "SCA-CONTRACT-CASES"),
        ("min_accept", "accepted", "SCA-CONTRACT-ACCEPT"),
        ("min_reject", "rejected", "SCA-CONTRACT-REJECT"),
    ] {
        let Some(expected) = config.get(config_key).and_then(toml::Value::as_integer) else {
            continue;
        };
        let actual = report.get(report_key).and_then(Value::as_i64).unwrap_or(0);
        if actual < expected {
            failures.push(contract_failure(code, format!("{mode} {report_key} floor {expected} not met; got {actual}")));
        }
    }
    let origins = report.get("origins").and_then(Value::as_object);
    let required_origins =
        config.get("required_origins").and_then(toml::Value::as_array).into_iter().flatten().filter_map(toml::Value::as_str);
    let missing_origins = required_origins.filter(|origin| origins.is_none_or(|map| !map.contains_key(*origin))).collect::<Vec<_>>();
    if !missing_origins.is_empty() {
        failures
            .push(contract_failure("SCA-CONTRACT-ORIGIN", format!("{mode} missing required origins: {}", missing_origins.join(", "))));
    }
    for item in report.get("known_bug_classes").and_then(Value::as_array).into_iter().flatten() {
        if item.get("required").and_then(Value::as_bool) != Some(true) || item.get("status").and_then(Value::as_str) == Some("covered")
        {
            continue;
        }
        let mut details = Vec::new();
        let missing_cases = string_array(item.get("missing_cases"));
        let missing_origins = string_array(item.get("missing_origins"));
        if !missing_cases.is_empty() {
            details.push(format!("missing cases: {}", missing_cases.join(", ")));
        }
        if !missing_origins.is_empty() {
            details.push(format!("missing origins: {}", missing_origins.join(", ")));
        }
        failures.push(contract_failure(
            item.get("id").and_then(Value::as_str).unwrap_or("SCA-CONTRACT-BUG"),
            format!(
                "{mode} bug-class coverage missing for {}: {}",
                item.get("name").and_then(Value::as_str).unwrap_or("unknown"),
                details.join("; ")
            ),
        ));
    }
    failures
}

fn write_reports(run_dir: &Path, report: &Value, failures: &[Value]) -> Result<()> {
    fs::write(run_dir.join("report.json"), format!("{}\n", python_json_pretty(report)?))?;
    let mut jsonl = String::new();
    for item in failures {
        jsonl.push_str(&python_json_compact(item)?);
        jsonl.push('\n');
    }
    fs::write(run_dir.join("report.jsonl"), jsonl)?;
    Ok(())
}

pub fn run(root: &Path, mode: &str, seed: u64, budget: Option<usize>, case_name: Option<&str>) -> Result<i32> {
    let _ = DEFAULT_SEED;
    let manifest = load_manifest(root)?;
    let matrix_path = root.join("tests/syntax_combo/matrix.toml");
    let matrix: toml::Value =
        fs::read_to_string(&matrix_path)?.parse().with_context(|| format!("failed to parse {}", matrix_path.display()))?;
    let cellc = cellc_bin(root)?;
    let timestamp_format = format_description::parse("[year][month][day]-[hour][minute][second]")?;
    let timestamp = OffsetDateTime::now_utc().format(&timestamp_format)?;
    let run_dir = root.join("target/syntax-combo-audit").join(format!("{timestamp}-{mode}-{seed}"));
    fs::create_dir_all(&run_dir)?;
    let mut cases = load_cases(root, &manifest, &matrix, mode, budget, seed)?;
    if mode == "repro" {
        let selected = case_name.context("repro mode requires --case <name-or-id>")?;
        cases.retain(|case| case.name == selected || case.case_id() == selected);
        if cases.is_empty() {
            bail!("unknown repro case: {selected}");
        }
    }

    let mut failures = Vec::new();
    let mut accepted = 0_usize;
    let mut rejected = 0_usize;
    let mut phases: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut origins: BTreeMap<String, usize> = BTreeMap::new();
    for case in &cases {
        *origins.entry(case.origin.clone()).or_default() += 1;
        let (status, case_failures) = audit_case(root, case, &run_dir, &cellc)?;
        let phase =
            phases.entry(case.expected.phase.clone()).or_insert_with(|| BTreeMap::from([("failed".into(), 0), ("passed".into(), 0)]));
        if case_failures.is_empty() {
            *phase.entry("passed".into()).or_default() += 1;
        } else {
            *phase.entry("failed".into()).or_default() += 1;
            failures.extend(case_failures);
        }
        match status.as_str() {
            "accepted" => accepted += 1,
            "rejected" => rejected += 1,
            _ => {}
        }
    }
    let known_bug_classes = evaluate_bug_class_coverage(mode, &cases, &manifest.bug_class_contracts);
    let mut report = json!({
        "status": if failures.is_empty() { "passed" } else { "failed" },
        "mode": mode,
        "seed": seed,
        "generated": cases.len(),
        "accepted": accepted,
        "rejected": rejected,
        "failures_count": failures.len(),
        "governance_release_matrix": manifest.governance_release_matrix,
        "governance_oracles": governance_oracles(&matrix),
        "known_bug_classes": known_bug_classes,
        "phases": phases,
        "origins": origins,
        "failures": failures.iter().take(10).cloned().collect::<Vec<_>>(),
    });
    let contract_failures = validate_mode_contract(mode, &matrix, &report);
    if !contract_failures.is_empty() {
        failures.extend(contract_failures);
        report["status"] = Value::String("failed".into());
        report["failures_count"] = Value::from(failures.len());
        report["failures"] = Value::Array(failures.iter().take(10).cloned().collect());
    }
    write_reports(&run_dir, &report, &failures)?;
    println!(
        "syntax-combo-audit: {} seed={seed} mode={mode} generated={} accepted={accepted} rejected={rejected} failures={}",
        report.get("status").and_then(Value::as_str).unwrap_or("failed"),
        cases.len(),
        failures.len()
    );
    println!("report={}", run_dir.join("report.json").display());
    if !failures.is_empty() {
        println!("top:");
        for item in failures.iter().take(5) {
            println!(
                "  {} {} case={} phase={}",
                item.get("code").and_then(Value::as_str).unwrap_or("-"),
                item.get("summary").and_then(Value::as_str).unwrap_or("-"),
                item.get("case").and_then(Value::as_str).unwrap_or("-"),
                item.get("phase").and_then(Value::as_str).unwrap_or("-")
            );
        }
        Ok(1)
    } else {
        Ok(0)
    }
}
