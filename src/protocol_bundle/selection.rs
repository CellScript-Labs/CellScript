//! Source-bound role reconstruction before a ProtocolBundle is materialized.
use super::*;
use cellscript_artifact_checker::cell_selection::transaction::{self, Catalog, Cell, CurrentScript, Selection};

pub(super) fn project(metadata: &CompileMetadata, entry: &ProtocolEntryIdentity) -> Result<Option<Catalog>> {
    let kind = match entry.kind {
        ProtocolEntryKind::Action => "action",
        ProtocolEntryKind::Lock => "lock",
        ProtocolEntryKind::Function => "helper",
    };
    transaction::project(&metadata.typed_semantics, kind, &entry.name)
        .map_err(|error| CompileError::without_span(format!("ProtocolBundle exact-role contract: {error}")))
}

pub(super) fn verify(artifact: &ProtocolArtifactIdentity, tx: &ProtocolTransactionSkeleton) -> Result<Vec<Selection>> {
    let catalog = artifact.exact_role_contracts.as_ref().ok_or_else(|| CompileError::without_span("missing role contracts"))?;
    if artifact.script_role == ProtocolScriptRole::SpawnedVerifier {
        return Err(CompileError::without_span("spawned verifier selection requires the caller's actual Script context"));
    }
    let current =
        CurrentScript { hash: hash(&artifact.deployment.script)?, is_type: artifact.script_role == ProtocolScriptRole::Type };
    let input = cells(catalog, &tx.inputs)?;
    let output = cells(catalog, &tx.outputs)?;
    catalog.verify(current, &views(&input), &views(&output)).map_err(|error| {
        CompileError::without_span(format!("ProtocolBundle artifact '{}' exact-role selection: {error}", artifact.id))
    })
}

fn hash(script: &ProtocolScriptIdentity) -> Result<[u8; 32]> {
    let hash = crate::script_handle::ckb_script_identity_hash(script)?;
    crate::hex_decode(hash.strip_prefix("0x").unwrap_or(&hash))
        .ok_or_else(|| CompileError::without_span("invalid complete Script hash"))
}

type OwnedCell = (Option<[u8; 32]>, [u8; 32], Vec<u8>);

fn cells(catalog: &Catalog, cells: &[ProtocolCellSlot]) -> Result<Vec<OwnedCell>> {
    if cells.len() > 64 {
        return Err(CompileError::without_span("exact-role transaction exceeds its complete scan profile"));
    }
    cells
        .iter()
        .map(|cell| {
            let type_hash = cell.r#type.as_ref().map(hash).transpose()?;
            let protected = catalog.sites.iter().any(|site| site.contract.roles.iter().any(|role| Some(role.type_hash) == type_hash));
            let data = if protected {
                let bytes = cell
                    .data
                    .as_ref()
                    .filter(|bytes| bytes.len() <= 2 + 2 * 32768)
                    .ok_or_else(|| CompileError::without_span("protected input/output requires bounded concrete data"))?;
                canonical_hex_bytes(bytes, "protected Cell data")?;
                hex::decode(&bytes[2..]).map_err(|_| CompileError::without_span("invalid protected Cell data"))?
            } else {
                Vec::new()
            };
            Ok((type_hash, hash(&cell.lock)?, data))
        })
        .collect()
}

fn views(cells: &[OwnedCell]) -> Vec<Cell<'_>> {
    cells.iter().map(|(type_hash, lock_hash, data)| Cell { type_hash: *type_hash, lock_hash: *lock_hash, data }).collect()
}

/// Project the exact action set exposed by a generated builder and its Locks.
pub(crate) fn builder_contracts(metadata: &CompileMetadata, actions: &[&str]) -> Result<BTreeMap<String, Catalog>> {
    let mut contracts = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for name in actions {
        if !seen.insert(*name) || !metadata.actions.iter().any(|action| action.name == *name) {
            return Err(CompileError::without_span("builder contains duplicate or unknown action names"));
        }
    }
    for (kind, name) in
        actions.iter().map(|name| ("action", *name)).chain(metadata.locks.iter().map(|lock| ("lock", lock.name.as_str())))
    {
        let catalog = transaction::project(&metadata.typed_semantics, kind, name)
            .map_err(|error| CompileError::without_span(format!("builder {kind} '{name}' exact-role contract: {error}")))?;
        if let Some(catalog) = catalog {
            contracts.insert(format!("{kind}:{name}"), catalog);
        }
    }
    Ok(contracts)
}
