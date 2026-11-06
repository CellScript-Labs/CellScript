//! Project an existing signed transaction into the public ProtocolBundle path.
//! The formal adapter remains the only transaction serializer here. External
//! secp256k1 and Type ID groups keep their actual upstream implementations.

use super::ledger::{Ledger, Program, FEE};
use cellscript::protocol_bundle::{check_protocol_bundle, ProtocolBundleInput};
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

type Result<T> = std::result::Result<T, String>;

fn prefixed(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn script(script: &packed::Script) -> Value {
    let hash_type = match script.hash_type().as_slice()[0] {
        0 => "data",
        1 => "type",
        2 => "data1",
        4 => "data2",
        other => panic!("unsupported Script hash_type {other}"),
    };
    json!({"code_hash":prefixed(script.code_hash().as_slice()),"hash_type":hash_type,"args":prefixed(&script.args().raw_data())})
}

fn point(point: &packed::OutPoint) -> Value {
    json!({"tx_hash":prefixed(point.tx_hash().as_slice()),"index":u32::from(point.index())})
}

fn dependency(dep: &packed::CellDep) -> Result<Value> {
    let kind = match dep.dep_type().as_slice()[0] {
        0 => "code",
        1 => "dep_group",
        other => return Err(format!("unsupported CellDep type {other}")),
    };
    Ok(json!({"out_point":point(&dep.out_point()),"dep_type":kind}))
}

fn cell(output: &packed::CellOutput, data: &Bytes) -> Value {
    let commitment = cellscript_artifact_checker::canonical_hash(
        "cellscript-finance-observed-cell-v1",
        &json!({"output":hex::encode(output.as_slice()),"data":hex::encode(data)}),
    )
    .unwrap();
    json!({"cell_commitment":format!("0x{commitment}"),"capacity":u64::from(output.capacity()),
        "lock":script(&output.lock()),"type":output.type_().to_opt().as_ref().map(script),"data":prefixed(data)})
}

pub(super) fn witness(index: usize, raw: &[u8]) -> Result<Value> {
    let parsed = packed::WitnessArgs::from_slice(raw).map_err(|error| format!(
        "witness[{index}] is outside the formal ProtocolBundle WitnessArgs contract: {error}; normalize empty WitnessArgs before signing"
    ))?;
    let mut value = json!({});
    for (name, bytes) in [
        ("lock", parsed.lock().to_opt()),
        ("input_type", parsed.input_type().to_opt()),
        ("output_type", parsed.output_type().to_opt()),
    ] {
        if let Some(bytes) = bytes {
            value[name] = json!(prefixed(&cellscript::ckb_blake2b256(&bytes.raw_data())));
            value[format!("{name}_bytes")] = json!(prefixed(&bytes.raw_data()));
        }
    }
    Ok(value)
}

struct Group {
    script: packed::Script,
    role: &'static str,
    inputs: Vec<usize>,
    outputs: Vec<usize>,
}

fn groups(ledger: &Ledger, tx: &TransactionView) -> Result<Vec<Group>> {
    let mut groups = BTreeMap::<(&str, Vec<u8>), Group>::new();
    for (index, input) in tx.inputs().into_iter().enumerate() {
        let (output, _) = ledger.live.get(&input.previous_output()).ok_or_else(|| format!("input[{index}] is not locally live"))?;
        for (role, script) in [("lock", Some(output.lock())), ("type", output.type_().to_opt())] {
            if let Some(script) = script {
                groups
                    .entry((role, script.as_slice().to_vec()))
                    .or_insert_with(|| Group { script: script.clone(), role, inputs: vec![], outputs: vec![] })
                    .inputs
                    .push(index);
            }
        }
    }
    for (index, output) in tx.outputs().into_iter().enumerate() {
        if let Some(script) = output.type_().to_opt() {
            groups
                .entry(("type", script.as_slice().to_vec()))
                .or_insert_with(|| Group { script: script.clone(), role: "type", inputs: vec![], outputs: vec![] })
                .outputs
                .push(index);
        }
    }
    Ok(groups.into_values().collect())
}

fn external(group: &Group) -> Result<Value> {
    let secp = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_blake160_sighash_all").unwrap();
    let kind = if group.role == "lock"
        && group.script.code_hash() == packed::CellOutput::calc_data_hash(&secp)
        && group.script.hash_type().as_slice() == [4]
        && group.script.args().raw_data().len() == 20
    {
        "upstream-secp256k1-blake160-sighash-all"
    } else if group.role == "type"
        && group.script.code_hash().as_slice() == cellscript::ckb_abi::type_id::CODE_HASH
        && group.script.hash_type().as_slice() == [1]
        && group.script.args().raw_data().len() == 32
    {
        "ckb-built-in-type-id"
    } else {
        return Err(format!(
            "unadmitted external {} group {}; supply its actual Program or an explicitly supported upstream implementation",
            group.role,
            prefixed(group.script.calc_script_hash().as_slice())
        ));
    };
    Ok(json!({"implementation":kind,"script_role":group.role,"script":script(&group.script),
        "script_hash":prefixed(group.script.calc_script_hash().as_slice()),"inputs":group.inputs,"outputs":group.outputs,
        "cellscript_artifact_admission":false,"execution":"verified-only-by-the-complete-local-transaction-verifier"}))
}

fn kind(program: &Program) -> Result<&'static str> {
    let action = program.metadata.actions.iter().any(|action| action.name == "verify");
    let lock = program.metadata.locks.iter().any(|lock| lock.name == "verify");
    match (action, lock) {
        (true, false) => Ok("action"),
        (false, true) => Ok("lock"),
        _ => Err(format!("{} must expose exactly one selected verify action/lock", program.name)),
    }
}

/// Run before committing to the local Ledger, while every input remains live.
/// This does not claim RPC, node admission, confirmation or external review.
pub fn check_and_materialize(root: &Path, programs: &[&Program], ledger: &Ledger, tx: &TransactionView) -> Result<Value> {
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let observed = groups(ledger, tx)?;
    let mut inputs = Vec::new();
    let mut live_cells = Vec::new();
    for input in tx.inputs() {
        let (output, data) = ledger.live.get(&input.previous_output()).ok_or("transaction input is not locally live")?;
        let mut value = cell(output, data);
        value["out_point"] = point(&input.previous_output());
        value["since"] = json!(u64::from(input.since()));
        inputs.push(value);
        live_cells.push((output.clone(), data.clone()));
    }
    let outputs =
        tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| cell(&output, &data.raw_data())).collect::<Vec<_>>();
    let witnesses =
        tx.witnesses().into_iter().enumerate().map(|(index, value)| witness(index, &value.raw_data())).collect::<Result<Vec<_>>>()?;
    let deps = tx.cell_deps().into_iter().map(|dep| dependency(&dep)).collect::<Result<Vec<_>>>()?;
    let mut artifacts = Vec::new();
    let mut roles = Vec::new();
    let mut witness_claims = Vec::new();
    let mut external_groups = Vec::new();
    let mut generated = Vec::new();
    let mut network = None;
    let mut custom = BTreeSet::new();
    for group in &observed {
        let matching = programs
            .iter()
            .filter(|program| {
                let expected = program.script(&[]);
                expected.code_hash() == group.script.code_hash() && expected.hash_type() == group.script.hash_type()
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            external_groups.push(external(group)?);
            continue;
        }
        if matching.len() != 1 {
            return Err("multiple Program objects claim one code identity".into());
        }
        let program = *matching[0];
        let entry_kind = kind(program)?;
        if (entry_kind == "action") != (group.role == "type") {
            return Err(format!("{} is used under an incompatible Script role", program.name));
        }
        if program.name.is_empty() || !program.name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')) {
            return Err("Program names must be safe single file-name components".into());
        }
        let context: Value = serde_json::from_slice(&program.module.context_bytes().map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        let selected_network = json!({"chain_id":context["chain_id"],"genesis_hash":context["network_genesis"]});
        if let Some(network) = &network {
            if network != &selected_network {
                return Err("frozen Program networks differ".into());
            }
        } else {
            network = Some(selected_network.clone());
        }
        let package_id = context["modules"][&program.metadata.module]["package"].as_str().ok_or("missing frozen entry package")?;
        let package = &context["packages"][package_id];
        let name = package["name"].as_str().ok_or("missing frozen package name")?;
        let version = package["version"].as_str().ok_or("missing frozen package version")?;
        let coordinate = match package["namespace"].as_str() {
            Some(namespace) => format!("{namespace}/{name}@{version}"),
            None => format!("{name}@{version}"),
        };
        let stem = &program.name;
        if custom.insert(stem.clone()) {
            for (suffix, bytes) in [
                ("elf", program.module.bundle()[0]),
                ("elf.meta.json", program.module.bundle()[1]),
                ("elf.lowering.json", program.module.bundle()[2]),
                ("elf.sourcemap.json", program.module.bundle()[3]),
            ] {
                std::fs::write(root.join(format!("{stem}.{suffix}")), bytes).map_err(|error| error.to_string())?;
            }
            std::fs::write(
                root.join(format!("{stem}.context.json")),
                program.module.context_bytes().map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
                .args(["gen-builder", "--metadata"])
                .arg(root.join(format!("{stem}.elf.meta.json")))
                .args(["--target", "typescript", "--output"])
                .arg(root.join(format!("{stem}-sdk")))
                .output()
                .map_err(|error| error.to_string())?;
            if !output.status.success() {
                return Err(format!("{} generated TS builder: {}", program.name, String::from_utf8_lossy(&output.stderr)));
            }
            generated.push(json!({"program":program.name,"manifest":format!("{stem}-sdk/cellscript-builder-manifest.json"),
                "frozen_context":program.module.context_identity(),"package_coordinate":coordinate}));
        }
        let selected_dep = tx
            .cell_deps()
            .into_iter()
            .find(|dep| ledger.context.get_cell(&dep.out_point()).is_some_and(|(_, data)| data.as_ref() == program.code()))
            .ok_or_else(|| format!("{} code CellDep is missing", program.name))?;
        let artifact_id = format!("{}-{}", program.name, hex::encode(group.script.calc_script_hash().as_slice()));
        artifacts.push(json!({"id":artifact_id,"package_coordinate":coordinate,
            "lock_node_id":format!("frozen-root:{}:{package_id}",context["lock_hash"].as_str().ok_or("missing frozen lock hash")?),
            "entry":{"kind":entry_kind,"name":"verify"},"script_role":group.role,
            "files":{"artifact":format!("{stem}.elf"),"metadata":format!("{stem}.elf.meta.json"),
                "lowering_record":format!("{stem}.elf.lowering.json"),"source_map":format!("{stem}.elf.sourcemap.json"),
                "builder_manifest":format!("{stem}-sdk/cellscript-builder-manifest.json")},
            "deployment":{"network":selected_network,"artifact_hash":hex::encode(cellscript::ckb_blake2b256(program.code())),
                "script":script(&group.script),"code_cell_dep":dependency(&selected_dep)?}}));
        let (location, index) = if let Some(index) = group.inputs.first() {
            ("input", *index)
        } else {
            ("output", *group.outputs.first().ok_or("empty Script group")?)
        };
        let mut role =
            json!({"artifact":artifact_id,"name":"script-group-root","location":location,"index":index,"ownership":"shared-read"});
        role[if group.role == "type" { "expected_type" } else { "expected_lock" }] = script(&group.script);
        roles.push(role);
        if let Some(commitment) = witnesses.get(index).and_then(|witness| witness["input_type"].as_str()) {
            witness_claims.push(json!({"artifact":artifact_id,"name":"generated-entry","index":index,
                "field":"input_type","ownership":"shared-read","abi":"cellscript-entry-witness-v1","value_commitment":commitment}));
        }
    }
    if artifacts.len() < 2 {
        return Err("formal ProtocolBundle requires at least two actual custom Script groups; external secp/Type ID must not be fabricated as CellScript artifacts".into());
    }
    let network = network.ok_or("no custom Program network")?;
    let fee_policy = cellscript_artifact_checker::canonical_hash("cellscript-finance-fee-policy-v1", &json!({"fee":FEE})).unwrap();
    let change_policy = cellscript_artifact_checker::canonical_hash(
        "cellscript-finance-exact-untyped-outputs-v1",
        &json!(outputs.iter().filter(|output| output["type"].is_null()).collect::<Vec<_>>()),
    )
    .unwrap();
    let input: ProtocolBundleInput = serde_json::from_value(json!({"schema":"cellscript-protocol-bundle-input-v1",
        "network":network,"artifacts":artifacts,"roles":roles,"witnesses":witness_claims,
        "transaction":{"version":tx.version(),"inputs":inputs,"outputs":outputs,"witnesses":witnesses,
            "cell_deps":deps,"header_deps":tx.header_deps().into_iter().map(|hash| prefixed(hash.as_slice())).collect::<Vec<_>>(),
            "fee_policy_hash":format!("0x{fee_policy}"),"change_policy_hash":format!("0x{change_policy}")}}))
    .map_err(|error| error.to_string())?;
    let report = check_protocol_bundle(&input, root).map_err(|error| error.to_string())?;
    if report.status != "ok" {
        return Err(format!(
            "ProtocolBundle conflicts: {:?}; metadata: {:?}",
            report.conflicts, report.evidence.metadata_transaction_validation
        ));
    }
    let bytes = serde_json::to_vec(&report).map_err(|error| error.to_string())?;
    let (materialized, evidence) =
        cellscript_ckb_adapter::materialize_protocol_bundle_report(&bytes).map_err(|error| error.to_string())?;
    if materialized.data().as_slice() != tx.data().as_slice() {
        return Err("formal adapter changed the signed transaction bytes".into());
    }
    if evidence.fee_shannons != FEE {
        return Err("formal materialization changed the exact fee policy".into());
    }
    let live = cellscript_ckb_adapter::protocol_bundle_live_resolution_evidence(
        &materialized,
        &evidence,
        input.network.chain_id.as_str(),
        input.network.genesis_hash.as_str(),
        &live_cells,
    )
    .map_err(|error| error.to_string())?;
    let dep_indexes =
        evidence.code_cell_dep_expectations.iter().map(|expected| expected.transaction_cell_dep_index).collect::<BTreeSet<_>>();
    let observations = dep_indexes
        .into_iter()
        .map(|index| {
            let dep = materialized.cell_deps().get(index as usize).ok_or("custom dependency index disappeared")?;
            let (output, data) = ledger.context.get_cell(&dep.out_point()).ok_or("custom code dependency is unresolved")?;
            Ok(cellscript_ckb_adapter::ProtocolBundleCellDepObservation {
                transaction_cell_dep_index: index,
                root: cellscript_ckb_adapter::ProtocolBundleLiveCellObservation { out_point: dep.out_point(), output, data },
                dep_group_members: vec![],
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let resolved =
        cellscript_ckb_adapter::protocol_bundle_dependency_resolution_evidence(&materialized, &evidence, &live, &observations)
            .map_err(|error| error.to_string())?;
    std::fs::write(root.join("protocol-input.json"), serde_json::to_vec_pretty(&input).unwrap()).map_err(|error| error.to_string())?;
    std::fs::write(root.join("protocol-report.json"), &bytes).map_err(|error| error.to_string())?;
    Ok(json!({"schema":"cellscript-finance-formal-bundle-evidence-v1","offline_report":report,
        "materialization":evidence,"local_live_resolution":live,"custom_code_resolution":resolved,
        "generated_builders":generated,"external_groups":external_groups,"identical_signed_transaction_bytes":true,
        "local_complete_transaction_vm":{"status":"not-executed-here","all_group_count":observed.len(),"chain_evidence":"not-executed"}}))
}
