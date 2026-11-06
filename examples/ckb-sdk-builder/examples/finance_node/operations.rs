//! Bounded real-node recovery experiments, separate from the frozen v2 lane.
use super::{
    backend::{cell, prefixed, Backend, Program, CELL_CAPACITY, FEE},
    bundle, checked_operation, order, original_observation, protocol,
    protocol::{Pool, Programs, Wallet},
    quote,
    recovery::{
        AmountEncoding, AssetAmountBound, FinanceIntent, FinanceIntentParameters, FinanceJournal, FinanceRecovery,
        FinanceRecoverySession,
    },
};
use anyhow::{bail, Context, Result};
use cellscript_ckb_adapter::{
    recovery::{RecoveryPolicy, RecoveryState, RecoverySubject, UnsignedAttemptFailure},
    CkbSdkAcceptance,
};
use ckb_sdk::CkbRpcClient;
use ckb_types::{core::TransactionView, packed, prelude::*, H256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub(super) fn provenance(root: &Path, node: &Backend) -> Result<Value> {
    let output = |program: &str, args: &[&str], directory: &Path| -> Result<String> {
        let result = Command::new(program).args(args).current_dir(directory).output()?;
        if !result.status.success() {
            bail!("provenance command {program} failed");
        }
        Ok(String::from_utf8(result.stdout)?.trim().to_owned())
    };
    let sdk = root.join("../ckb-sdk-rust");
    Ok(json!({"schema":"cellscript-finance-node-host-context-v1","rustc":output("rustc",&["--version","--verbose"],root)?,
        "repo_head":output("git",&["rev-parse","HEAD"],root)?,"repo_status_porcelain":output("git",&["status","--porcelain"],root)?,
        "compiled_cargo_lock_hash":prefixed(&ckb_hash::blake2b_256(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/../../Cargo.lock")))),
        "observed_cargo_lock_hash":super::file_hash(&root.join("Cargo.lock"))?,"toolchain_contract_hash":super::file_hash(&root.join("rust-toolchain.toml"))?,
        "sdk_head":output("git",&["rev-parse","HEAD"],&sdk)?,"sdk_status_porcelain":output("git",&["status","--porcelain"],&sdk)?,
        "pinned_node_source_head":output("git",&["rev-parse","HEAD"],&node.node.ckb_repo)?,
        "node_config_hash":super::file_hash(&node.run_dir.join("ckb-node/ckb.toml"))?,
        "node_chain_spec_hash":super::file_hash(&node.run_dir.join("ckb-node/specs/integration.toml"))?,
        "rpc_placement":"127.0.0.1 disposable child process; transport-fault proxy uses another local TCP listener",
        "cache_and_warmup":"fresh private chain/package artifact directory per run; no warmup; OS/page/compiler caches uncontrolled; sequential same-process repetitions",
        "source_identity_scope":"application_compiled_source_hashes identify embedded application sources; repo status records runtime checkout, not a clean published release"}))
}

fn bound(asset: &packed::Script, amount: u128) -> AssetAmountBound {
    AssetAmountBound { asset: asset.clone(), encoding: AmountEncoding::U128Le, amount }
}
fn policy() -> RecoveryPolicy {
    RecoveryPolicy { maximum_rebuilds: 1, maximum_observations: 12, required_confirmations: 1 }
}
fn fresh(node: &Backend, intent: FinanceIntent, journal: &FinanceJournal) -> Result<FinanceRecovery> {
    match FinanceRecovery::start_or_resume(intent, node.chain.clone(), H256::from(node.network_domain), policy(), journal)? {
        FinanceRecoverySession::Fresh(client) => Ok(*client),
        FinanceRecoverySession::ObservationOnly(_) => bail!("new experiment reused an existing operation"),
    }
}
fn intent(node: &Backend, pool: &Pool, wallet: &Wallet, label: &str, input: u128) -> Result<FinanceIntent> {
    FinanceIntent::new(FinanceIntentParameters {
        operation_id: H256::from(ckb_hash::blake2b_256(
            [label.as_bytes(), pool.identity.as_slice(), &pool.revision.to_le_bytes()].concat(),
        )),
        operation: "operations-exact-input-a-for-b".into(),
        owner: node.owner(wallet.key),
        recipient: node.owner(wallet.key),
        debits: vec![bound(&pool.assets[0], input), bound(&pool.assets[1], 0)],
        credits: vec![bound(&pool.assets[1], 1)],
        subjects: vec![RecoverySubject::Type(pool.identity.clone()), RecoverySubject::Lock(pool.controller.clone())],
        maximum_fee_shannons: FEE + 1,
        maximum_capacity_debit_shannons: FEE + 1,
        maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    case: String,
    occurrence: usize,
    metrics: BTreeMap<String, u64>,
    unavailable: BTreeMap<String, String>,
}
pub(super) struct Clock {
    start: Instant,
    quote_index: usize,
    build_index: usize,
    encoding_ns: u64,
    encoding_calls: u64,
    resolve_ns: u64,
    record_index: usize,
}
impl Clock {
    pub(super) fn start(node: &mut Backend, programs: &Programs, points: &[packed::OutPoint]) -> Result<Self> {
        let start = Instant::now();
        let (encoding_ns, encoding_calls) = encoding_totals(programs);
        let quote_index = node.quote_events.borrow().len();
        let build_index = node.packing_build_events.borrow().len();
        let resolve_start = Instant::now();
        for point in points {
            node.refresh(point)?;
        }
        Ok(Self {
            start,
            quote_index,
            build_index,
            encoding_ns,
            encoding_calls,
            resolve_ns: ns(resolve_start)?,
            record_index: node.records.len(),
        })
    }
    fn finish(self, node: &Backend, programs: &Programs, case: &str, occurrence: usize) -> Result<Sample> {
        let (encoding_ns, encoding_calls) = encoding_totals(programs);
        let mut metrics = BTreeMap::from([
            ("whole_operation_ns".into(), ns(self.start)?),
            ("known_predecessor_live_resolution_ns".into(), self.resolve_ns),
            (
                "canonical_data_and_witness_encoding_ns".into(),
                encoding_ns.checked_sub(self.encoding_ns).context("encoding clock regressed")?,
            ),
            ("canonical_encoding_calls".into(), encoding_calls - self.encoding_calls),
            ("quote_ns".into(), node.quote_events.borrow()[self.quote_index..].iter().map(|event| event.elapsed_ns).sum()),
            ("quote_calls".into(), (node.quote_events.borrow().len() - self.quote_index) as u64),
            (
                "packing_build_ns".into(),
                node.packing_build_events.borrow()[self.build_index..].iter().map(|event| event.elapsed_ns).sum(),
            ),
            ("packing_build_calls".into(), (node.packing_build_events.borrow().len() - self.build_index) as u64),
        ]);
        let mut records = node.records[self.record_index..].iter().collect::<Vec<_>>();
        for record in &node.records[self.record_index..] {
            for key in ["candidate_a_presend_acceptance", "candidate_b_presend_acceptance", "before_fill", "before_cancel"] {
                if record[key].is_object() {
                    records.push(&record[key]);
                }
            }
        }
        let mut transactions = 0;
        let mut rpc_observed = false;
        let mut maxima = BTreeMap::from([
            ("maximum_transaction_bytes", 0),
            ("maximum_witness_bytes", 0),
            ("maximum_occupied_output_capacity_shannons", 0),
            ("maximum_fee_shannons", 0),
            ("maximum_available_rpc_cycles", 0),
        ]);
        for record in records {
            if let Some(bytes) = record["transaction"].as_str() {
                let raw = hex::decode(bytes.strip_prefix("0x").context("transaction hex prefix")?)?;
                let tx = packed::Transaction::from_slice(&raw)?.into_view();
                if tx.outputs().len() != tx.outputs_data().len() {
                    bail!("transaction output/data mismatch");
                }
                let mut occupied = 0u64;
                let mut output_capacity = 0u64;
                for (output, data) in tx.outputs().into_iter().zip(tx.outputs_data()) {
                    occupied = occupied
                        .checked_add(output.occupied_capacity(ckb_types::core::Capacity::bytes(data.raw_data().len())?)?.as_u64())
                        .context("occupied overflow")?;
                    output_capacity = output_capacity.checked_add(output.capacity().unpack()).context("output capacity overflow")?;
                }
                let input_capacity = record["inputs"].as_array().context("actual input context missing")?.iter().try_fold(
                    0u64,
                    |sum, input| -> Result<u64> {
                        let output = packed::CellOutput::from_slice(&hex::decode(
                            input["output"].as_str().context("input output bytes missing")?.trim_start_matches("0x"),
                        )?)?;
                        sum.checked_add(output.capacity().unpack()).context("input capacity overflow")
                    },
                )?;
                for (key, value) in [
                    ("maximum_transaction_bytes", raw.len() as u64),
                    ("maximum_witness_bytes", tx.witnesses().into_iter().map(|w| w.raw_data().len() as u64).sum()),
                    ("maximum_occupied_output_capacity_shannons", occupied),
                    ("maximum_fee_shannons", input_capacity.checked_sub(output_capacity).context("capacity creation")?),
                ] {
                    let old = maxima.get_mut(key).context("metric missing")?;
                    *old = (*old).max(value);
                }
                transactions += 1;
            }
            for path in [
                "/node_admission/signed_node_execution/aggregate_cycles",
                "/node_admission/estimate_cycles/cycles",
                "/node_admission/tx_pool/aggregate_dry_run_cycles",
                "/node_admission/tx_pool/cycles",
                "/tx_pool_evidence/aggregate_dry_run_cycles",
                "/tx_pool_evidence/tx_pool_cycles",
            ] {
                if let Some(value) = record.pointer(path) {
                    let Some(cycles) = rpc_cycle_value(value)? else {
                        continue;
                    };
                    rpc_observed = true;
                    let old = maxima.get_mut("maximum_available_rpc_cycles").context("cycles metric missing")?;
                    *old = (*old).max(cycles);
                }
            }
        }
        // Whole-operation timing includes argument construction, allocation,
        // metadata encoding, packing, checking, signing, send and observation.
        // The nested phase times overlap and must not be added together.
        metrics.insert("retained_transaction_record_count".into(), transactions);
        let mut unavailable = BTreeMap::from([
            ("dynamic_indexer_discovery_ns".into(), "static admitted identities; no indexer query is performed".into()),
            (
                "observed_vm_stack_bytes".into(),
                "node RPC exposes no observed guest stack; separate VM cost evidence owns this metric".into(),
            ),
            ("observed_vm_peak_memory_bytes".into(), "node RPC exposes no observed guest peak memory".into()),
        ]);
        for (key, value) in maxima {
            if value == 0 {
                if key == "maximum_available_rpc_cycles" && !rpc_observed {
                    unavailable.insert(key.into(), "no cycle total returned by this sample's retained RPC outcomes; rejected execution is not assigned zero cycles".into());
                    continue;
                }
                if key != "maximum_available_rpc_cycles" {
                    bail!("operation metric {key} was not observed");
                }
            }
            metrics.insert(key.into(), value);
        }
        Ok(Sample { case: case.into(), occurrence, metrics, unavailable })
    }
    pub(super) fn record(self, node: &mut Backend, programs: &Programs, case: &str) -> Result<()> {
        let occurrence = node.full_operation_samples.iter().filter(|sample| sample.case == case).count() + 1;
        let sample = self.finish(node, programs, case, occurrence)?;
        node.full_operation_samples.push(sample);
        Ok(())
    }
}
fn rpc_cycle_value(value: &Value) -> Result<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    if let Some(number) = value.as_u64() {
        return Ok(Some(number));
    }
    let encoded = value.as_str().context("RPC cycles not an unsigned integer")?;
    let digits = encoded.strip_prefix("0x").context("RPC cycles missing hex prefix")?;
    if digits.is_empty()
        || digits.len() > 1 && digits.starts_with('0')
        || !digits.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("RPC cycles are not a canonical hex quantity");
    }
    Ok(Some(u64::from_str_radix(digits, 16)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nullable_rpc_cycles_remain_distinct_from_malformed_and_measured_zero() {
        assert_eq!(rpc_cycle_value(&Value::Null).unwrap(), None);
        assert_eq!(rpc_cycle_value(&json!(0)).unwrap(), Some(0));
        assert_eq!(rpc_cycle_value(&json!("0x64")).unwrap(), Some(100));
        for value in [json!(-1), json!(1.5), json!("100"), json!("0x01"), json!("0xno"), json!(false)] {
            assert!(rpc_cycle_value(&value).is_err());
        }
    }
}

fn encoding_totals(programs: &Programs) -> (u64, u64) {
    programs
        .all()
        .iter()
        .fold((0, 0), |(time, count), program| (time + program.encoding_nanoseconds.get(), count + program.encoding_calls.get()))
}
fn ns(start: Instant) -> Result<u64> {
    u64::try_from(start.elapsed().as_nanos()).context("operation duration exceeds u64")
}
fn mark(node: &mut Backend) {
    node.records.push(json!({"case":"operations-sample-boundary","status":"matched","operations_sample_end":true}));
}

fn initialise(node: &mut Backend, programs: &Programs, wallet: &mut Wallet, assets: [packed::Script; 2], nonce: u64) -> Result<Pool> {
    let (mut pool, unsigned) = protocol::initialise(programs, node, wallet, assets, 30, nonce, 10_000, 50_000)?;
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &unsigned)?;
    let authorization = FinanceIntent::new(FinanceIntentParameters {
        operation_id: H256::from(ckb_hash::blake2b_256(pool.identity.as_slice())),
        operation: "operations-pool-initialise".into(),
        owner: node.owner(wallet.key),
        recipient: node.owner(wallet.key),
        debits: vec![bound(&pool.assets[0], 10_000), bound(&pool.assets[1], 50_000)],
        credits: vec![AssetAmountBound {
            asset: pool.lp.clone(),
            encoding: AmountEncoding::PoolShares {
                pool: pool.identity.calc_script_hash().unpack(),
                controller_code: pool.controller.code_hash().unpack(),
            },
            amount: pool.state.total_lp - quote::LOCKED_MINIMUM,
        }],
        subjects: vec![RecoverySubject::Lock(pool.controller.clone())],
        maximum_fee_shannons: FEE,
        maximum_capacity_debit_shannons: 3 * CELL_CAPACITY + FEE,
        maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
    })?;
    let points = checked_operation(node, programs, &prepared, authorization, "operations-authentic-pool-initialise")?;
    pool.points = points[..3].to_vec();
    wallet.a = points[3].clone();
    wallet.b = points[4].clone();
    wallet.lp = Some(points[5].clone());
    Ok(pool)
}
fn apply_swap(pool: &mut Pool, wallet: &mut Wallet, points: Vec<packed::OutPoint>, next: quote::PoolState) {
    pool.points = points[..3].to_vec();
    wallet.a = points[3].clone();
    wallet.b = points[4].clone();
    pool.state = next;
    pool.revision += 1;
}
fn deposit(node: &mut Backend, programs: &Programs, pool: &mut Pool, wallet: &mut Wallet) -> Result<()> {
    let clock = Clock::start(node, programs, &pool.points)?;
    let funding = wallet.funding()?;
    let (unsigned, quoted) = protocol::deposit(programs, node, pool, wallet, funding, 1_000, 7_000)?;
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &unsigned)?;
    let authorization = FinanceIntent::new(FinanceIntentParameters {
        operation_id: H256::from(ckb_hash::blake2b_256(
            [b"operations-second-provider-deposit".as_slice(), pool.identity.as_slice()].concat(),
        )),
        operation: "operations-second-provider-proportional-deposit".into(),
        owner: node.owner(wallet.key),
        recipient: node.owner(wallet.key),
        debits: vec![bound(&pool.assets[0], 1_000), bound(&pool.assets[1], 7_000)],
        credits: vec![AssetAmountBound {
            asset: pool.lp.clone(),
            encoding: AmountEncoding::PoolShares {
                pool: pool.identity.calc_script_hash().unpack(),
                controller_code: pool.controller.code_hash().unpack(),
            },
            amount: quoted.minted,
        }],
        subjects: vec![RecoverySubject::Type(pool.identity.clone())],
        maximum_fee_shannons: FEE,
        maximum_capacity_debit_shannons: CELL_CAPACITY + FEE,
        maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
    })?;
    let points = checked_operation(node, programs, &prepared, authorization, "operations-second-provider-deposit-and-refunds")?;
    pool.points = points[..3].to_vec();
    pool.state = quoted.next;
    pool.revision += 1;
    wallet.a = points[3].clone();
    wallet.b = points[4].clone();
    wallet.lp = Some(points[5].clone());
    node.records.last_mut().context("deposit record missing")?["deposit_refunds"] = json!({"used_a":quoted.used_a.to_string(),"used_b":quoted.used_b.to_string(),"unused_offered_a":quoted.refund_a.to_string(),"unused_offered_b":quoted.refund_b.to_string(),"minted_lp":quoted.minted.to_string()});
    clock.record(node, programs, "pool-second-provider-deposit")?;
    Ok(())
}
fn preflight(node: &Backend, programs: &Programs, transaction: &TransactionView) -> Result<Value> {
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, transaction)?;
    let (signed, signing) = prepared.sign(&programs.all(), node)?;
    let acceptance = CkbSdkAcceptance::new(&node.rpc);
    let dry = acceptance.dry_run_signed_protocol_bundle(&signed, &prepared.materialization, &signing)?;
    let pool = acceptance.test_signed_protocol_bundle(&signed, &prepared.materialization, &signing, &dry)?;
    Ok(json!({"tx_pool_evidence":pool,"transaction":prefixed(signed.data().as_slice()),
        "raw_transaction_hash":prefixed(signed.hash().as_slice()),"serialized_transaction_hash":prefixed(&ckb_hash::blake2b_256(signed.data().as_slice())),
        "inputs":prepared.observed_inputs.iter().map(|cell|json!({"out_point":prefixed(cell.out_point.as_slice()),"output":prefixed(cell.output.as_slice()),"data":prefixed(&cell.data)})).collect::<Vec<_>>()}))
}

fn split_wallets(
    node: &mut Backend,
    programs: &Programs,
    first: &mut Wallet,
    second: &mut Wallet,
    assets: &[packed::Script; 2],
) -> Result<()> {
    let funding = first.funding()?;
    let outputs = vec![
        protocol::token(programs, node, &assets[0], first.key, 500_000)?,
        protocol::token(programs, node, &assets[1], first.key, 500_000)?,
        protocol::token(programs, node, &assets[0], second.key, 500_000)?,
        protocol::token(programs, node, &assets[1], second.key, 500_000)?,
        cell(node.owner(first.key), None, vec![]),
    ];
    let unsigned = node.build(
        &[first.a.clone(), first.b.clone(), funding],
        outputs,
        4,
        vec![programs.asset.witness(&[])?, programs.asset.witness(&[])?],
    )?;
    let points = node.commit(&node.sign(&unsigned)?, "operations-disjoint-wallet-inventories")?;
    first.a = points[0].clone();
    first.b = points[1].clone();
    second.a = points[2].clone();
    second.b = points[3].clone();
    Ok(())
}

pub(super) fn fund_unrelated(node: &mut Backend, wallet: &mut Wallet, label: &str) -> Result<Vec<packed::OutPoint>> {
    let funding = wallet.funding()?;
    let owner = node.owner(*node.key_labels.get(3).context("C4 needs a distinct non-issuer third party")?);
    let outputs = vec![
        cell(owner.clone(), None, format!("c4-{label}-first").into_bytes()),
        {
            let cell = cell(owner, None, format!("c4-{label}-second-distinct").into_bytes());
            (cell.0.as_builder().capacity(2 * CELL_CAPACITY).build(), cell.1)
        },
        cell(node.owner(wallet.key), None, vec![]),
    ];
    let unsigned = node.build(&[funding], outputs, 2, vec![])?;
    Ok(node.commit(&node.sign(&unsigned)?, &format!("actual-c4-fund-distinct-third-party-tail-{label}"))?[..2].to_vec())
}
pub(super) fn append_unrelated(
    node: &Backend,
    tx: &TransactionView,
    tails: &[packed::OutPoint],
    change: usize,
) -> Result<TransactionView> {
    let mut inputs = tx.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
    inputs.extend_from_slice(tails);
    let mut outputs =
        tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect::<Vec<_>>();
    for tail in tails {
        outputs.push(node.live.get(tail).context("C4 tail not live")?.clone());
    }
    let witnesses = tx
        .witnesses()
        .into_iter()
        .map(|w| packed::WitnessArgs::from_slice(&w.raw_data()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    node.build(&inputs, outputs, change, witnesses)
}
pub(super) fn permutation_pair(
    node: &mut Backend,
    programs: &Programs,
    tx: TransactionView,
    input_prefix: usize,
    output_prefix: usize,
    change: usize,
    label: &str,
) -> Result<TransactionView> {
    if tx.inputs().len() < input_prefix + 2 || tx.outputs().len() < output_prefix + 2 {
        bail!("C4 requires two real trailing Cells");
    }
    let mut inputs = tx.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
    let mut outputs =
        tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect::<Vec<_>>();
    inputs.swap(input_prefix, input_prefix + 1);
    outputs.swap(output_prefix, output_prefix + 1);
    let witnesses = tx
        .witnesses()
        .into_iter()
        .map(|w| packed::WitnessArgs::from_slice(&w.raw_data()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let permuted = node.build(&inputs, outputs, change, witnesses)?;
    let input_cells = permuted
        .inputs()
        .into_iter()
        .skip(input_prefix)
        .map(|input| {
            node.live
                .get(&input.previous_output())
                .context("C4 input missing")
                .map(|(output, data)| [output.as_slice(), data.as_ref()].concat())
        })
        .collect::<Result<Vec<_>>>()?;
    let output_cells = permuted
        .outputs()
        .into_iter()
        .zip(permuted.outputs_data())
        .skip(output_prefix)
        .map(|(output, data)| [output.as_slice(), data.raw_data().as_ref()].concat())
        .collect::<Vec<_>>();
    let mut original = input_cells;
    let mut refunds = output_cells;
    original.sort();
    refunds.sort();
    if original != refunds
        || tx.inputs().into_iter().take(input_prefix).collect::<Vec<_>>()
            != permuted.inputs().into_iter().take(input_prefix).collect::<Vec<_>>()
        || tx.outputs().into_iter().take(output_prefix).collect::<Vec<_>>()
            != permuted.outputs().into_iter().take(output_prefix).collect::<Vec<_>>()
        || tx.outputs_data().into_iter().take(output_prefix).collect::<Vec<_>>()
            != permuted.outputs_data().into_iter().take(output_prefix).collect::<Vec<_>>()
    {
        bail!("C4 changed its financial prefix or whole-cell refunds");
    }
    let canonical = preflight(node, programs, &tx)?;
    let equivalent = preflight(node, programs, &permuted)?;
    node.records.push(json!({"case":format!("actual-c4-{label}-equivalent-trailing-permutations"),"status":"both-preflighted-one-selected",
        "candidate_a_presend_acceptance":canonical,"candidate_b_presend_acceptance":equivalent,
        "fixed_input_prefix":input_prefix,"fixed_output_prefix":output_prefix,"selected_raw_hash":prefixed(permuted.hash().as_slice()),"selected_for_single_commit":"permuted",
        "whole_cell_refunds_verified":true,"third_party_lock":prefixed(node.owner(node.key_labels[3]).as_slice()),
        "tail_scope":"exact original third-party Lock/type/data/capacity preserved; only trailing pairs exchange indices; no financial-prefix permutation"}));
    Ok(permuted)
}
pub(super) fn reject_tail_fee(node: &mut Backend, tx: &TransactionView, wrapper: &Program, label: &str) -> Result<()> {
    let mut outputs = tx.outputs().into_iter().collect::<Vec<_>>();
    let last = outputs.len() - 1;
    outputs[last] = outputs[last]
        .clone()
        .as_builder()
        .capacity(u64::from(outputs[last].capacity()).checked_sub(1).context("tail capacity underflow")?)
        .build();
    node.reject(
        &tx.as_advanced_builder().set_outputs(outputs).build(),
        wrapper,
        label,
        cellscript::runtime_errors::CellScriptRuntimeError::AssertionFailed.code() as i64,
    )
}

fn pool_competition(
    node: &mut Backend,
    programs: &Programs,
    pool: &mut Pool,
    first: &mut Wallet,
    second: &mut Wallet,
    occurrence: usize,
) -> Result<Value> {
    let start = node.elapsed_ms();
    let funding_a = first.funding()?;
    let funding_b = second.funding()?;
    let (a, next, _) = protocol::swap(programs, node, pool, first, funding_a, 100, quote::Direction::AForB)?;
    let (b, _, _) = protocol::swap(programs, node, pool, second, funding_b, 100, quote::Direction::AForB)?;
    let before_a = preflight(node, programs, &a)?;
    let before_b = preflight(node, programs, &b)?;
    let b = node.sign(&b)?;
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &a)?;
    let authorization = intent(node, pool, first, "repeated-pool-winner", 100)?;
    let points = checked_operation(node, programs, &prepared, authorization, "operations-repeated-pool-winner")?;
    apply_swap(pool, first, points, next);
    let stale = node.submit_stale_original(&b)?;
    let end = node.elapsed_ms();
    let result = json!({"case":"operations-homogeneous-pool-contention","status":"conflict-observed","occurrence":occurrence,
        "candidate_a_presend_acceptance":before_a,"candidate_b_presend_acceptance":before_b,"stale_submission":stale,
        "window_start_ms":start,"window_end_ms":end,"window_ms":end-start,"offered_candidates":2,"actual_submissions":2,
        "committed":1,"conflicts":1,"new_economic_retries":0,"terminal_failures":0,"serialized":true});
    node.records.push(result.clone());
    Ok(result)
}

fn nonconflicting(
    node: &mut Backend,
    programs: &Programs,
    pools: &mut [Pool; 2],
    first: &mut Wallet,
    second: &mut Wallet,
) -> Result<()> {
    let start = node.elapsed_ms();
    let funding_a = first.funding()?;
    let funding_b = second.funding()?;
    let (a, next_a, _) = protocol::swap(programs, node, &pools[0], first, funding_a, 100, quote::Direction::AForB)?;
    let (b, next_b, _) = protocol::swap(programs, node, &pools[1], second, funding_b, 100, quote::Direction::AForB)?;
    if a.inputs().into_iter().any(|a| b.inputs().into_iter().any(|b| a.previous_output() == b.previous_output())) {
        bail!("control shares an input");
    }
    let before_a = preflight(node, programs, &a)?;
    let before_b = preflight(node, programs, &b)?;
    let (left, right) = pools.split_at_mut(1);
    for (pool, wallet, tx, next) in [(&mut left[0], first, a, next_a), (&mut right[0], second, b, next_b)] {
        let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &tx)?;
        let authorization = intent(node, pool, wallet, "nonconflicting-pool-control", 100)?;
        let points = checked_operation(node, programs, &prepared, authorization, "operations-nonconflicting-pool-commit")?;
        apply_swap(pool, wallet, points, next);
    }
    node.records.push(json!({"case":"operations-nonconflicting-pool-control","status":"matched","disjoint_inputs":true,
        "candidate_a_presend_acceptance":before_a,"candidate_b_presend_acceptance":before_b,"window_ms":node.elapsed_ms()-start,
        "offered_candidates":2,"actual_submissions":2,"committed":2,"conflicts":0,"new_economic_retries":0,"terminal_failures":0,"serialized":true}));
    Ok(())
}

fn unsigned_rebuild(node: &mut Backend, programs: &Programs, pool: &mut Pool, winner: &mut Wallet, retry: &mut Wallet) -> Result<()> {
    let funding = retry.funding()?;
    let (old, _, _) = protocol::swap(programs, node, pool, retry, funding, 100, quote::Direction::AForB)?;
    let old_prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &old)?;
    let authorization = intent(node, pool, retry, "unsigned-original-intent", 100)?;
    let commitment = authorization.recovery_intent(node.chain.clone(), H256::from(node.network_domain))?.binding_hash();
    let journal = FinanceJournal::new(node.run_dir.join("journal"))?;
    let mut client = fresh(node, authorization.clone(), &journal)?;
    client.prepare(old_prepared.candidate(), &old_prepared.observed_inputs)?;
    let old_signed = old_prepared.sign(&programs.all(), node)?;
    // The test driver can produce old signed bytes for a wrong-binding probe;
    // the recovery client has never bound them and remains unsigned Prepared.
    let funding = winner.funding()?;
    let (competing, next, _) = protocol::swap(programs, node, pool, winner, funding, 100, quote::Direction::AForB)?;
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &competing)?;
    let win_intent = intent(node, pool, winner, "state-update-before-unsigned-rebuild", 100)?;
    let points = checked_operation(node, programs, &prepared, win_intent, "operations-state-updated-before-unsigned-rebuild")?;
    apply_swap(pool, winner, points, next);
    let actual_stale =
        CkbSdkAcceptance::new(&node.rpc).test_tx_pool_accept(&old_signed.0).expect_err("original predecessor was spent");
    client.unsigned_failure(UnsignedAttemptFailure::StaleInput)?;
    for point in &pool.points {
        node.refresh(point)?;
    }
    let funding = retry.funding()?;
    let (rebuilt, next, _) = protocol::swap(programs, node, pool, retry, funding, 100, quote::Direction::AForB)?;
    let mut outputs = rebuilt.outputs().into_iter().collect::<Vec<_>>();
    let capacity = u64::from(outputs[5].capacity());
    outputs[5] = outputs[5].clone().as_builder().capacity(capacity - 1).build();
    let rebuilt = rebuilt.as_advanced_builder().set_outputs(outputs).build();
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &rebuilt)?;
    client.prepare(prepared.candidate(), &prepared.observed_inputs)?;
    if client.bind_signed(&old_signed.0, &old_signed.1).is_ok() {
        bail!("old signature bound to rebuilt candidate");
    }
    if authorization.recovery_intent(node.chain.clone(), H256::from(node.network_domain))?.binding_hash() != commitment {
        bail!("intent changed");
    }
    let (signed, signing) = prepared.sign(&programs.all(), node)?;
    client.bind_signed(&signed, &signing)?;
    let acceptance = CkbSdkAcceptance::new(&node.rpc);
    let dry = acceptance.dry_run_signed_protocol_bundle(&signed, &prepared.materialization, &signing)?;
    let pool_evidence = acceptance.test_signed_protocol_bundle(&signed, &prepared.materialization, &signing, &dry)?;
    let durable = client.begin_submission(&pool_evidence, &journal)?;
    let submission = acceptance.submit_signed_protocol_bundle(&signed, &signing, &pool_evidence)?;
    client.submitted(&submission)?;
    let points = node.complete_submitted(
        &signed,
        "operations-rebuilt-with-latest-state-new-funding-and-fee",
        json!({"signing":signing,"submission":submission,"journal":durable,"signed_node_execution":dry,"tx_pool":pool_evidence}),
    )?;
    apply_swap(pool, retry, points, next);
    node.records.push(json!({"case":"operations-unsigned-requote-preserves-original-intent","status":"matched",
        "original_hash":prefixed(old.hash().as_slice()),"rebuilt_hash":prefixed(rebuilt.hash().as_slice()),"intent_binding":prefixed(commitment.as_bytes()),
        "original_unsigned_transaction":prefixed(old.data().as_slice()),"old_signed_binding_probe":prefixed(old_signed.0.data().as_slice()),
        "original_resolved_inputs":old_prepared.observed_inputs.iter().map(|cell|json!({"out_point":prefixed(cell.out_point.as_slice()),"output":prefixed(cell.output.as_slice()),"data":prefixed(&cell.data)})).collect::<Vec<_>>(),
        "actual_stale_preflight_error":format!("{actual_stale:?}"),"rebuilt_from_actual_successor":true,"old_signature_rejected":true,
        "old_fee_shannons":FEE,"new_fee_shannons":FEE+1,"maximum_fee_shannons":FEE+1,"rebuilds":client.recovery().rebuilds(),"new_economic_retries":1}));
    Ok(())
}

fn order_competition(
    node: &mut Backend,
    programs: &Programs,
    maker: &mut Wallet,
    taker: &mut Wallet,
    assets: [packed::Script; 2],
) -> Result<()> {
    let mut order = order::initialise(programs, node, maker, assets, 901, 30)?;
    let (fill, authorization, quotation) = order::build_fill(programs, node, &order, taker, 3, 7)?;
    let (cancel, _) = order::build_cancel(programs, node, &order, maker)?;
    let before_fill = preflight(node, programs, &fill)?;
    let before_cancel = preflight(node, programs, &cancel)?;
    let cancel = node.sign(&cancel)?;
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &fill)?;
    let points = checked_operation(node, programs, &prepared, authorization, "operations-order-fill-wins-cancel-race")?;
    taker.b = points[4].clone();
    order.points = points[..2].to_vec();
    order.quote = quotation.next;
    order.revision += 1;
    let stale = node.submit_stale_original(&cancel)?;
    node.records.push(json!({"case":"operations-same-order-fill-versus-cancel","status":"conflict-observed","before_fill":before_fill,"before_cancel":before_cancel,
        "stale_submission":stale,"offered_candidates":2,"actual_submissions":2,"committed":1,"conflicts":1,"new_economic_retries":0,"terminal_failures":0}));
    order::cancel(programs, node, &mut order, maker)?;
    Ok(())
}

/// Forward one complete SDK HTTP request to the real node, retain its actual
/// response, then close TCP without returning that response to the SDK.
fn forward_then_drop(upstream: &str) -> Result<(String, thread::JoinHandle<Result<Value>>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}", listener.local_addr()?);
    let upstream = upstream.strip_prefix("http://").context("private RPC must use local http")?.trim_end_matches('/').to_owned();
    let handle = thread::spawn(move || {
        let (mut incoming, _) = listener.accept()?;
        incoming.set_read_timeout(Some(Duration::from_secs(30)))?;
        let request = http_message(&mut incoming)?;
        let body = request.windows(4).position(|part| part == b"\r\n\r\n").context("request headers missing")? + 4;
        let value: Value = serde_json::from_slice(&request[body..])?;
        if value["method"] != "send_transaction" {
            bail!("fault proxy only forwards the original send");
        }
        let mut outgoing = TcpStream::connect(upstream)?;
        outgoing.set_read_timeout(Some(Duration::from_secs(30)))?;
        outgoing.write_all(&request)?;
        let response = http_message(&mut outgoing)?;
        let body = response.windows(4).position(|part| part == b"\r\n\r\n").context("response headers missing")? + 4;
        let value: Value = serde_json::from_slice(&response[body..])?;
        // The real node has answered. Hold that response beyond the SDK's
        // explicit deadline, then drop it, to exercise a genuine local timeout.
        thread::sleep(Duration::from_millis(300));
        incoming.shutdown(Shutdown::Both)?;
        Ok(value)
    });
    Ok((url, handle))
}
fn http_message(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            bail!("HTTP peer closed an incomplete message");
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > 16 * 1024 * 1024 {
            bail!("bounded fault proxy message too large");
        }
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end])?;
            let length = header
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse::<usize>())
                })
                .context("HTTP content-length required")??;
            if bytes.len() == end + 4 + length {
                return Ok(bytes);
            }
            if bytes.len() > end + 4 + length {
                bail!("unexpected pipelined HTTP bytes");
            }
        }
    }
}

/// Controlled wallet callback failures through the upstream message/signature
/// adapter, not a second signer or a claim about a physical wallet device.
struct FailingWallet {
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    cancelled: bool,
}
impl ckb_sdk::traits::Signer for FailingWallet {
    fn match_id(&self, _: &[u8]) -> bool {
        true
    }
    fn sign(
        &self,
        _: &[u8],
        message: &[u8],
        _: bool,
        _: &TransactionView,
    ) -> std::result::Result<ckb_types::bytes::Bytes, ckb_sdk::traits::SignerError> {
        if message.len() != 32 {
            return Err(ckb_sdk::traits::SignerError::InvalidMessage("expected actual SDK sighash".into()));
        }
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(ckb_sdk::traits::SignerError::Other(anyhow::anyhow!(if self.cancelled {
            "controlled-wallet-user-cancelled"
        } else {
            "controlled-wallet-signature-rejected"
        })))
    }
}

fn signer_callbacks(
    node: &mut Backend,
    pool: &Pool,
    wallet: &Wallet,
    prepared: &bundle::Prepared,
    journal: &FinanceJournal,
) -> Result<()> {
    use ckb_sdk::unlock::{ScriptSigner, SecpSighashScriptSigner};
    for cancelled in [false, true] {
        let label = if cancelled { "actual-sdk-wallet-cancelled" } else { "actual-sdk-wallet-rejected" };
        let mut client = fresh(node, intent(node, pool, wallet, label, 100)?, journal)?;
        client.prepare(prepared.candidate(), &prepared.observed_inputs)?;
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let signer = SecpSighashScriptSigner::new(Box::new(FailingWallet { calls: calls.clone(), cancelled }));
        let owner = node.owner(wallet.key);
        let mut group = ckb_sdk::types::ScriptGroup::from_lock_script(&owner);
        group.input_indices =
            prepared.inputs.iter().enumerate().filter_map(|(index, (output, _))| (output.lock() == owner).then_some(index)).collect();
        let error = signer.sign_tx(&prepared.transaction, &group).expect_err("controlled wallet callback failed");
        if calls.load(std::sync::atomic::Ordering::SeqCst) != 1
            || !error.to_string().contains(if cancelled {
                "controlled-wallet-user-cancelled"
            } else {
                "controlled-wallet-signature-rejected"
            })
        {
            bail!("failure did not reach the wallet callback");
        }
        client.unsigned_failure(if cancelled {
            UnsignedAttemptFailure::SignerCancelled
        } else {
            UnsignedAttemptFailure::SignerRejected
        })?;
        if client.recovery().state() != if cancelled { RecoveryState::Cancelled } else { RecoveryState::Rejected } {
            bail!("wrong wallet failure transition");
        }
        if client.prepare(prepared.candidate(), &prepared.observed_inputs).is_ok() {
            bail!("failed wallet restored execution rights");
        }
        node.records.push(json!({"case":label,"status":if cancelled{"signer-cancelled"}else{"signer-rejected"},"sdk_error":error.to_string(),"wallet_callback_calls":1,"submitted":false,"new_economic_retries":0,
            "scope":"controlled wallet callback on actual resolved candidate via upstream SecpSighashScriptSigner; no hardware/user claim"}));
    }
    Ok(())
}

fn rpc_fault_and_reorg(node: &mut Backend, programs: &Programs, pool: &mut Pool, wallet: &mut Wallet) -> Result<()> {
    let funding = wallet.funding()?;
    let (unsigned, next, _) = protocol::swap(programs, node, pool, wallet, funding, 100, quote::Direction::AForB)?;
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &unsigned)?;
    let authorization = intent(node, pool, wallet, "controlled-transport-fault-original", 100)?;
    let journal = FinanceJournal::new(node.run_dir.join("journal"))?;
    signer_callbacks(node, pool, wallet, &prepared, &journal)?;
    let mut client = fresh(node, authorization.clone(), &journal)?;
    client.prepare(prepared.candidate(), &prepared.observed_inputs)?;
    // Exercise the actual SDK's failure path with no supplied Lock unlockers.
    let provider = ckb_sdk::traits::DefaultTransactionDependencyProvider::new(&node.node.rpc_url, 100);
    let signer_error = cellscript_ckb_adapter::unlock_protocol_bundle_transaction(
        &prepared.transaction,
        &prepared.signing,
        &provider,
        &std::collections::HashMap::new(),
    )
    .expect_err("missing signer must fail");
    let mut rejected = fresh(node, intent(node, pool, wallet, "actual-sdk-signer-rejection", 100)?, &journal)?;
    rejected.prepare(prepared.candidate(), &prepared.observed_inputs)?;
    rejected.unsigned_failure(UnsignedAttemptFailure::SignerRejected)?;
    if rejected.recovery().state() != RecoveryState::Rejected {
        bail!("signer error not terminal");
    }
    node.records.push(json!({"case":"operations-actual-sdk-missing-signer","status":"signer-rejected","error":format!("{signer_error:#}"),"submitted":false}));
    let (signed, signing) = prepared.sign(&programs.all(), node)?;
    client.bind_signed(&signed, &signing)?;
    let acceptance = CkbSdkAcceptance::new(&node.rpc);
    let dry = acceptance.dry_run_signed_protocol_bundle(&signed, &prepared.materialization, &signing)?;
    let pool_evidence = acceptance.test_signed_protocol_bundle(&signed, &prepared.materialization, &signing, &dry)?;
    let durable = client.begin_submission(&pool_evidence, &journal)?;
    let pre_tip = node.node.rpc("get_tip_header", vec![])?;
    let live_before = node.live.clone();
    let (proxy, forwarded) = forward_then_drop(&node.node.rpc_url)?;
    let fault_client = CkbRpcClient::new_with_timeout(&proxy, Duration::from_millis(100))?;
    let fault_error = CkbSdkAcceptance::new(&fault_client)
        .submit_signed_protocol_bundle(&signed, &signing, &pool_evidence)
        .expect_err("TCP response was dropped");
    if !matches!(fault_error.downcast_ref::<ckb_sdk::RpcError>(),Some(ckb_sdk::RpcError::Http(error)) if error.is_timeout()) {
        bail!("controlled RPC failure was not the configured SDK timeout: {fault_error:#}");
    }
    let actual_response = forwarded.join().map_err(|_| anyhow::anyhow!("fault proxy panicked"))??;
    if actual_response["result"] != prefixed(signed.hash().as_slice()) {
        bail!("node did not accept forwarded original: {actual_response}");
    }
    client.submission_unknown()?;
    drop(client);
    let mut observer = match FinanceRecovery::start_or_resume(
        authorization,
        node.chain.clone(),
        H256::from(node.network_domain),
        policy(),
        &journal,
    )? {
        FinanceRecoverySession::ObservationOnly(observer) => observer,
        FinanceRecoverySession::Fresh(_) => bail!("fault recovery regained execution rights"),
    };
    let hash = observer.original_transaction_hash().clone();
    let hash_string = prefixed(hash.as_bytes());
    let pending = node.node.rpc("get_transaction", vec![json!(hash_string)])?;
    observer.observe(&node.chain, &H256::from(node.network_domain), &hash, original_observation(&pending)?)?;
    node.complete_submitted(&signed,"operations-original-commits-after-real-transport-error",json!({"journal":durable,"actual_node_forwarded_response":actual_response,"sdk_error":format!("{fault_error:?}"),"signed_node_execution":dry,"tx_pool":pool_evidence}))?;
    let first = node.node.rpc("get_transaction", vec![json!(hash_string)])?;
    observer.observe(&node.chain, &H256::from(node.network_domain), &hash, original_observation(&first)?)?;
    node.node.rpc("truncate", vec![pre_tip["hash"].clone()])?;
    let absent = node.node.rpc("get_transaction", vec![json!(hash_string)])?;
    observer.observe(&node.chain, &H256::from(node.network_domain), &hash, original_observation(&absent)?)?;
    if observer.state() != RecoveryState::Reorged {
        bail!("real truncate did not remove the observed inclusion: {absent}");
    }
    node.live = live_before;
    node.node.rpc("generate_block", vec![])?;
    // Explicit private-node test-driver rebroadcast of identical original bytes.
    // ObservationOnlyRecovery itself grants no send or signing capability.
    CkbSdkAcceptance::new(&node.rpc).send_transaction(&signed)?;
    let points = node.complete_submitted(
        &signed,
        "operations-same-signed-original-reincluded-after-truncate",
        json!({"test_driver_action":"rebroadcast-identical-original-after-actual-truncate; observer-has-no-send-api"}),
    )?;
    let second = node.node.rpc("get_transaction", vec![json!(hash_string)])?;
    observer.observe(&node.chain, &H256::from(node.network_domain), &hash, original_observation(&second)?)?;
    if first["tx_status"]["block_hash"] == second["tx_status"]["block_hash"] {
        bail!("inclusion did not change");
    }
    let tip = node.node.rpc("get_tip_header", vec![])?;
    let cellscript_ckb_adapter::recovery::RecoveryObservation::Included(inclusion) = original_observation(&second)? else {
        bail!("original not included");
    };
    observer.observe_confirmation(
        &node.chain,
        &H256::from(node.network_domain),
        &hash,
        inclusion,
        H256::from_slice(&hex::decode(tip["hash"].as_str().context("tip hash")?.trim_start_matches("0x"))?)?,
        u64::from_str_radix(tip["number"].as_str().context("tip number")?.trim_start_matches("0x"), 16)?,
    )?;
    node.records.push(json!({"case":"operations-actual-rpc-transport-fault-and-changed-inclusion","status":"original-reincluded-depth-observed",
        "fault":"controlled-forward-accepted-request-hold-300ms-beyond-SDK-100ms-deadline-then-drop-TCP-response; not-organic-outage","sdk_error":format!("{fault_error:?}"),"sdk_timeout_verified":true,
        "before_mining":pending,"first_inclusion":first,"after_actual_truncate":absent,"second_inclusion":second,
        "original_hash":hash_string,"original_signed_bytes":prefixed(signed.data().as_slice()),"reorgs_observed":observer.reorgs_observed(),
        "signatures_after_restart":0,"new_economic_retries":0,"identical_original_rebroadcasts":1,"observer_state":format!("{:?}",observer.state())}));
    apply_swap(pool, wallet, points, next);
    Ok(())
}

pub fn run(
    node: &mut Backend,
    programs: &Programs,
    first: &mut Wallet,
    second: &mut Wallet,
    assets: [packed::Script; 2],
) -> Result<Value> {
    split_wallets(node, programs, first, second, &assets)?;
    let mut pools =
        [initialise(node, programs, first, assets.clone(), 801)?, initialise(node, programs, second, assets.clone(), 802)?];
    deposit(node, programs, &mut pools[0], second)?;
    let mut samples = vec![];
    let mut windows = vec![];
    for occurrence in 1..=3 {
        mark(node);
        let mut points = pools[0].points.clone();
        points.extend([first.a.clone(), first.b.clone(), second.a.clone(), second.b.clone()]);
        let clock = Clock::start(node, programs, &points)?;
        windows.push(pool_competition(node, programs, &mut pools[0], first, second, occurrence)?);
        samples.push(clock.finish(node, programs, "homogeneous-pool-contention", occurrence)?);
    }
    mark(node);
    let clock = Clock::start(node, programs, &[first.a.clone(), first.b.clone(), second.a.clone(), second.b.clone()])?;
    nonconflicting(node, programs, &mut pools, first, second)?;
    samples.push(clock.finish(node, programs, "nonconflicting-pool-control", 1)?);
    mark(node);
    let clock = Clock::start(node, programs, &pools[0].points)?;
    unsigned_rebuild(node, programs, &mut pools[0], first, second)?;
    samples.push(clock.finish(node, programs, "unsigned-rebuild-requote-fee-funding", 1)?);
    mark(node);
    let clock = Clock::start(node, programs, &[first.a.clone(), first.b.clone(), second.a.clone(), second.b.clone()])?;
    order_competition(node, programs, second, first, assets)?;
    samples.push(clock.finish(node, programs, "same-order-fill-versus-cancel", 1)?);
    mark(node);
    let clock = Clock::start(node, programs, &pools[0].points)?;
    rpc_fault_and_reorg(node, programs, &mut pools[0], first)?;
    samples.push(clock.finish(node, programs, "rpc-forward-drop-and-actual-reorg", 1)?);
    samples.extend(node.full_operation_samples.iter().cloned());
    let mut latencies =
        windows.iter().map(|window| window["window_ms"].as_f64().context("window missing")).collect::<Result<Vec<_>>>()?;
    latencies.sort_by(f64::total_cmp);
    let total_ms: f64 = latencies.iter().sum();
    Ok(json!({"schema":"cellscript-finance-node-recovery-experiment-v1","status":"passed","samples":samples,"homogeneous_contention":{
        "pattern":"serialized-two-signed-candidates-share-one-pool-predecessor","repetitions":3,"windows":windows,
        "window_ms_samples_sorted":latencies,"minimum_ms":latencies[0],"median_ms":latencies[1],"maximum_ms":latencies[2],
        "actual_submissions":6,"committed":3,"conflicts":3,"new_economic_retries":0,"terminal_failures":0,
        "observed_commits_per_active_window_second":3000.0/total_ms,"scope":"finite-three-window-single-host-experiment; setup-and-control-excluded; serialized-not-simultaneous; no-sustainable-network-capacity-or-percentile-claim"},
        "measurement_scope":{"whole_operation":"from live predecessor re-resolution before quote/funding/argument construction through signing/preflight/submission/canonical inclusion or classified failure; includes encoding, assembly and any explicit private-node mining",
        "canonical_encoding":"actual compiler-owned fixed-data and entry-witness calls, before packed build; argument construction included only in whole-operation",
        "dynamic_indexer_discovery":null,"dynamic_indexer_discovery_unavailable_reason":"static admitted identities; exact-known predecessor live RPCs measured instead",
        "cache":"one process; initially fresh disposable chain and artifacts; no warmup; three sequential same-shape contention windows; OS/page/compiler caches uncontrolled",
        "nested_phases":"overlap whole_operation and cannot be summed"}}))
}
