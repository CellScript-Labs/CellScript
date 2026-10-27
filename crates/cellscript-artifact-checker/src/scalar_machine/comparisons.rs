//! Scalar comparison values and branch-only lowering share typed operands.
//! A fused branch may omit the boolean slot only when no other use needs it.
use super::*;
use std::collections::BTreeSet;

pub(super) fn is_comparison(operator: &str) -> bool {
    matches!(operator, "eq" | "ne" | "lt" | "le" | "gt" | "ge")
}

pub(super) fn materialized(c: &mut Cursor<'_>, operator: &str, signed: bool) -> Result<(), CheckerError> {
    if matches!(operator, "eq" | "ne") {
        c.sub(5, 5, 6)?;
        if operator == "eq" {
            c.immediate(0x13, 3, 5, 5, 1)?;
        } else {
            // snez t0, t0 = sltu t0, zero, t0.
            c.exact((5 << 20) | (3 << 12) | (5 << 7) | 0x33)?;
        }
    } else {
        let (left, right) = if matches!(operator, "gt" | "le") { (6, 5) } else { (5, 6) };
        c.exact((right << 20) | (left << 15) | ((if signed { 2 } else { 3 }) << 12) | (5 << 7) | 0x33)?;
        if matches!(operator, "le" | "ge") {
            c.immediate(0x13, 4, 5, 5, 1)?;
        }
    }
    Ok(())
}

pub(super) type FusedSites<'a> = BTreeMap<(&'a str, u32, u32), [u64; 2]>;

pub(super) fn fused_sites(record: &VerifiedLoweringRecord) -> Result<FusedSites<'_>, CheckerError> {
    let mut starts = BTreeMap::new();
    for block in &record.blocks {
        if let Some(id) = block.lowering_block_id {
            starts
                .entry((block.owner_entry.as_str(), id))
                .and_modify(|start: &mut u64| *start = (*start).min(block.range.start))
                .or_insert(block.range.start);
        }
    }
    let mut sites = BTreeMap::new();
    let mut work = 0usize;
    for entry in &record.typed_semantics.entries {
        if !covered_entry(entry) {
            continue;
        }
        let mut uses = BTreeMap::<u32, usize>::new();
        let typed_blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
        for operand in entry.blocks.iter().flat_map(|block| &block.operations).flat_map(|op| &op.operands) {
            if let Some(local) = operand.local {
                *uses.entry(local).or_default() += 1;
            }
        }
        for block in &entry.blocks {
            let [.., comparison, condition] = block.operations.as_slice() else { continue };
            let TypedSemanticOperationDetail::BinaryOperator { operator } = &comparison.detail else { continue };
            if comparison.opcode != "binary"
                || !is_comparison(operator)
                || comparison.destinations.len() != 1
                || comparison.operands.iter().any(|operand| !scalar(&operand.ty))
                || block.terminator != "branch"
                || block.runtime_error.is_some()
                || block.successors.len() != 2
                || condition.opcode != "branch-condition"
                || condition.operands.len() != 1
                || condition.operands[0].local != Some(comparison.destinations[0])
                || uses.get(&comparison.destinations[0]) != Some(&1)
            {
                continue;
            }
            let targets = block
                .successors
                .iter()
                .map(|id| {
                    let mut id = *id;
                    let mut visited = BTreeSet::new();
                    loop {
                        work += 1;
                        if work > 4_000_000 || !visited.insert(id) {
                            return Err(invalid("scalar branch target resolution cycles or exceeds its proof budget"));
                        }
                        if let Some(address) = starts.get(&(entry.id.as_str(), id)) {
                            return Ok(*address);
                        }
                        // Empty fallthrough labels share their successor's address;
                        // only the last typed label owns the following instructions.
                        // Never skip a value definition, condition or failure block.
                        let block = typed_blocks.get(&id).ok_or_else(|| invalid("scalar branch target is absent"))?;
                        if !block.operations.is_empty()
                            || block.terminator != "jump"
                            || block.runtime_error.is_some()
                            || block.successors.len() != 1
                        {
                            return Err(invalid("scalar branch target has no typed machine entry"));
                        }
                        id = block.successors[0];
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            sites.insert((entry.id.as_str(), block.id, comparison.index), [targets[0], targets[1]]);
        }
    }
    Ok(sites)
}

pub(super) fn fused(c: &mut Cursor<'_>, operator: &str, signed: bool, successors: [u64; 2]) -> Result<(), CheckerError> {
    let [yes, no] = successors;
    let less = if signed { 4 } else { 6 };
    let (kind, left, right) = match operator {
        "eq" => (0, 5, 6),
        "ne" => (1, 5, 6),
        "lt" => (less, 5, 6),
        "le" => (less ^ 1, 6, 5),
        "gt" => (less, 6, 5),
        "ge" => (less ^ 1, 5, 6),
        _ => return Err(invalid("non-comparison fused branch")),
    };
    match c.end.checked_sub(c.pc) {
        Some(4) if c.end == yes => {
            let target = c.target(no);
            c.branch(kind ^ 1, left, right, target)?;
        }
        Some(4) if c.end == no => {
            let target = c.target(yes);
            c.branch(kind, left, right, target)?;
        }
        Some(8) => {
            let target = c.target(no);
            c.branch(kind ^ 1, left, right, target)?;
            let target = c.target(yes);
            c.jump(target)?;
        }
        _ => return Err(invalid("scalar branch has an unbound fallthrough or extent")),
    }
    c.finish()
}
