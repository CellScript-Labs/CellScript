//! Canonical builder projection from checked typed entries. A conditional site
//! must not silently become an unconditional transaction requirement.
use super::*;
use crate::{TypedSemanticEntry, TypedSemanticOperationDetail, TypedSemanticRecord};
use std::collections::{BTreeSet, VecDeque};

const SCHEMA: &str = "cellscript-exact-role-transaction-contracts-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub entry: String,
    pub block: u32,
    pub operation: u32,
    pub contract: Contract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema: String,
    pub types: Vec<TypedSemanticType>,
    pub sites: Vec<Site>,
}

impl Catalog {
    pub fn verify(&self, current: CurrentScript, inputs: &[Cell<'_>], outputs: &[Cell<'_>]) -> Result<Vec<Selection>, Error> {
        if self.schema != SCHEMA || self.sites.is_empty() || self.sites.len() > 64 {
            return Err(Error::Contract);
        }
        let mut keys = BTreeSet::new();
        self.sites
            .iter()
            .map(|site| {
                if site.entry.is_empty() || !keys.insert((&site.entry, site.block, site.operation)) {
                    return Err(Error::Contract);
                }
                select(&site.contract, &self.types, current, inputs, outputs)
            })
            .collect()
    }
}

/// Project only the chosen entry and its called helpers. A site must dominate
/// every normal return in an unconditionally called entry. Other paths need
/// future execution-path evidence, rather than being silently overconstrained.
pub fn project(record: &TypedSemanticRecord, kind: &str, name: &str) -> Result<Option<Catalog>, Error> {
    // Existing modules do not opt into the additional selection projection limits.
    if !record
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .any(|operation| matches!(operation.detail, TypedSemanticOperationDetail::ExactRoleSelection { .. }))
    {
        return Ok(None);
    }
    if record.entries.iter().flat_map(|entry| &entry.blocks).take(16385).count() > 16384
        || record.entries.iter().flat_map(|entry| &entry.blocks).flat_map(|block| &block.operations).take(262145).count() > 262144
    {
        return Err(Error::Contract);
    }
    if record.entries.len() > 512
        || record.types.len() > 512
        || record
            .entries
            .iter()
            .any(|entry| entry.blocks.len() > 4096 || entry.blocks.iter().any(|block| block.operations.len() > 4096))
    {
        return Err(Error::Contract);
    }
    let entries = record.entries.iter().map(|entry| (entry.name.as_str(), entry)).collect::<BTreeMap<_, _>>();
    if entries.len() != record.entries.len() {
        return Err(Error::Contract);
    }
    let Some(root) = entries.get(name).filter(|entry| entry.kind == kind) else {
        return Err(Error::Contract);
    };
    let reachable =
        entries.iter().map(|(name, entry)| Ok((*name, reachable_blocks(entry)?))).collect::<Result<BTreeMap<_, _>, Error>>()?;
    let mut reached = BTreeMap::new();
    let mut pending = VecDeque::from([(root.name.as_str(), true)]);
    let mut mandatory = BTreeMap::new();
    while let Some((name, always)) = pending.pop_front() {
        if reached.get(name).is_some_and(|previous| *previous || !always) {
            continue;
        }
        reached.insert(name, always);
        let entry = entries[name];
        for block in entry.blocks.iter().filter(|block| reachable[name].contains(&block.id)) {
            for operation in &block.operations {
                let Some(target) = operation.call.as_ref().and_then(|call| entries.get(call.target.as_str())) else { continue };
                if target.kind != "helper" {
                    return Err(Error::Contract);
                }
                let required = *mandatory.entry((entry.name.as_str(), block.id)).or_insert_with(|| dominates_returns(entry, block.id));
                pending.push_back((target.name.as_str(), always && required));
            }
        }
    }
    let mut sites = Vec::new();
    for (name, always) in reached {
        let entry = entries[name];
        for block in entry.blocks.iter().filter(|block| reachable[name].contains(&block.id)) {
            for operation in &block.operations {
                let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &operation.detail else { continue };
                if !always || !*mandatory.entry((name, block.id)).or_insert_with(|| dominates_returns(entry, block.id)) {
                    return Err(Error::Conditional);
                }
                contract.validate().map_err(|_| Error::Contract)?;
                sites.push(Site { entry: entry.id.clone(), block: block.id, operation: operation.index, contract: contract.clone() });
                if sites.len() > 64 {
                    return Err(Error::Contract);
                }
            }
        }
    }
    if sites.is_empty() {
        return Ok(None);
    }
    sites.sort_by(|left, right| (&left.entry, left.block, left.operation).cmp(&(&right.entry, right.block, right.operation)));
    let types = record.types.clone();
    let schemas = types.iter().map(|ty| (ty.name.as_str(), ty)).collect();
    for site in &sites {
        layout::derive(&site.contract, &schemas).map_err(|_| Error::Contract)?;
    }
    Ok(Some(Catalog { schema: SCHEMA.into(), types, sites }))
}

fn dominates_returns(entry: &TypedSemanticEntry, excluded: u32) -> bool {
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    if blocks.len() != entry.blocks.len() || !blocks.contains_key(&excluded) {
        return false;
    }
    let mut pending = vec![entry.entry_block];
    let mut visited = BTreeSet::new();
    let mut reaches_site = false;
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        if id == excluded {
            reaches_site = true;
            continue;
        }
        let Some(block) = blocks.get(&id) else { return false };
        if block.runtime_error.is_some() || block.terminator == "verifier-failure" {
            continue;
        }
        if block.terminator == "return" {
            return false;
        }
        pending.extend(&block.successors);
    }
    reaches_site
}

fn reachable_blocks(entry: &TypedSemanticEntry) -> Result<BTreeSet<u32>, Error> {
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    if blocks.len() != entry.blocks.len() {
        return Err(Error::Contract);
    }
    let mut pending = vec![entry.entry_block];
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let block = blocks.get(&id).ok_or(Error::Contract)?;
        if block.runtime_error.is_none() && block.terminator != "verifier-failure" {
            pending.extend(&block.successors);
        }
    }
    Ok(visited)
}
