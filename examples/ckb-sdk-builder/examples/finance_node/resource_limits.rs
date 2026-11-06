//! Optional, immutable ceilings for measured example operations. No routine in
//! this module generates or widens a limit, and unavailable metrics stay null.

use anyhow::{bail, Context, Result};
use ckb_types::{core::Capacity, packed, prelude::*};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Serialize,
};
use serde_json::{json, Number, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Read,
    marker::PhantomData,
    path::{Path, PathBuf},
};

const SCHEMA: &str = "cellscript-finance-node-resource-limits-v2";
const SCOPE: &str = "fixed-profile-per-case-occurrence-observations; one-host-run-samples; no-percentile-or-throughput-guarantee";
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
#[derive(Clone, Copy)]
enum Unit {
    Integer,
    Milliseconds,
    Unmeasured,
}
const METRICS: &[(&str, Unit)] = &[
    ("transaction_bytes", Unit::Integer),
    ("witness_bytes", Unit::Integer),
    ("rpc_cycles", Unit::Integer),
    ("input_capacity_shannons", Unit::Integer),
    ("output_capacity_shannons", Unit::Integer),
    ("occupied_output_capacity_shannons", Unit::Integer),
    ("fee_shannons", Unit::Integer),
    ("artifact_check_materialization_and_live_resolution_ms", Unit::Milliseconds),
    ("commit_observation_ms", Unit::Milliseconds),
    ("single_action_estimate_ms", Unit::Milliseconds),
    ("single_action_tx_pool_ms", Unit::Milliseconds),
    ("single_action_send_ms", Unit::Milliseconds),
    ("intent_recovery_prepare_ms", Unit::Milliseconds),
    ("sdk_sign_and_receipt_ms", Unit::Milliseconds),
    ("signed_execution_rpc_and_receipt_ms", Unit::Milliseconds),
    ("tx_pool_rpc_and_receipt_ms", Unit::Milliseconds),
    ("durable_journal_ms", Unit::Milliseconds),
    ("submission_rpc_and_receipt_ms", Unit::Milliseconds),
    ("canonical_confirmation_rpc_ms", Unit::Milliseconds),
    ("operation_after_materialization_ms", Unit::Milliseconds),
    ("discarded_ack_send_ms", Unit::Milliseconds),
    ("node_rejection_dry_run_ms", Unit::Milliseconds),
    ("stale_original_send_ms", Unit::Milliseconds),
    ("contention_window_ms", Unit::Milliseconds),
    ("external_metadata_encoding_ns", Unit::Unmeasured),
    ("dynamic_indexer_discovery_ms", Unit::Unmeasured),
    ("observed_vm_stack_bytes", Unit::Unmeasured),
    ("observed_vm_peak_memory_bytes", Unit::Unmeasured),
];
type Metrics = BTreeMap<String, Option<Number>>;

fn unique_map<'de, D, V>(deserializer: D) -> std::result::Result<BTreeMap<String, V>, D::Error>
where
    D: de::Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct Unique<V>(PhantomData<V>);
    impl<'de, V: Deserialize<'de>> Visitor<'de> for Unique<V> {
        type Value = BTreeMap<String, V>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an object with unique keys")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, V>()? {
                if values.insert(key.clone(), value).is_some() {
                    return Err(de::Error::custom(format!("duplicate key {key}")));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Unique(PhantomData))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub lane: String,
    pub fee_bps: u64,
    pub pool_nonce: u64,
    pub reserve_a: String,
    pub reserve_b: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    pub os: String,
    pub arch: String,
    pub available_parallelism: Option<usize>,
    pub cpu_model: Option<String>,
}
pub fn machine() -> Machine {
    let cpu_model = fs::read_to_string("/proc/cpuinfo").ok().and_then(|text| {
        text.lines().find_map(|line| {
            line.strip_prefix("model name").and_then(|line| line.split_once(':')).map(|(_, model)| model.trim().to_owned())
        })
    });
    Machine {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        available_parallelism: std::thread::available_parallelism().ok().map(|v| v.get()),
        cpu_model,
    }
}
pub fn checker_source_hash() -> String {
    super::backend::prefixed(&ckb_hash::blake2b_256(include_str!("resource_limits.rs").as_bytes()))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ContentionPattern {
    pattern: String,
    offered_candidates: u64,
    actual_submission_attempts: u64,
    committed: u64,
    submission_conflicts: u64,
    new_economic_retries: u64,
    terminal: bool,
    serialized_contenders: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Window {
    pattern: String,
    start_elapsed_ms: f64,
    end_elapsed_ms: f64,
    duration_ms: f64,
    offered_candidates: u64,
    actual_submission_attempts: u64,
    committed: u64,
    submission_conflicts: u64,
    new_economic_retries: u64,
    terminal: bool,
    serialized_contenders: bool,
    scope: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseObservation {
    case: String,
    occurrence: u32,
    status: String,
    metrics: Metrics,
    contention: Option<ContentionPattern>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteEvent {
    pub operation: String,
    pub elapsed_ns: u64,
    pub success: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildEvent {
    pub raw_transaction_hash: String,
    pub elapsed_ns: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TimingObservation {
    operation: String,
    occurrence: u32,
    elapsed_ns: u64,
    success: bool,
    raw_transaction_hash: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimingLimit {
    operation: String,
    occurrence: u32,
    success: bool,
    observed_max_ns: u64,
    upper_ns: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observations {
    schema: String,
    pub profile: Profile,
    pub machine: Machine,
    pub total_wall_ms: f64,
    pub cases: Vec<CaseObservation>,
    quote_events: Vec<TimingObservation>,
    packing_build_events: Vec<TimingObservation>,
}

fn empty_metrics() -> Metrics {
    METRICS.iter().map(|(key, _)| ((*key).into(), None)).collect()
}
fn set(metrics: &mut Metrics, key: &str, value: impl Into<Number>) {
    *metrics.get_mut(key).expect("closed metric key") = Some(value.into());
}
fn copy_number(metrics: &mut Metrics, key: &str, value: Option<&Value>) -> Result<()> {
    if let Some(value) = value.filter(|v| !v.is_null()) {
        let number = value.as_number().context("measured metric is not a JSON number")?;
        set(metrics, key, number.clone());
    }
    Ok(())
}
fn bytes(text: &Value) -> Result<Vec<u8>> {
    let text = text.as_str().context("missing canonical packed bytes")?.strip_prefix("0x").context("missing hex prefix")?;
    let bytes = hex::decode(text)?;
    if hex::encode(&bytes) != text {
        bail!("noncanonical packed hex");
    }
    Ok(bytes)
}
fn packed_metrics(case: &Value, metrics: &mut Metrics) -> Result<()> {
    let Some(transaction) = case.get("transaction") else {
        return Ok(());
    };
    let raw = bytes(transaction)?;
    let tx = packed::Transaction::from_slice(&raw)?.into_view();
    if tx.outputs().len() != tx.outputs_data().len() {
        bail!("resource observation output/data cardinality differs");
    }
    if case["raw_transaction_hash"] != super::backend::prefixed(tx.hash().as_slice())
        || case["serialized_transaction_hash"] != super::backend::prefixed(&ckb_hash::blake2b_256(&raw))
    {
        bail!("resource observation transaction identity differs");
    }
    let inputs = case["inputs"].as_array().context("resource observation has no resolved inputs")?;
    if inputs.len() != tx.inputs().len() {
        bail!("resource observation input cardinality differs");
    }
    let mut input_capacity = 0u64;
    for (input, observed) in tx.inputs().into_iter().zip(inputs) {
        if observed["out_point"] != super::backend::prefixed(input.previous_output().as_slice()) {
            bail!("resource observation input identity differs");
        }
        let output = packed::CellOutput::from_slice(&bytes(&observed["output"])?)?;
        input_capacity = input_capacity.checked_add(u64::from(output.capacity())).context("input capacity overflow")?;
    }
    let mut output_capacity = 0u64;
    let mut occupied = 0u64;
    for (output, data) in tx.outputs().into_iter().zip(tx.outputs_data()) {
        output_capacity = output_capacity.checked_add(u64::from(output.capacity())).context("output capacity overflow")?;
        occupied = occupied
            .checked_add(output.occupied_capacity(Capacity::bytes(data.raw_data().len())?)?.as_u64())
            .context("occupied capacity overflow")?;
    }
    set(metrics, "transaction_bytes", raw.len() as u64);
    set(metrics, "witness_bytes", tx.witnesses().into_iter().map(|w| w.raw_data().len() as u64).sum::<u64>());
    set(metrics, "input_capacity_shannons", input_capacity);
    set(metrics, "output_capacity_shannons", output_capacity);
    set(metrics, "occupied_output_capacity_shannons", occupied);
    set(metrics, "fee_shannons", input_capacity.checked_sub(output_capacity).context("candidate creates capacity")?);
    Ok(())
}

fn unsigned_rpc(value: &Value) -> Result<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    if let Some(number) = value.as_u64() {
        return Ok(Some(number));
    }
    let text = value.as_str().context("RPC count is neither u64 nor canonical hex")?;
    let value = u64::from_str_radix(text.strip_prefix("0x").context("RPC count has no hex prefix")?, 16)?;
    if text != format!("0x{value:x}") {
        bail!("RPC count is not canonical hex");
    }
    Ok(Some(value))
}
fn rpc_peak(case: &Value) -> Result<Option<u64>> {
    let mut peak = None;
    // A summary can retain a complete-transaction preflight/commit cycle
    // result. Budget the largest available result, never invent a sum or a
    // measured total from a rejected RPC's diagnostic text.
    for path in [
        "/node_admission/signed_node_execution/aggregate_cycles",
        "/node_admission/estimate_cycles/cycles",
        "/node_admission/tx_pool/tx_pool_cycles",
        "/node_admission/tx_pool/aggregate_dry_run_cycles",
        "/node_admission/tx_pool/cycles",
        "/candidate_a_presend_acceptance/tx_pool_cycles",
        "/candidate_a_presend_acceptance/aggregate_dry_run_cycles",
        "/candidate_b_presend_acceptance/cycles",
        "/presend_acceptance/cycles",
        "/after_commit/cycles",
        "/before_mining/cycles",
    ] {
        if let Some(value) = case.pointer(path)
            && let Some(cycles) = unsigned_rpc(value)?
        {
            peak = Some(peak.map_or(cycles, |old: u64| old.max(cycles)));
        }
    }
    Ok(peak)
}

pub fn observe(report: &Value, profile: Profile) -> Result<Observations> {
    let mut occurrences = BTreeMap::<String, u32>::new();
    let mut cases = vec![];
    for value in report["cases"].as_array().context("node report has no cases")? {
        let case = value["case"].as_str().context("node case has no name")?.to_owned();
        let occurrence = occurrences.entry(case.clone()).or_default();
        *occurrence += 1;
        let status = value["status"].as_str().context("node case has no status")?.to_owned();
        let mut metrics = empty_metrics();
        packed_metrics(value, &mut metrics)?;
        let cycles = rpc_peak(value)?;
        if let Some(cycles) = cycles {
            set(&mut metrics, "rpc_cycles", cycles);
        }
        for (key, path) in [
            (
                "artifact_check_materialization_and_live_resolution_ms",
                "/node_admission/formal_bundle/artifact_check_materialization_and_live_resolution_ms",
            ),
            ("commit_observation_ms", "/commit_observation_ms"),
            ("single_action_estimate_ms", "/node_admission/timings_ms/estimate"),
            ("single_action_tx_pool_ms", "/node_admission/timings_ms/preflight"),
            ("single_action_send_ms", "/node_admission/timings_ms/send"),
            ("intent_recovery_prepare_ms", "/client_timings_ms/intent_recovery_prepare"),
            ("sdk_sign_and_receipt_ms", "/client_timings_ms/sdk_sign_and_receipt"),
            ("signed_execution_rpc_and_receipt_ms", "/client_timings_ms/signed_execution_rpc_and_receipt"),
            ("tx_pool_rpc_and_receipt_ms", "/client_timings_ms/tx_pool_rpc_and_receipt"),
            ("durable_journal_ms", "/client_timings_ms/durable_journal"),
            ("submission_rpc_and_receipt_ms", "/client_timings_ms/submission_rpc_and_receipt"),
            ("canonical_confirmation_rpc_ms", "/client_timings_ms/canonical_confirmation_rpc"),
            ("operation_after_materialization_ms", "/client_timings_ms/operation_after_materialization"),
            ("discarded_ack_send_ms", "/node_admission/send_ms"),
            ("node_rejection_dry_run_ms", "/dry_run_ms"),
            ("stale_original_send_ms", "/stale_submission/elapsed_ms"),
        ] {
            copy_number(&mut metrics, key, value.pointer(path))?;
        }
        let contention = if let Some(window) = value.get("contention_window") {
            let window: Window = serde_json::from_value(window.clone())?;
            if !window.start_elapsed_ms.is_finite()
                || window.start_elapsed_ms < 0.0
                || !window.end_elapsed_ms.is_finite()
                || window.end_elapsed_ms < window.start_elapsed_ms
                || !window.duration_ms.is_finite()
                || (window.duration_ms - (window.end_elapsed_ms - window.start_elapsed_ms)).abs() > 0.000001
                || window.scope != "one-local-serialized-two-candidate-pattern; stale-original-send-after-commit; excludes-financial-invalid-probes; no-load-percentile"
            {
                bail!("invalid actual contention window");
            }
            let submission = &value["stale_submission"];
            let error = &submission["rpc_error"];
            let data = error["data"].as_str().unwrap_or("");
            if window.offered_candidates != 2
                || window.actual_submission_attempts != 2
                || window.committed != 1
                || window.submission_conflicts != 1
                || window.new_economic_retries != 0
                || window.terminal
                || !window.serialized_contenders
                || submission["method"] != "send_transaction"
                || submission["status"] != "stale-input-rpc-rejected"
                || submission["same_previously_signed_original"] != true
                || submission["rebuilt"] != false
                || submission["resigned"] != false
                || submission["new_economic_retry"] != false
                || error["code"].as_i64() != Some(-301)
                || !(data.contains("Resolve(Unknown(OutPoint(") || data.contains("Resolve(Dead(OutPoint("))
            {
                bail!("contention outcome lacks the original stale submission rejection");
            }
            set(&mut metrics, "contention_window_ms", Number::from_f64(window.duration_ms).context("invalid contention duration")?);
            Some(ContentionPattern {
                pattern: window.pattern,
                offered_candidates: window.offered_candidates,
                actual_submission_attempts: window.actual_submission_attempts,
                committed: window.committed,
                submission_conflicts: window.submission_conflicts,
                new_economic_retries: window.new_economic_retries,
                terminal: window.terminal,
                serialized_contenders: window.serialized_contenders,
            })
        } else {
            None
        };
        validate_metrics(&metrics)?;
        cases.push(CaseObservation { case, occurrence: *occurrence, status, metrics, contention });
    }
    let quotes: Vec<QuoteEvent> = serde_json::from_value(report["quote_events"].clone())?;
    let mut quote_occurrences = BTreeMap::<String, u32>::new();
    let quote_events = quotes
        .into_iter()
        .map(|event| {
            let occurrence = quote_occurrences.entry(event.operation.clone()).or_default();
            *occurrence += 1;
            TimingObservation {
                operation: event.operation,
                occurrence: *occurrence,
                elapsed_ns: event.elapsed_ns,
                success: event.success,
                raw_transaction_hash: None,
            }
        })
        .collect();
    let builds: Vec<BuildEvent> = serde_json::from_value(report["packing_build_events"].clone())?;
    let mut packing_build_events = vec![];
    for (index, event) in builds.into_iter().enumerate() {
        if !hash(&event.raw_transaction_hash) {
            bail!("packing event has an invalid transaction identity");
        }
        packing_build_events.push(TimingObservation {
            operation: "capacity-balance-resolved-action-and-packed-materialization".into(),
            occurrence: u32::try_from(index + 1)?,
            elapsed_ns: event.elapsed_ns,
            success: true,
            raw_transaction_hash: Some(event.raw_transaction_hash),
        });
    }
    let total_wall_ms = report["total_wall_ms"].as_f64().context("total wall time missing")?;
    if !total_wall_ms.is_finite() || total_wall_ms < 0.0 {
        bail!("total wall time is not finite/nonnegative");
    }
    Ok(Observations {
        schema: "cellscript-finance-node-resource-observations-v2".into(),
        profile,
        machine: machine(),
        total_wall_ms,
        cases,
        quote_events,
        packing_build_events,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    profile: String,
    report: String,
    report_hash: String,
    machine: Option<Machine>,
    kind: String,
    use_for: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseLimit {
    case: String,
    occurrence: u32,
    status: String,
    #[serde(deserialize_with = "unique_map")]
    observed_max: Metrics,
    #[serde(deserialize_with = "unique_map")]
    upper: Metrics,
    contention: Option<ContentionPattern>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileLimits {
    profile: Profile,
    observed_total_wall_ms: f64,
    total_wall_upper_ms: f64,
    cases: Vec<CaseLimit>,
    quote_events: Vec<TimingLimit>,
    packing_build_events: Vec<TimingLimit>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Specification {
    schema: String,
    scope: String,
    checker_source_hash: String,
    ckb_binary_hash: String,
    compiler_version: String,
    network_chain_id: String,
    network_genesis_hash: String,
    #[serde(deserialize_with = "unique_map")]
    protocol_elf_hashes: BTreeMap<String, String>,
    headroom: String,
    baselines: Vec<Baseline>,
    profiles: Vec<ProfileLimits>,
}
pub struct Limits {
    path: PathBuf,
    hash: String,
    specification: Specification,
}
const HEADROOM:&str="integers=ceil(observed_max*5/4); stage_ms=ceil(observed_max*3+25); journal_ms=ceil(observed_max*3+1000); contention_ms=ceil(observed_max*3+1000); quote_and_packing_ns=observed_max*3+1000000; total_wall_ms=ceil(observed_max*3+1000)";

fn timing_upper(observed: u64) -> Result<u64> {
    observed.checked_mul(3).and_then(|n| n.checked_add(1_000_000)).context("nanosecond ceiling overflow")
}
fn validate_timings(events: &[TimingLimit]) -> Result<()> {
    if events.is_empty() {
        bail!("measured timing event inventory is empty");
    }
    let mut occurrences = BTreeMap::<&str, u32>::new();
    for event in events {
        let occurrence = occurrences.entry(&event.operation).or_default();
        *occurrence += 1;
        if event.operation.is_empty() || *occurrence != event.occurrence || event.upper_ns != timing_upper(event.observed_max_ns)? {
            bail!("timing occurrence/explicit ceiling differs");
        }
    }
    Ok(())
}
fn check_timings(actual: &[TimingObservation], limits: &[TimingLimit], label: &str, violations: &mut Vec<String>) {
    if actual.len() != limits.len() {
        violations.push(format!("{label} timing inventory cardinality differs"));
    }
    for (event, limit) in actual.iter().zip(limits) {
        if event.operation != limit.operation
            || event.occurrence != limit.occurrence
            || event.success != limit.success
            || event.elapsed_ns > limit.upper_ns
        {
            violations.push(format!(
                "{label} {}#{} identity/outcome/ceiling differs: {}ns (limit {}ns)",
                event.operation, event.occurrence, event.elapsed_ns, limit.upper_ns
            ));
        }
    }
}

fn validate_metrics(metrics: &Metrics) -> Result<()> {
    if metrics.len() != METRICS.len() || METRICS.iter().any(|(key, _)| !metrics.contains_key(*key)) {
        bail!("metric inventory differs from closed schema");
    }
    for (key, unit) in METRICS {
        if let Some(number) = &metrics[*key] {
            match unit {
                Unit::Integer => {
                    if number.as_u64().is_none() {
                        bail!("{key} is not a u64 measurement");
                    }
                }
                Unit::Milliseconds => {
                    if number.as_f64().is_none_or(|n| !n.is_finite() || n < 0.0) {
                        bail!("{key} is not a finite nonnegative measurement");
                    }
                }
                Unit::Unmeasured => bail!("{key} is unavailable and must remain null"),
            }
        }
    }
    Ok(())
}
fn upper(key: &str, number: &Number, unit: Unit) -> Result<Number> {
    Ok(match unit {
        Unit::Integer => Number::from(u64::try_from((u128::from(number.as_u64().context("non-integer baseline")?) * 5).div_ceil(4))?),
        Unit::Milliseconds => Number::from_f64(
            (number.as_f64().context("non-numeric baseline")? * 3.0
                + if matches!(key, "contention_window_ms" | "durable_journal_ms") { 1000.0 } else { 25.0 })
            .ceil(),
        )
        .context("ceiling overflow")?,
        Unit::Unmeasured => bail!("cannot freeze unavailable metric"),
    })
}
fn hash(value: &str) -> bool {
    value.strip_prefix("0x").is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}
impl Limits {
    pub fn load(path: &Path, profile: &Profile) -> Result<Self> {
        let mut bytes = vec![];
        fs::File::open(path)?.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            bail!("resource limits file exceeds size bound");
        }
        let specification: Specification = serde_json::from_slice(&bytes)?;
        let limits = Self { path: path.to_path_buf(), hash: super::backend::prefixed(&ckb_hash::blake2b_256(&bytes)), specification };
        limits.validate(profile)?;
        Ok(limits)
    }
    fn validate(&self, profile: &Profile) -> Result<()> {
        let spec = &self.specification;
        if spec.schema != SCHEMA
            || spec.scope != SCOPE
            || spec.headroom != HEADROOM
            || spec.checker_source_hash != checker_source_hash()
            || !hash(&spec.ckb_binary_hash)
            || spec.network_chain_id.is_empty()
            || !hash(&spec.network_genesis_hash)
            || spec.protocol_elf_hashes.is_empty()
            || spec.protocol_elf_hashes.values().any(|h| !hash(h))
        {
            bail!("resource specification scope/checker/identity differs");
        }
        let mut profiles = BTreeSet::new();
        for entry in &spec.profiles {
            if !profiles.insert(entry.profile.name.as_str()) {
                bail!("duplicate resource profile");
            }
            if !entry.observed_total_wall_ms.is_finite()
                || entry.observed_total_wall_ms < 0.0
                || entry.total_wall_upper_ms != (entry.observed_total_wall_ms * 3.0 + 1000.0).ceil()
            {
                bail!("invalid explicit wall-time ceiling");
            }
            validate_timings(&entry.quote_events)?;
            validate_timings(&entry.packing_build_events)?;
            let mut keys = BTreeSet::new();
            let mut occurrences = BTreeMap::<&str, u32>::new();
            for case in &entry.cases {
                if !keys.insert((case.case.as_str(), case.occurrence)) {
                    bail!("duplicate case occurrence");
                }
                let occurrence = occurrences.entry(&case.case).or_default();
                *occurrence += 1;
                if *occurrence != case.occurrence {
                    bail!("non-contiguous case occurrence");
                }
                validate_metrics(&case.observed_max)?;
                validate_metrics(&case.upper)?;
                for (key, unit) in METRICS {
                    match (&case.observed_max[*key], &case.upper[*key]) {
                        (None, None) => {}
                        (Some(observed), Some(limit)) if upper(key, observed, *unit)? == *limit => {}
                        _ => bail!("{key} ceiling does not match explicit frozen headroom"),
                    }
                }
            }
        }
        if !spec.profiles.iter().any(|entry| &entry.profile == profile) {
            bail!("resource limits do not admit these exact profile parameters");
        }
        let mut baseline_hashes = BTreeSet::new();
        for baseline in &spec.baselines {
            if !profiles.contains(baseline.profile.as_str())
                || baseline.report.is_empty()
                || !hash(&baseline.report_hash)
                || !baseline_hashes.insert(&baseline.report_hash)
                || !matches!(baseline.kind.as_str(), "existing-fixed-client" | "operational-pilot")
                || (baseline.kind == "operational-pilot" && baseline.machine.is_none())
                || baseline.use_for.is_empty()
                || baseline.use_for.iter().any(|scope| {
                    !matches!(scope.as_str(), "existing-frozen-ceilings" | "v2-stale-send-and-contention" | "rpc-summary-coverage")
                })
            {
                bail!("invalid frozen baseline provenance");
            }
            let _ = &baseline.machine;
        }
        if spec.baselines.len() < 2
            || !spec.baselines.iter().any(|b| {
                b.profile == profile.name
                    && b.kind == "operational-pilot"
                    && b.use_for.iter().any(|scope| scope == "v2-stale-send-and-contention")
            })
        {
            bail!("an actual operational pilot is required before freezing this profile");
        }
        Ok(())
    }
    pub fn check(&self, report: &Value, observed: &Observations) -> Value {
        let mut violations = vec![];
        let spec = &self.specification;
        let Some(profile) = spec.profiles.iter().find(|p| p.profile == observed.profile) else {
            return json!({"status":"failed","violations":["exact profile differs"]});
        };
        if report["ckb_binary_hash"] != spec.ckb_binary_hash
            || report["compiler_version"] != spec.compiler_version
            || report["chain_id"] != spec.network_chain_id
            || report["genesis_header_hash"] != spec.network_genesis_hash
            || serde_json::to_value(&spec.protocol_elf_hashes).ok().as_ref() != report.get("protocol_elf_hashes")
        {
            violations.push("pinned node/compiler/protocol ELF identities differ".into());
        }
        if observed.total_wall_ms > profile.total_wall_upper_ms {
            violations.push("total wall time exceeds frozen measured-scope ceiling".into());
        }
        if observed.cases.len() != profile.cases.len() {
            violations.push("case inventory cardinality differs".into());
        }
        for (actual, limit) in observed.cases.iter().zip(&profile.cases) {
            let context = format!("{}#{}", actual.case, actual.occurrence);
            if actual.case != limit.case || actual.occurrence != limit.occurrence || actual.status != limit.status {
                violations.push(format!("{context} identity/status/order differs"));
                continue;
            }
            if actual.contention != limit.contention {
                violations.push(format!("{context} contention outcomes differ"));
            }
            for (key, unit) in METRICS {
                match (&actual.metrics[*key], &limit.upper[*key]) {
                    (None, None) => {}
                    (Some(actual), Some(limit)) => {
                        let exceeded = match unit {
                            Unit::Integer => actual.as_u64() > limit.as_u64(),
                            _ => actual.as_f64() > limit.as_f64(),
                        };
                        if exceeded {
                            violations.push(format!("{context} {key} exceeds frozen ceiling: {actual} > {limit}"));
                        }
                    }
                    _ => violations.push(format!("{context} {key} measured/unavailable status differs")),
                }
            }
        }
        check_timings(&observed.quote_events, &profile.quote_events, "pure-quote", &mut violations);
        check_timings(&observed.packing_build_events, &profile.packing_build_events, "packing-build", &mut violations);
        json!({"status":if violations.is_empty(){"passed-measured-scope"}else{"failed"},"limits_path":self.path,"limits_hash":self.hash,"scope":SCOPE,
            "checker_source_hash":checker_source_hash(),"checked_case_occurrences":observed.cases.len(),"observed_machine":observed.machine,"violations":violations,
            "unmeasured":["external-metadata-data/witness-encoding","dynamic-indexer-discovery-not-used-by-static-seeded-profile","VM-stack","VM-peak-memory"],"automatic_widening":false,"latency_percentile_claim":false,"universal_throughput_claim":false})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nullable_metrics_cannot_be_silently_zeroed_or_omitted() {
        let mut values = empty_metrics();
        assert!(validate_metrics(&values).is_ok());
        values.remove("external_metadata_encoding_ns");
        assert!(validate_metrics(&values).is_err());
        let mut values = empty_metrics();
        set(&mut values, "external_metadata_encoding_ns", 0);
        assert!(validate_metrics(&values).is_err());
        let mut values = empty_metrics();
        values.insert("rpc_cycles".into(), Some(Number::from(-1)));
        assert!(validate_metrics(&values).is_err());
    }
    #[test]
    fn duplicate_metric_keys_and_unknown_schema_fields_are_rejected() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(deserialize_with = "unique_map")]
            values: Metrics,
        }
        assert!(serde_json::from_str::<Wrapper>(r#"{"values":{"a":null,"a":0}}"#).is_err());
        let valid: Wrapper = serde_json::from_str(r#"{"values":{"a":null}}"#).unwrap();
        assert!(valid.values["a"].is_none());
        assert!(serde_json::from_value::<Profile>(
            json!({"name":"reference","lane":"complete","fee_bps":30,"pool_nonce":1,"reserve_a":"1","reserve_b":"2","unknown":true})
        )
        .is_err());
    }
    #[test]
    fn ceiling_uses_exact_integer_headroom_and_explicit_timing_margin() {
        assert_eq!(upper("rpc_cycles", &Number::from(5), Unit::Integer).unwrap(), Number::from(7));
        assert_eq!(upper("sdk_sign_and_receipt_ms", &Number::from(2), Unit::Milliseconds).unwrap().as_f64(), Some(31.0));
        assert_eq!(upper("contention_window_ms", &Number::from(2), Unit::Milliseconds).unwrap().as_f64(), Some(1006.0));
    }

    fn fixture() -> (Limits, Observations, Value) {
        let profile = Profile {
            name: "reference".into(),
            lane: "complete".into(),
            fee_bps: 30,
            pool_nonce: 1,
            reserve_a: "100000".into(),
            reserve_b: "500000".into(),
        };
        let mut metrics = empty_metrics();
        set(&mut metrics, "rpc_cycles", 100u64);
        let mut ceiling = metrics.clone();
        set(&mut ceiling, "rpc_cycles", 125u64);
        let hash = |n: u8| super::super::backend::prefixed(&[n; 32]);
        let timing = json!({"operation":"synthetic-timing","occurrence":1,"success":true,"observed_max_ns":100,"upper_ns":1000300});
        let specification:Specification=serde_json::from_value(json!({"schema":SCHEMA,"scope":SCOPE,"headroom":HEADROOM,"checker_source_hash":checker_source_hash(),
            "ckb_binary_hash":hash(1),"compiler_version":"test","network_chain_id":"test","network_genesis_hash":hash(2),"protocol_elf_hashes":{"asset":hash(3)},
            "baselines":[{"profile":"reference","report":"old","report_hash":hash(4),"machine":null,"kind":"existing-fixed-client","use_for":["existing-frozen-ceilings"]},
                {"profile":"reference","report":"pilot","report_hash":hash(5),"machine":machine(),"kind":"operational-pilot","use_for":["existing-frozen-ceilings","v2-stale-send-and-contention"]}],
            "profiles":[{"profile":profile,"observed_total_wall_ms":10.0,"total_wall_upper_ms":1030.0,"quote_events":[timing],"packing_build_events":[timing],
                "cases":[{"case":"synthetic-budget-test","occurrence":1,"status":"matched","observed_max":metrics,"upper":ceiling,"contention":null}]}]})).unwrap();
        let limits = Limits { path: "fixture.json".into(), hash: hash(6), specification };
        let observations = Observations {
            schema: "cellscript-finance-node-resource-observations-v2".into(),
            profile,
            machine: machine(),
            total_wall_ms: 10.0,
            cases: vec![CaseObservation {
                case: "synthetic-budget-test".into(),
                occurrence: 1,
                status: "matched".into(),
                metrics,
                contention: None,
            }],
            quote_events: vec![TimingObservation {
                operation: "synthetic-timing".into(),
                occurrence: 1,
                elapsed_ns: 100,
                success: true,
                raw_transaction_hash: None,
            }],
            packing_build_events: vec![TimingObservation {
                operation: "synthetic-timing".into(),
                occurrence: 1,
                elapsed_ns: 100,
                success: true,
                raw_transaction_hash: Some(hash(7)),
            }],
        };
        let report = json!({"ckb_binary_hash":hash(1),"compiler_version":"test","chain_id":"test","genesis_header_hash":hash(2),"protocol_elf_hashes":{"asset":hash(3)}});
        (limits, observations, report)
    }
    #[test]
    fn fixed_limits_reject_overrun_missing_cases_and_changed_identity_without_widening() {
        let (limits, mut observed, mut report) = fixture();
        limits.validate(&observed.profile).unwrap();
        assert_eq!(limits.check(&report, &observed)["status"], "passed-measured-scope");
        set(&mut observed.cases[0].metrics, "rpc_cycles", 126u64);
        assert_eq!(limits.check(&report, &observed)["status"], "failed");
        assert_eq!(limits.specification.profiles[0].cases[0].upper["rpc_cycles"].as_ref().unwrap().as_u64(), Some(125));
        observed.cases.clear();
        assert_eq!(limits.check(&report, &observed)["status"], "failed");
        let (_, observed, _) = fixture();
        report["protocol_elf_hashes"]["asset"] = json!("changed");
        assert_eq!(limits.check(&report, &observed)["status"], "failed");
        let mut different = observed.profile.clone();
        different.reserve_a = "100001".into();
        assert!(limits.validate(&different).is_err());
    }
    #[test]
    fn handwritten_ceiling_widening_and_contention_outcome_changes_fail() {
        let (mut limits, observed, report) = fixture();
        set(&mut limits.specification.profiles[0].cases[0].upper, "rpc_cycles", 126u64);
        assert!(limits.validate(&observed.profile).is_err());
        let (mut limits, observed, report2) = fixture();
        limits.specification.profiles[0].cases[0].contention = Some(ContentionPattern {
            pattern: "test".into(),
            offered_candidates: 2,
            actual_submission_attempts: 2,
            committed: 1,
            submission_conflicts: 1,
            new_economic_retries: 0,
            terminal: false,
            serialized_contenders: true,
        });
        assert_eq!(limits.check(&report2, &observed)["status"], "failed");
        assert_eq!(report, report2);
    }
    #[test]
    fn nanosecond_events_must_be_observed_and_respect_fixed_occurrence_limits() {
        assert!(serde_json::from_value::<QuoteEvent>(json!({"operation":"quote","success":true})).is_err());
        let (limits, mut observed, report) = fixture();
        observed.quote_events[0].elapsed_ns = 1_000_301;
        assert_eq!(limits.check(&report, &observed)["status"], "failed");
        observed.quote_events.clear();
        assert_eq!(limits.check(&report, &observed)["status"], "failed");
        let (mut limits, observed, _) = fixture();
        limits.specification.profiles[0].packing_build_events[0].upper_ns += 1;
        assert!(limits.validate(&observed.profile).is_err());
        assert!(timing_upper(u64::MAX).is_err());
    }
    #[test]
    fn every_available_rpc_cycle_result_is_bounded_without_parsing_rejection_text() {
        let values = json!({"candidate_a_presend_acceptance":{"tx_pool_cycles":9,"aggregate_dry_run_cycles":13},"candidate_b_presend_acceptance":{"cycles":"0xc"},"after_commit":{"cycles":"0xa"},"before_mining":{"cycles":"0xe"}});
        assert_eq!(rpc_peak(&values).unwrap(), Some(14));
        assert_eq!(rpc_peak(&json!({"observation":{"reason":"error used 999999 cycles"}})).unwrap(), None);
        assert!(rpc_peak(&json!({"presend_acceptance":{"cycles":"unavailable"}})).is_err());
        assert!(unsigned_rpc(&json!("0x01")).is_err());
        assert!(unsigned_rpc(&json!(-1)).is_err());
    }
    #[test]
    fn malformed_report_time_and_output_data_cardinality_fail_closed() {
        let (_, observed, _) = fixture();
        let report = json!({"cases":[],"quote_events":[],"packing_build_events":[],"total_wall_ms":-1});
        assert!(observe(&report, observed.profile).unwrap_err().to_string().contains("finite/nonnegative"));
        let tx = packed::Transaction::new_builder()
            .raw(packed::RawTransaction::new_builder().outputs(vec![packed::CellOutput::default()]).build())
            .build()
            .into_view();
        let case = json!({"transaction":super::super::backend::prefixed(tx.data().as_slice()),"inputs":[],"raw_transaction_hash":super::super::backend::prefixed(tx.hash().as_slice()),
            "serialized_transaction_hash":super::super::backend::prefixed(&ckb_hash::blake2b_256(tx.data().as_slice()))});
        assert!(packed_metrics(&case, &mut empty_metrics()).unwrap_err().to_string().contains("output/data cardinality"));
    }
}
