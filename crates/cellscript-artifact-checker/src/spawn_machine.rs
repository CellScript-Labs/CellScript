//! Finite single-inherited-fd SPAWN body and every incoming status guard.
//! This establishes the private syscall buffers and status/child-id separation;
//! child program identity, behavior and subsequent IPC lifecycle are separate.
use crate::cell_selection::machine::cursor::Cursor;
use crate::checker::jump_targets_runtime_error;
use crate::{CheckerError, CheckerRejectionCode, ParsedElf, VerifiedLoweringRecord};
use std::collections::BTreeMap;

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("SPAWN machine contract: {}", message.into()))
}
fn checked<T>(result: Result<T, CheckerError>) -> Result<T, CheckerError> {
    result.map_err(|error| invalid(error.message))
}
fn spawn_failure_code(target: &str) -> Option<i32> {
    match target {
        "__novaseal_bip340_require_signature" | "__novaseal_bip340_require_signature_from_cell_dep" => Some(51),
        "__zk_require_transition" => Some(76),
        _ => None,
    }
}

pub(crate) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, sink: Option<&str>) -> Result<(), CheckerError> {
    let typed_spawn =
        record.typed_semantics.entries.iter().flat_map(|entry| &entry.blocks).flat_map(|block| &block.operations).any(|op| {
            op.call
                .as_ref()
                .is_some_and(|call| call.contract == "versioned-runtime-helper" && spawn_failure_code(&call.target).is_some())
        });
    let helper = record.entries.iter().find(|entry| entry.id == "runtime:__ckb_spawn_with_fd1");
    if helper.is_none() && !typed_spawn {
        return Ok(());
    }
    let helper = helper.ok_or_else(|| invalid("typed SPAWN producer has no runtime body"))?;
    let abort = sink
        .and_then(|id| record.blocks.iter().find(|block| block.id == id))
        .map(|block| block.range.start)
        .ok_or_else(|| invalid("SPAWN caller has no verified abort sink"))?;
    let words: BTreeMap<_, _> = elf.instructions.iter().map(|word| (word.address, word.word)).collect();
    let targets: BTreeMap<_, _> = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut blocks: Vec<_> = record.blocks.iter().filter(|block| block.owner_entry == helper.id).collect();
    blocks.sort_by_key(|block| block.range.start);
    let first = blocks.first().ok_or_else(|| invalid("SPAWN helper is empty"))?;
    let start = first.range.start;
    let end = blocks.last().unwrap().range.end;
    if first.id != helper.entry_block || blocks.windows(2).any(|pair| pair[0].range.end != pair[1].range.start) {
        return Err(invalid("SPAWN helper ownership or entry differs"));
    }
    let mut c = Cursor::new(start, end, &words, &targets);
    if !record.target_profile.starts_with("ckb") {
        checked(c.constant(10, 1))?;
        let target = c.target(abort);
        checked(c.jump(target))?;
    } else {
        // RFC0050 spawn_args: argc/argv/process_id/inherited_fds. The child
        // id, two-element fd list (including zero terminator), arguments and
        // saved return address occupy disjoint, full-width private slots.
        checked(c.stack_adjust(-80))?;
        checked(c.stack_store(1, 72))?;
        checked(c.stack_store(11, 8))?;
        checked(c.stack_store(10, 64))?;
        for offset in [16, 32, 40] {
            checked(c.stack_store(0, offset))?;
        }
        checked(c.addi(5, 2, 24))?;
        checked(c.stack_store(5, 48))?;
        checked(c.addi(5, 2, 8))?;
        checked(c.stack_store(5, 56))?;
        checked(c.stack_load(10, 64))?;
        checked(c.constant(11, 3))?; // CellDep
        checked(c.constant(12, 0))?; // complete code
        checked(c.constant(13, 0))?; // Cell data
        checked(c.addi(14, 2, 32))?;
        checked(c.constant(17, 2601))?;
        checked(c.syscall())?;
        let failed = c.label();
        let done = c.label();
        checked(c.branch(1, 10, 0, failed))?;
        checked(c.stack_load(11, 24))?;
        checked(c.constant(10, 0))?;
        checked(c.jump(done))?;
        checked(c.bind(failed))?;
        checked(c.constant(10, 1))?;
        checked(c.bind(done))?;
        checked(c.stack_load(1, 72))?;
        checked(c.stack_adjust(80))?;
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
            return Err(invalid("external control flow enters a partial SPAWN helper"));
        }
        if edge.target != start {
            continue;
        }
        let word = words.get(&edge.address).ok_or_else(|| invalid("SPAWN call instruction is absent"))?;
        if !matches!(word & 0x7f, 0x6f | 0x67) || word >> 7 & 31 != 1 {
            return Err(invalid("SPAWN entry is not a linked call"));
        }
        let owner = owners
            .range(..=edge.address)
            .next_back()
            .map(|(_, block)| *block)
            .filter(|block| block.range.contains(edge.address))
            .ok_or_else(|| invalid("SPAWN call has no owner"))?;
        let block = anchors
            .get(owner.owner_entry.as_str())
            .and_then(|blocks| blocks.range(..=edge.address).next_back())
            .map(|(_, id)| *id)
            .ok_or_else(|| invalid("SPAWN call has no typed block"))?;
        calls.entry((&owner.owner_entry, block)).or_default().push(edge.address);
    }
    for entry in &record.typed_semantics.entries {
        let owner_end = record.blocks.iter().filter(|block| block.owner_entry == entry.id).map(|block| block.range.end).max();
        for block in &entry.blocks {
            let expected: Vec<_> = block
                .operations
                .iter()
                .filter_map(|op| {
                    let call = op.call.as_ref()?;
                    if call.contract != "versioned-runtime-helper" {
                        return None;
                    }
                    spawn_failure_code(&call.target)
                })
                .collect();
            let mut actual = calls.remove(&(entry.id.as_str(), block.id)).unwrap_or_default();
            actual.sort_unstable();
            if expected.len() != actual.len() {
                return Err(invalid("typed SPAWN inventory differs from actual calls"));
            }
            for (code, address) in expected.into_iter().zip(actual) {
                let mut c = Cursor::new(address + 4, owner_end.ok_or_else(|| invalid("SPAWN caller body missing"))?, &words, &targets);
                let success = c.label();
                checked(c.branch(0, 10, 0, success))?;
                let destination = if jump_targets_runtime_error(record, elf, c.pc, code) {
                    targets[&c.pc]
                } else {
                    checked(c.constant(10, code as u64))?;
                    abort
                };
                let destination = c.target(destination);
                checked(c.jump(destination))?;
                checked(c.bind(success))?;
                let admitted = c.pc;
                c.end = admitted;
                checked(c.finish())?;
                // Include the first success instruction: a foreign edge to
                // that exact boundary must not bypass the status test.
                if elf.control_flow.iter().any(|edge| {
                    address + 4 <= edge.target && edge.target <= admitted && !(edge.address == address + 4 && edge.target == admitted)
                }) {
                    return Err(invalid("control flow bypasses SPAWN result admission"));
                }
            }
        }
    }
    if !calls.is_empty() {
        return Err(invalid("SPAWN call has no typed producer"));
    }
    Ok(())
}
