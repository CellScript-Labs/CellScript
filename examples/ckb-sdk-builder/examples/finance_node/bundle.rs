//! Application composition through the existing public ProtocolBundle path.

use super::backend::{prefixed, Backend, Program};
use anyhow::{bail, Context, Result};
use cellscript::protocol_bundle::{check_protocol_bundle, ProtocolBundleInput};
use cellscript_ckb_adapter::*;
use ckb_sdk::{
    traits::{DefaultTransactionDependencyProvider, SecpCkbRawKeySigner, TransactionDependencyProvider},
    types::{ScriptGroup, ScriptId},
    unlock::{ScriptUnlocker, SecpSighashScriptSigner, SecpSighashUnlocker, UnlockError},
};
use ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
    time::Instant,
};

fn script(value: &packed::Script) -> Value {
    json!({"code_hash":prefixed(value.code_hash().as_slice()),"hash_type":match value.hash_type().as_slice()[0]{0=>"data",1=>"type",2=>"data1",4=>"data2",_=>unreachable!()},"args":prefixed(&value.args().raw_data())})
}
fn point(value: &packed::OutPoint) -> Value {
    json!({"tx_hash":prefixed(value.tx_hash().as_slice()),"index":u32::from(value.index())})
}
fn dependency(value: &packed::CellDep) -> Value {
    json!({"out_point":point(&value.out_point()),"dep_type":if value.dep_type().as_slice()==[0]{"code"}else{"dep_group"}})
}
fn cell(output: &packed::CellOutput, data: &Bytes) -> Result<Value> {
    let commitment = cellscript_artifact_checker::canonical_hash(
        "cellscript-finance-observed-cell-v1",
        &json!({"output":hex::encode(output.as_slice()),"data":hex::encode(data)}),
    )?;
    Ok(
        json!({"cell_commitment":format!("0x{commitment}"),"capacity":u64::from(output.capacity()),"lock":script(&output.lock()),"type":output.type_().to_opt().as_ref().map(script),"data":prefixed(data)}),
    )
}

pub struct Prepared {
    pub transaction: TransactionView,
    pub materialization: ProtocolBundleMaterializationEvidence,
    pub live: ProtocolBundleLiveResolutionEvidence,
    pub dependencies: ProtocolBundleDependencyResolutionEvidence,
    pub signing: ProtocolBundleReadyToSignEvidence,
    pub inputs: Vec<(packed::CellOutput, Bytes)>,
    pub observed_inputs: Vec<ProtocolBundleLiveCellObservation>,
    pub report: Value,
}

pub fn prepare(artifacts: &Path, programs: &[&Program], backend: &Backend, tx: &TransactionView) -> Result<Prepared> {
    let started = Instant::now();
    let mut groups = BTreeMap::<(&str, Vec<u8>), (packed::Script, Vec<usize>, Vec<usize>)>::new();
    let mut inputs = vec![];
    let mut actual_inputs = vec![];
    let mut observed_inputs = vec![];
    for (index, input) in tx.inputs().into_iter().enumerate() {
        let (output, data) = backend.live.get(&input.previous_output()).context("unresolved candidate input")?;
        let mut value = cell(output, data)?;
        value["out_point"] = point(&input.previous_output());
        value["since"] = json!(u64::from(input.since()));
        inputs.push(value);
        actual_inputs.push((output.clone(), data.clone()));
        observed_inputs.push(ProtocolBundleLiveCellObservation {
            out_point: input.previous_output(),
            output: output.clone(),
            data: data.clone(),
        });
        for (role, group) in [("lock", Some(output.lock())), ("type", output.type_().to_opt())] {
            if let Some(group) = group {
                groups.entry((role, group.as_slice().to_vec())).or_insert_with(|| (group, vec![], vec![])).1.push(index);
            }
        }
    }
    let outputs = tx
        .outputs()
        .into_iter()
        .zip(tx.outputs_data())
        .enumerate()
        .map(|(index, (output, data))| {
            if let Some(group) = output.type_().to_opt() {
                groups.entry(("type", group.as_slice().to_vec())).or_insert_with(|| (group, vec![], vec![])).2.push(index);
            }
            cell(&output, &data.raw_data())
        })
        .collect::<Result<Vec<_>>>()?;
    let witnesses = tx
        .witnesses()
        .into_iter()
        .map(|bytes| {
            let parsed = packed::WitnessArgs::from_slice(&bytes.raw_data())?;
            let mut result = json!({});
            for (name, value) in [
                ("lock", parsed.lock().to_opt()),
                ("input_type", parsed.input_type().to_opt()),
                ("output_type", parsed.output_type().to_opt()),
            ] {
                if let Some(value) = value {
                    result[name] = json!(prefixed(&ckb_hash::blake2b_256(value.raw_data())));
                    result[format!("{name}_bytes")] = json!(prefixed(&value.raw_data()));
                }
            }
            Ok(result)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut artifacts_input = vec![];
    let mut roles = vec![];
    let mut claims = vec![];
    let mut external = vec![];
    for ((role, _), (group, group_inputs, group_outputs)) in groups {
        let matching = programs
            .iter()
            .filter(|program| program.script(&[]).code_hash() == group.code_hash() && group.hash_type().as_slice() == [4])
            .collect::<Vec<_>>();
        if matching.is_empty() {
            let implementation = if role == "lock" && group.code_hash() == backend.secp_code {
                "upstream-secp256k1-sighash"
            } else if role == "type" && group.code_hash().as_slice() == cellscript::ckb_abi::type_id::CODE_HASH {
                "ckb-built-in-type-id"
            } else {
                bail!("unadmitted external Script group {}", prefixed(group.calc_script_hash().as_slice()));
            };
            external.push(json!({"implementation":implementation,"script":script(&group),"script_role":role,"inputs":group_inputs,"outputs":group_outputs}));
            continue;
        }
        if matching.len() != 1 {
            bail!("ambiguous compiled code identity");
        }
        let program = matching[0];
        let context: Value = serde_json::from_slice(&program.module.context_bytes()?)?;
        let package_id = context["modules"][&program.metadata.module]["package"].as_str().context("missing package ID")?;
        let package = &context["packages"][package_id];
        let name = package["name"].as_str().context("package name missing")?;
        let version = package["version"].as_str().context("version missing")?;
        let coordinate = match package["namespace"].as_str() {
            Some(namespace) => format!("{namespace}/{name}@{version}"),
            None => format!("{name}@{version}"),
        };
        let code_point = backend.code_points.get(&program.name).context("actual code Cell deployment missing")?;
        let dep = tx.cell_deps().into_iter().find(|dep| dep.out_point() == *code_point).context("actual code CellDep missing")?;
        let id = format!("{}-{}", program.name, hex::encode(group.calc_script_hash().as_slice()));
        let stem = &program.name;
        artifacts_input.push(json!({"id":id,"package_coordinate":coordinate,"lock_node_id":format!("frozen-root:{}:{package_id}",context["lock_hash"].as_str().context("lock hash missing")?),
            "entry":{"kind":if program.action{"action"}else{"lock"},"name":"verify"},"script_role":role,
            "files":{"artifact":format!("{stem}.elf"),"metadata":format!("{stem}.elf.meta.json"),"lowering_record":format!("{stem}.elf.lowering.json"),"source_map":format!("{stem}.elf.sourcemap.json"),"builder_manifest":format!("{stem}-sdk/cellscript-builder-manifest.json")},
            "deployment":{"network":{"chain_id":backend.chain,"genesis_hash":backend.genesis},"artifact_hash":hex::encode(ckb_hash::blake2b_256(program.code())),"script":script(&group),"code_cell_dep":dependency(&dep)}}));
        let (location, index) = group_inputs
            .first()
            .map(|index| ("input", *index))
            .or_else(|| group_outputs.first().map(|index| ("output", *index)))
            .context("empty Script group")?;
        let mut claim = json!({"artifact":id,"name":"script-group-root","location":location,"index":index,"ownership":"shared-read"});
        claim[if role == "type" { "expected_type" } else { "expected_lock" }] = script(&group);
        roles.push(claim);
        if let Some(commitment) = witnesses[index]["input_type"].as_str() {
            claims.push(json!({"artifact":id,"name":"generated-entry","index":index,"field":"input_type","ownership":"shared-read","abi":"cellscript-entry-witness-v1","value_commitment":commitment}));
        }
    }
    if artifacts_input.len() < 2 {
        bail!("use the ordinary single-action adapter for fewer than two custom groups");
    }
    let fee = cellscript_artifact_checker::canonical_hash("cellscript-finance-fee-policy-v1", &json!({"fee":super::backend::FEE}))?;
    let change = cellscript_artifact_checker::canonical_hash(
        "cellscript-finance-exact-untyped-outputs-v1",
        &json!(outputs.iter().filter(|output| output["type"].is_null()).collect::<Vec<_>>()),
    )?;
    let input: ProtocolBundleInput = serde_json::from_value(
        json!({"schema":"cellscript-protocol-bundle-input-v1","network":{"chain_id":backend.chain,"genesis_hash":backend.genesis},
        "artifacts":artifacts_input,"roles":roles,"witnesses":claims,
        "transaction":{"version":tx.version(),"inputs":inputs,"outputs":outputs,"witnesses":witnesses,"cell_deps":tx.cell_deps().into_iter().map(|dep|dependency(&dep)).collect::<Vec<_>>(),
            "header_deps":tx.header_deps().into_iter().map(|hash|prefixed(hash.as_slice())).collect::<Vec<_>>(),"fee_policy_hash":format!("0x{fee}"),"change_policy_hash":format!("0x{change}")}}),
    )?;
    let report = check_protocol_bundle(&input, artifacts)?;
    if report.status != "ok" {
        bail!(
            "public ProtocolBundle rejected: {:?}; metadata {:?}",
            report.conflicts,
            report.evidence.metadata_transaction_validation
        );
    }
    let bytes = serde_json::to_vec(&report)?;
    let (transaction, materialization) = materialize_protocol_bundle_report(&bytes)?;
    if transaction.data().as_slice() != tx.data().as_slice() {
        bail!("formal adapter changed exact candidate bytes");
    }
    let acceptance = CkbSdkAcceptance::new(&backend.rpc);
    let live = acceptance.verify_protocol_bundle_live_inputs(&transaction, &materialization)?;
    let dependencies = acceptance.verify_protocol_bundle_live_dependencies(&transaction, &materialization, &live)?;
    let signing = protocol_bundle_ready_to_sign_evidence(&transaction, &materialization, &live, &dependencies)?;
    let hash = hex::encode(tx.hash().as_slice());
    fs::write(artifacts.join(format!("{hash}.protocol-input.json")), serde_json::to_vec_pretty(&input)?)?;
    fs::write(artifacts.join(format!("{hash}.protocol-report.json")), &bytes)?;
    let report = json!({"offline_report":report,"materialization":materialization,"live_resolution":live,"dependency_resolution":dependencies,"ready_to_sign":signing,"external_groups":external,
        "artifact_check_materialization_and_live_resolution_ms":started.elapsed().as_secs_f64()*1000.0});
    Ok(Prepared { transaction, materialization, live, dependencies, signing, inputs: actual_inputs, observed_inputs, report })
}

/// This SDK hook supplies no financial authorization. It only recognizes that
/// compiler-owned entry witnesses were already built; the real node still
/// executes every compiled policy Lock. No verifier bytes are replaced.
struct ProvidedPolicyWitness {
    empty_entry: bool,
}
#[async_trait::async_trait]
impl ScriptUnlocker for ProvidedPolicyWitness {
    fn match_args(&self, _: &[u8]) -> bool {
        true
    }
    async fn is_unlocked_async(
        &self,
        tx: &TransactionView,
        group: &ScriptGroup,
        _: &dyn TransactionDependencyProvider,
    ) -> std::result::Result<bool, UnlockError> {
        if self.empty_entry {
            return Ok(true);
        }
        let Some(index) = group.input_indices.first() else {
            return Ok(false);
        };
        let Some(witness) = tx.witnesses().get(*index) else {
            return Ok(false);
        };
        let parsed = packed::WitnessArgs::from_slice(&witness.raw_data()).map_err(|_| UnlockError::InvalidWitnessArgs(*index))?;
        Ok(parsed.input_type().to_opt().is_some_and(|value| value.raw_data().starts_with(b"CSARGv1\0")))
    }
    async fn unlock_async(
        &self,
        _: &TransactionView,
        _: &ScriptGroup,
        _: &dyn TransactionDependencyProvider,
    ) -> std::result::Result<TransactionView, UnlockError> {
        Err(UnlockError::Other(anyhow::anyhow!("compiler-owned policy witness missing")))
    }
    async fn fill_placeholder_witness_async(
        &self,
        tx: &TransactionView,
        _: &ScriptGroup,
        _: &dyn TransactionDependencyProvider,
    ) -> std::result::Result<TransactionView, UnlockError> {
        Ok(tx.clone())
    }
}

impl Prepared {
    pub fn sign(
        &self,
        programs: &[&Program],
        backend: &Backend,
    ) -> Result<(TransactionView, ProtocolBundleSignedTransactionEvidence)> {
        let raw = SecpCkbRawKeySigner::new_with_secret_keys(
            backend
                .key_labels
                .iter()
                .map(|key| secp256k1::SecretKey::from_slice(&[*key; 32]))
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
        let mut unlockers: HashMap<ScriptId, Box<dyn ScriptUnlocker>> = HashMap::new();
        unlockers.insert(
            ScriptId::from(&backend.owner(backend.key_labels[0])),
            Box::new(SecpSighashUnlocker::new(SecpSighashScriptSigner::new(Box::new(raw)))),
        );
        for program in programs.iter().filter(|program| !program.action) {
            let empty_entry = program.witness(&[]).is_ok();
            unlockers.insert(ScriptId::from(&program.script(&[])), Box::new(ProvidedPolicyWitness { empty_entry }));
        }
        let provider = DefaultTransactionDependencyProvider::new(&backend.node.rpc_url, 100);
        unlock_protocol_bundle_transaction(&self.transaction, &self.signing, &provider, &unlockers)
    }
    pub fn candidate(&self) -> cellscript_ckb_adapter::recovery::RecoveryCandidate<'_> {
        cellscript_ckb_adapter::recovery::RecoveryCandidate {
            transaction: &self.transaction,
            materialization: &self.materialization,
            resolved_inputs: &self.inputs,
            live_resolution: &self.live,
            dependency_resolution: &self.dependencies,
        }
    }
}
