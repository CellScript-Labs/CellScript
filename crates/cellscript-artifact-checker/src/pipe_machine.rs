//! Independent PIPE helper and ordinary typed-result boundary. The compiler's
//! ABI registry is not read here. Internal ZK/BIP callers retain their raw
//! status/a1/a2 contract rather than the source tuple convention.
use crate::cell_selection::machine::cursor::Cursor;
use crate::checker::{canonical_abi_type, committed_state_type_width, jump_targets_runtime_error};
use crate::{CheckerError, CheckerRejectionCode, ParsedElf, TypedSemanticEntry, VerifiedLoweringRecord};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("PIPE machine contract: {}", message.into()))
}
fn checked<T>(result: Result<T, CheckerError>) -> Result<T, CheckerError> {
    result.map_err(|error| invalid(error.message))
}
fn slot(entry: &TypedSemanticEntry, id: u32, frame: u32) -> Result<u32, CheckerError> {
    entry
        .locals
        .iter()
        .find(|local| local.id == id)
        .and_then(|local| local.source_id.checked_mul(8))
        .and_then(|offset| u32::try_from(offset).ok())
        .filter(|offset| offset.checked_add(8).is_some_and(|end| end <= frame.saturating_sub(16)))
        .ok_or_else(|| invalid("tuple result local leaves the private frame"))
}
// The finite fixed-value prefix is reconstructed from typed source locals.
// A producer-supplied arbitrary address inside the frame is not isolation.
fn tuple_buffer(entry: &TypedSemanticEntry, destination: u32, frame: u32) -> Result<u32, CheckerError> {
    let destination = entry.locals.iter().find(|local| local.id == destination).ok_or_else(|| invalid("PIPE tuple local missing"))?;
    let mut next = entry
        .locals
        .iter()
        .map(|local| local.source_id.checked_add(1).and_then(|id| id.checked_mul(8)))
        .collect::<Option<Vec<_>>>()
        .and_then(|ends| ends.into_iter().max())
        .ok_or_else(|| invalid("PIPE local inventory overflow"))?;
    let defined: BTreeSet<_> =
        entry.blocks.iter().flat_map(|block| &block.operations).flat_map(|op| &op.destinations).copied().collect();
    let mut locals: Vec<_> =
        entry.locals.iter().filter(|local| defined.contains(&local.id) && local.source_id <= destination.source_id).collect();
    locals.sort_by_key(|local| local.source_id);
    if locals.windows(2).any(|pair| pair[0].source_id == pair[1].source_id) {
        return Err(invalid("PIPE prefix aliases source identities"));
    }
    for local in locals {
        let ty = canonical_abi_type(&local.ty);
        let width = match ty.as_str() {
            "unit" | "bool" | "u8" | "u16" | "u32" | "i32" | "u64" | "usize" => 0,
            "u128" | "hash" | "address" | "ScriptHash" | "SighashAllDigest" => {
                committed_state_type_width(&ty, &[]).ok_or_else(|| invalid("PIPE prefix fixed primitive width is unavailable"))?
            }
            _ if ty.starts_with("[u8;") || ty.starts_with('(') => {
                let width =
                    committed_state_type_width(&ty, &[]).ok_or_else(|| invalid("PIPE prefix fixed-value width is unproved"))?;
                if width <= 8 {
                    return Err(invalid("PIPE prefix small-aggregate storage is outside the checked profile"));
                }
                width
            }
            _ => return Err(invalid("PIPE prefix storage is outside the checked fixed-value profile")),
        };
        if width == 0 {
            continue;
        }
        next = next.checked_add(7).map(|value| value & !7).ok_or_else(|| invalid("PIPE buffer alignment overflow"))?;
        if local.id == destination.id {
            return u32::try_from(next)
                .ok()
                .filter(|offset| width == 16 && offset.checked_add(16).is_some_and(|end| end <= frame.saturating_sub(16)))
                .ok_or_else(|| invalid("PIPE tuple storage leaves the owned frame"));
        }
        next = next
            .checked_add(width.checked_add(7).map(|value| value & !7).ok_or_else(|| invalid("fixed-value extent overflow"))?)
            .ok_or_else(|| invalid("PIPE buffer inventory overflow"))?;
    }
    Err(invalid("PIPE tuple has no independently reconstructed storage extent"))
}

pub(crate) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, sink: Option<&str>) -> Result<(), CheckerError> {
    let typed_pipe =
        record.typed_semantics.entries.iter().flat_map(|entry| &entry.blocks).flat_map(|block| &block.operations).any(|op| {
            op.call.as_ref().is_some_and(|call| {
                matches!(
                    call.target.as_str(),
                    "__ckb_pipe"
                        | "__zk_require_transition"
                        | "__novaseal_bip340_require_signature"
                        | "__novaseal_bip340_require_signature_from_cell_dep"
                )
            })
        });
    let helper = record.entries.iter().find(|entry| entry.id == "runtime:__ckb_pipe");
    if helper.is_none() && !typed_pipe {
        return Ok(());
    }
    let helper = helper.ok_or_else(|| invalid("typed PIPE producer has no runtime body"))?;
    let abort = sink
        .and_then(|id| record.blocks.iter().find(|block| block.id == id))
        .map(|block| block.range.start)
        .ok_or_else(|| invalid("PIPE caller has no verified abort sink"))?;
    let words: BTreeMap<_, _> = elf.instructions.iter().map(|word| (word.address, word.word)).collect();
    let targets: BTreeMap<_, _> = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut blocks: Vec<_> = record.blocks.iter().filter(|block| block.owner_entry == helper.id).collect();
    blocks.sort_by_key(|block| block.range.start);
    let first = blocks.first().ok_or_else(|| invalid("PIPE helper is empty"))?;
    let start = first.range.start;
    let end = blocks.last().unwrap().range.end;
    if first.id != helper.entry_block || blocks.windows(2).any(|pair| pair[0].range.end != pair[1].range.start) {
        return Err(invalid("PIPE helper ownership or entry differs"));
    }
    let mut c = Cursor::new(start, end, &words, &targets);
    if !record.target_profile.starts_with("ckb") {
        checked(c.constant(10, 1))?;
        let target = c.target(abort);
        checked(c.jump(target))?;
    } else {
        checked(c.stack_adjust(-32))?;
        checked(c.stack_store(1, 24))?;
        checked(c.addi(10, 2, 8))?;
        checked(c.constant(17, 2604))?;
        checked(c.syscall())?;
        let failed = c.label();
        let done = c.label();
        checked(c.branch(1, 10, 0, failed))?;
        checked(c.stack_load(11, 8))?;
        checked(c.stack_load(12, 16))?;
        checked(c.constant(10, 0))?;
        checked(c.jump(done))?;
        checked(c.bind(failed))?;
        checked(c.constant(10, 1))?;
        checked(c.bind(done))?;
        checked(c.stack_load(1, 24))?;
        checked(c.stack_adjust(32))?;
        checked(c.exact(0x00008067))?;
    }
    checked(c.finish())?;

    let owners: BTreeMap<_, _> = record.blocks.iter().map(|block| (block.range.start, block)).collect();
    let mut anchors = BTreeMap::<&str, BTreeMap<u64, u32>>::new();
    for block in &record.blocks {
        if let Some(id) = block.lowering_block_id {
            anchors.entry(&block.owner_entry).or_default().insert(block.range.start, id);
        }
    }
    let mut calls = BTreeMap::<(&str, u32), Vec<u64>>::new();
    for edge in &elf.control_flow {
        if start < edge.target && edge.target < end && !(start..end).contains(&edge.address) {
            return Err(invalid("external control flow enters a partial PIPE helper"));
        }
        if edge.target != start {
            continue;
        }
        let word = words.get(&edge.address).ok_or_else(|| invalid("PIPE call instruction is absent"))?;
        if !matches!(word & 0x7f, 0x6f | 0x67) || word >> 7 & 31 != 1 {
            return Err(invalid("PIPE entry is not a linked call"));
        }
        let owner = owners
            .range(..=edge.address)
            .next_back()
            .map(|(_, block)| *block)
            .filter(|block| block.range.contains(edge.address))
            .ok_or_else(|| invalid("PIPE call has no owner"))?;
        let block = anchors
            .get(owner.owner_entry.as_str())
            .and_then(|blocks| blocks.range(..=edge.address).next_back())
            .map(|(_, id)| *id)
            .ok_or_else(|| invalid("PIPE call has no typed block"))?;
        calls.entry((&owner.owner_entry, block)).or_default().push(edge.address);
    }
    for entry in &record.typed_semantics.entries {
        for block in &entry.blocks {
            let expected: Vec<_> = block
                .operations
                .iter()
                .filter_map(|op| {
                    let call = op.call.as_ref()?;
                    let error = match call.target.as_str() {
                        "__ckb_pipe" => None,
                        "__zk_require_transition" => Some(75),
                        "__novaseal_bip340_require_signature" | "__novaseal_bip340_require_signature_from_cell_dep" => Some(50),
                        _ => return None,
                    };
                    Some((op, call, error))
                })
                .collect();
            let mut actual = calls.remove(&(entry.id.as_str(), block.id)).unwrap_or_default();
            actual.sort_unstable();
            if expected.len() != actual.len() {
                return Err(invalid("typed PIPE inventory differs from actual calls"));
            }
            if expected.is_empty() {
                continue;
            }
            let lowering = record
                .entries
                .iter()
                .find(|candidate| candidate.id == entry.id)
                .ok_or_else(|| invalid("PIPE caller entry missing"))?;
            let frame = crate::nested_machine::owned_frame(record, elf, lowering)?;
            let owner_end =
                record.blocks.iter().filter(|block| block.owner_entry == entry.id).map(|block| block.range.end).max().unwrap_or(0);
            for ((op, call, internal_error), address) in expected.into_iter().zip(actual) {
                let mut c = Cursor::new(address + 4, owner_end, &words, &targets);
                let success = c.label();
                checked(c.branch(0, 10, 0, success))?;
                if let Some(code) = internal_error {
                    // A callable may share its fixed-code error block, while
                    // another emitter materializes the error inline. Decode
                    // the exact fixed error and bind the actual continuation.
                    let destination = if jump_targets_runtime_error(record, elf, c.pc, code) {
                        targets[&c.pc]
                    } else {
                        checked(c.constant(10, code as u64))?;
                        abort
                    };
                    let target = c.target(destination);
                    checked(c.jump(target))?;
                    checked(c.bind(success))?;
                    c.end = c.pc;
                    checked(c.finish())?;
                    // Existing ZK/BIP paths consume raw a1/a2. They are not a
                    // source tuple and must never receive the ordinary remap.
                    continue;
                }
                let abort_target = c.target(abort);
                checked(c.jump(abort_target))?;
                checked(c.bind(success))?;
                let admitted = c.pc;
                checked(c.addi(10, 11, 0))?;
                checked(c.addi(11, 12, 0))?;
                if op.opcode != "call"
                    || call.contract != "versioned-runtime-helper"
                    || !call.params.is_empty()
                    || !op.operands.is_empty()
                    || canonical_abi_type(&call.return_type) != "(u64,u64)"
                    || op.destinations.len() > 1
                {
                    return Err(invalid("ordinary PIPE typed result is not two u64 values"));
                }
                if let Some(&id) = op.destinations.first() {
                    if !entry.locals.iter().any(|local| local.id == id && canonical_abi_type(&local.ty) == "(u64,u64)") {
                        return Err(invalid("PIPE result local has a different tuple type"));
                    }
                    let root_slot = slot(entry, id, frame)?;
                    let buffer = tuple_buffer(entry, id, frame)?;
                    checked(c.addi(5, 10, 0))?;
                    for endpoint in 0..2 {
                        if endpoint == 1 {
                            checked(c.addi(5, 11, 0))?;
                        }
                        for byte in 0..8 {
                            checked(c.stack_byte(5, buffer + endpoint * 8 + byte, true))?;
                            if byte < 7 {
                                checked(c.immediate(0x13, 5, 5, 5, 8))?;
                            }
                        }
                    }
                    checked(c.stack_address(5, buffer))?;
                    checked(c.stack_store(5, root_slot))?;
                }
                let protected_end = c.pc;
                c.end = c.pc;
                checked(c.finish())?;
                // An external edge cannot enter halfway through the status /
                // remap / receive sequence. The success branch is its sole edge.
                if elf.control_flow.iter().any(|edge| {
                    address + 4 <= edge.target
                        && edge.target < protected_end
                        && !(edge.address == address + 4 && edge.target == admitted)
                }) {
                    return Err(invalid("control flow bypasses part of PIPE result admission"));
                }
            }
        }
    }
    if !calls.is_empty() {
        return Err(invalid("PIPE call has no typed producer"));
    }
    Ok(())
}
