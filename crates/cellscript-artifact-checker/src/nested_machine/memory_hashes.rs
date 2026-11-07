//! Complete runtime closures used by bounded caller memory-effect summaries.
use super::*;
use crate::runtime_contract::RuntimeHelper;

pub(super) const SCRIPT: &str = "runtime:__ckb_script_hash";
pub(super) const MEMORY: &str = "runtime:__ckb_hash_blake2b_var";

fn memory_operation(operation: &crate::TypedSemanticOperation) -> bool {
    matches!(
        helper(operation),
        Some(
            RuntimeHelper::CkbHashDataPacked
                | RuntimeHelper::CkbHashBlake2bPacked
                | RuntimeHelper::CkbHashBlake2bVar
                | RuntimeHelper::CkbCommitmentOpen
        )
    )
}

fn script_operation(operation: &crate::TypedSemanticOperation) -> bool {
    helper(operation) == Some(RuntimeHelper::CkbScriptHash)
}

fn helper(operation: &crate::TypedSemanticOperation) -> Option<RuntimeHelper> {
    operation
        .call
        .as_ref()
        .filter(|call| call.contract == "versioned-runtime-helper")
        .and_then(|call| RuntimeHelper::from_symbol(&call.target))
}

pub(super) fn callers(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> BTreeSet<String> {
    let owners = record.blocks.iter().map(|block| (block.range.start, block)).collect::<BTreeMap<_, _>>();
    let typed = record.typed_semantics.entries.iter().map(|entry| entry.id.as_str()).collect::<BTreeSet<_>>();
    let targets = record
        .entries
        .iter()
        .filter(|entry| matches!(entry.id.as_str(), SCRIPT | MEMORY))
        .filter_map(|entry| record.blocks.iter().find(|block| block.id == entry.entry_block).map(|block| block.range.start))
        .collect::<BTreeSet<_>>();
    let mut callers = record
        .typed_semantics
        .entries
        .iter()
        .filter(|entry| {
            entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .any(|operation| script_operation(operation) || memory_operation(operation))
        })
        .map(|entry| entry.id.clone())
        .collect::<BTreeSet<_>>();
    for edge in elf.control_flow.iter().filter(|edge| targets.contains(&edge.target)) {
        if let Some((_, block)) = owners.range(..=edge.address).next_back().filter(|(_, block)| block.range.contains(edge.address))
            && typed.contains(block.owner_entry.as_str())
        {
            callers.insert(block.owner_entry.clone());
        }
    }
    callers
}

/// Bind actual calls one-to-one to their checked typed blocks. Script args and
/// raw packed data have an independently derived fixed width. Other packed
/// preimages retain their separate materialization contracts; None still
/// requires a concrete caller-owned read extent at every execution state.
pub(super) fn calls(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    script: bool,
) -> Result<BTreeMap<u64, Option<i64>>, CheckerError> {
    let expected_operation = if script { script_operation } else { memory_operation };
    let expected_count = record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .filter(|operation| expected_operation(operation))
        .count();
    let Some(entry) = record.entries.iter().find(|entry| entry.id == if script { SCRIPT } else { MEMORY }) else {
        return if expected_count == 0 { Ok(BTreeMap::new()) } else { Err(invalid("typed hash has no runtime helper")) };
    };
    let target = record
        .blocks
        .iter()
        .find(|block| block.id == entry.entry_block)
        .ok_or_else(|| invalid("hash helper has no entry"))?
        .range
        .start;
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    let mut current = BTreeMap::new();
    let mut actual = BTreeMap::<(&str, u32), Vec<u64>>::new();
    let typed = record.typed_semantics.entries.iter().map(|entry| entry.id.as_str()).collect::<BTreeSet<_>>();
    let addresses = elf.control_flow.iter().filter(|edge| edge.target == target).map(|edge| edge.address).collect::<BTreeSet<_>>();
    for block in blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        for &address in addresses.range(block.range.start..block.range.end) {
            if typed.contains(block.owner_entry.as_str()) {
                let id = *current.get(block.owner_entry.as_str()).ok_or_else(|| invalid("hash call has no typed body block"))?;
                actual.entry((&block.owner_entry, id)).or_default().push(address);
            }
        }
    }
    let mut calls = BTreeMap::new();
    for entry in &record.typed_semantics.entries {
        for block in &entry.blocks {
            let expected = block.operations.iter().filter(|operation| expected_operation(operation)).collect::<Vec<_>>();
            let addresses = actual.remove(&(entry.id.as_str(), block.id)).unwrap_or_default();
            if expected.len() != addresses.len() {
                return Err(invalid("hash machine calls differ from their typed block inventory"));
            }
            for (operation, address) in expected.into_iter().zip(addresses) {
                let data = helper(operation) == Some(RuntimeHelper::CkbHashDataPacked);
                let width = if script || data {
                    let operand = operation
                        .operands
                        .get(if script { 2 } else { 0 })
                        .ok_or_else(|| invalid("hash call has no fixed input operand"))?;
                    let width = crate::checker::committed_state_type_width(&operand.ty, &record.typed_semantics.types)
                        .filter(|width| !script || *width <= 459)
                        .ok_or_else(|| invalid("hash input lacks a bounded fixed width"))?;
                    Some(i64::try_from(width).map_err(|_| invalid("hash input width overflows"))?)
                } else {
                    None
                };
                calls.insert(address, width);
            }
        }
    }
    if !actual.is_empty() {
        return Err(invalid("unbound hash machine call"));
    }
    Ok(calls)
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str) -> Result<(), CheckerError> {
    for dependency in [MEMORY, "runtime:__cellscript_blake2b_compress"] {
        streaming_hashes::extent(record, elf, dependency)?;
    }
    if owner == SCRIPT {
        crate::checker::validate_script_hash_helper(record, elf)
    } else if owner == MEMORY {
        crate::instance_machine::validate_memory_hash(record, elf)
    } else {
        Err(invalid("unknown bounded memory hash"))
    }
}
