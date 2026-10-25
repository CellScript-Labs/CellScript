//! Reconstruct source-bound roles from materialized outputs and declared resolved
//! inputs. This does not authenticate source sidecars or replace live resolution.
use super::*;
use cellscript_artifact_checker::cell_selection::transaction::{Catalog, Cell, CurrentScript, Selection};
use std::collections::BTreeMap;

pub(super) fn verify(bundle: &BundleWire, evidence: &Value, tx: &TransactionView) -> Result<BTreeMap<String, Vec<Selection>>> {
    let expected: BTreeMap<String, Vec<Selection>> = evidence
        .get("exact_role_selections")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .context("invalid exact-role selection evidence")?
        .unwrap_or_default();
    let selected = bundle.artifacts.iter().filter(|artifact| artifact.exact_role_contracts.is_some()).collect::<Vec<_>>();
    if (bundle.schema == PROTOCOL_BUNDLE_SELECTION_SCHEMA) != !selected.is_empty() || selected.len() != expected.len() {
        bail!("exact-role catalog/evidence coverage differs from the versioned bundle schema");
    }
    let mut actual = BTreeMap::new();
    for artifact in selected {
        let catalog = artifact.exact_role_contracts.as_ref().expect("selected catalog");
        if matches!(artifact.script_role, ScriptRoleWire::SpawnedVerifier) {
            bail!("spawned verifier selection requires its caller's actual Script context");
        }
        if bundle.transaction.inputs.len() > 64 || tx.outputs().len() > 64 {
            bail!("exact-role selection exceeds complete scan profile");
        }
        let script = parse_script("exact-role current Script", &artifact.deployment.script)?;
        let current = CurrentScript { hash: script_hash(&script), is_type: matches!(artifact.script_role, ScriptRoleWire::Type) };
        let inputs = bundle
            .transaction
            .inputs
            .iter()
            .map(|cell| {
                let type_script = cell.type_script.as_ref().map(|script| parse_script("exact-role input Type", script)).transpose()?;
                let type_hash = type_script.as_ref().map(script_hash);
                let data = if protected(catalog, type_hash) {
                    let bytes = cell
                        .data
                        .as_ref()
                        .filter(|bytes| bytes.len() <= 2 + 2 * 32768)
                        .ok_or_else(|| anyhow::anyhow!("protected input requires bounded concrete data"))?;
                    parse_hex("protected input data", bytes)?
                } else {
                    Vec::new()
                };
                Ok((type_hash, script_hash(&parse_script("exact-role input Lock", &cell.lock)?), data))
            })
            .collect::<Result<Vec<_>>>()?;
        let outputs = tx
            .outputs()
            .into_iter()
            .zip(tx.outputs_data())
            .map(|(cell, data)| {
                let type_hash = cell.type_().to_opt().as_ref().map(script_hash);
                (type_hash, script_hash(&cell.lock()), data.raw_data().to_vec())
            })
            .collect::<Vec<_>>();
        let result = catalog
            .verify(current, &views(&inputs), &views(&outputs))
            .with_context(|| format!("artifact '{}' materialized exact-role selection", artifact.id))?;
        if expected.get(&artifact.id) != Some(&result) {
            bail!("artifact '{}' materialized exact-role selection differs from checked evidence", artifact.id);
        }
        actual.insert(artifact.id.clone(), result);
    }
    Ok(actual)
}

fn script_hash(script: &Script) -> [u8; 32] {
    script.calc_script_hash().as_slice().try_into().expect("packed Script hash is 32 bytes")
}

fn protected(catalog: &Catalog, type_hash: Option<[u8; 32]>) -> bool {
    catalog.sites.iter().any(|site| site.contract.roles.iter().any(|role| Some(role.type_hash) == type_hash))
}

type OwnedCell = (Option<[u8; 32]>, [u8; 32], Vec<u8>);
fn views(cells: &[OwnedCell]) -> Vec<Cell<'_>> {
    cells.iter().map(|(type_hash, lock_hash, data)| Cell { type_hash: *type_hash, lock_hash: *lock_hash, data }).collect()
}
