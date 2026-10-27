//! Bind non-fused bounded-body branches and jumps to their typed successors.
//! Empty fallthroughs retain inventory evidence without inventing instructions.
use super::*;
use std::collections::BTreeSet;

const PREFIX: &str = ".Lallocated_branch_";

fn target(
    entry: &TypedSemanticEntry,
    mut id: u32,
    starts: &BTreeMap<(&str, u32), u64>,
    work: &mut usize,
) -> Result<u64, CheckerError> {
    let mut visited = BTreeSet::new();
    loop {
        *work += 1;
        if *work > 4_000_000 || !visited.insert(id) {
            return Err(invalid("bounded branch target resolution cycles or exceeds its budget"));
        }
        if let Some(start) = starts.get(&(entry.id.as_str(), id)) {
            return Ok(*start);
        }
        let block = entry.blocks.iter().find(|block| block.id == id).ok_or_else(|| invalid("bounded branch target is absent"))?;
        if !block.operations.is_empty() || block.terminator != "jump" || block.runtime_error.is_some() || block.successors.len() != 1 {
            return Err(invalid("bounded branch target has no machine entry"));
        }
        id = block.successors[0];
    }
}

pub(super) fn validate<'a>(
    record: &'a VerifiedLoweringRecord,
    machine: &Machine<'_>,
    fused_blocks: &BTreeSet<(&str, u32)>,
    required: &mut flow::Markers<'a>,
) -> Result<flow::Fallthroughs<'a>, CheckerError> {
    let mut expected = BTreeMap::new();
    for entry in record.typed_semantics.entries.iter().filter(|entry| covered_entry(entry)) {
        for block in &entry.blocks {
            if matches!(block.terminator.as_str(), "branch" | "jump")
                && block.runtime_error.is_none()
                && !fused_blocks.contains(&(entry.id.as_str(), block.id))
            {
                expected.insert((entry.id.as_str(), block.id), (entry, block));
            }
        }
    }
    let mut starts = BTreeMap::new();
    let mut boundaries = BTreeMap::new();
    let mut attribution = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        let owner = block.owner_entry.as_str();
        if let Some(id) = block.lowering_block_id {
            current.insert(owner, id);
            starts.entry((owner, id)).or_insert(block.range.start);
        }
        attribution.insert(block.range.start, (owner, current.get(owner).copied()));
        for label in &block.allocated_lifecycle_labels {
            let Some(suffix) = label.strip_prefix(PREFIX) else { continue };
            let parts = suffix.split('_').collect::<Vec<_>>();
            if parts.len() != 3 || !matches!(parts[2], "begin" | "end") {
                return Err(invalid("invalid bounded branch marker"));
            }
            let id = parts[0].parse::<u32>().map_err(|_| invalid("invalid bounded branch block"))?;
            let nonce = parts[1].parse::<u32>().map_err(|_| invalid("invalid bounded branch nonce"))?;
            if boundaries.insert((owner, id, nonce, parts[2]), block.range.start).is_some() {
                return Err(invalid("duplicate bounded branch marker"));
            }
        }
    }
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for (&source, &destination) in machine.targets {
        incoming.entry(destination).or_default().push(source);
    }
    let mut work = 0;
    let mut consumed = 0;
    let mut fallthroughs = flow::Fallthroughs::new();
    for (&(owner, id, nonce, edge), &start) in &boundaries {
        if edge != "begin" {
            continue;
        }
        let &end = boundaries.get(&(owner, id, nonce, "end")).ok_or_else(|| invalid("bounded branch end is absent"))?;
        let (entry, block) = expected.remove(&(owner, id)).ok_or_else(|| invalid("extra bounded source branch"))?;
        if start > end || end - start > 64 {
            return Err(invalid("reversed or excessive bounded branch span"));
        }
        for pc in (start..end).step_by(4) {
            if attribution.range(..=pc).next_back().map(|(_, value)| *value) != Some((owner, Some(id))) {
                return Err(invalid("bounded branch leaves its typed block"));
            }
        }
        if start < end {
            for (_, sources) in incoming.range((std::ops::Bound::Excluded(start), std::ops::Bound::Excluded(end))) {
                work += sources.len();
                if work > 4_000_000 || sources.iter().any(|pc| !(start..end).contains(pc)) {
                    return Err(invalid("bounded branch preparation can be bypassed or exceeds its proof budget"));
                }
            }
        }
        let yes = target(entry, block.successors[0], &starts, &mut work)?;
        let no = if block.terminator == "branch" { target(entry, block.successors[1], &starts, &mut work)? } else { yes };
        let condition = block.operations.last().filter(|op| op.opcode == "branch-condition").and_then(|op| op.operands.first());
        let chosen = if block.terminator == "jump" || block.successors[0] == block.successors[1] {
            Some(yes)
        } else if let Some(value) = condition.and_then(|condition| condition.constant.as_ref()) {
            Some(if literal(value)? != 0 { yes } else { no })
        } else {
            None
        };
        let mut c = Cursor::new(start, end, machine.words, machine.targets);
        let control;
        if let Some(chosen) = chosen {
            control = start;
            if start == end {
                if end != chosen {
                    return Err(invalid("empty bounded branch does not fall through to its typed successor"));
                }
                if starts.contains_key(&(owner, id)) {
                    fallthroughs.insert((owner, id), chosen);
                } else if block.operations.iter().any(|op| op.opcode != "branch-condition") {
                    return Err(invalid("unmapped fallthrough contains source operations"));
                }
            } else if c.jump_target()? != chosen {
                return Err(invalid("bounded jump has the wrong source successor"));
            }
        } else {
            let operand = condition.ok_or_else(|| invalid("bounded branch has no typed condition"))?;
            if !scalar(&operand.ty) {
                return Err(invalid("bounded branch condition is not scalar"));
            }
            let frame = record
                .entries
                .iter()
                .find(|candidate| candidate.id == owner)
                .ok_or_else(|| invalid("branch owner is absent"))?
                .frame_size_bytes;
            let locals = entry.locals.iter().map(|local| (local.id, local)).collect();
            super::operand(&mut c, operand, &locals, frame, 5)?;
            control = c.pc;
            match end - c.pc {
                4 if end == yes => {
                    let label = c.target(no);
                    c.branch(0, 5, 0, label)?;
                }
                4 if end == no => {
                    let label = c.target(yes);
                    c.branch(1, 5, 0, label)?;
                }
                8 => {
                    let label = c.target(no);
                    c.branch(0, 5, 0, label)?;
                    let label = c.target(yes);
                    c.jump(label)?;
                }
                _ => return Err(invalid("bounded branch has an unbound fallthrough or extent")),
            }
        }
        c.finish()?;
        // Empty source jumps are label aliases, not executable events. Other
        // terminators must be reached, with their decoded edges exposed to flow.
        if start != end {
            let position = flow::operation_position(block.operations.len() as u32, 0)?;
            if required.insert((owner, id, position), (start, control, 0)).is_some() {
                return Err(invalid("bounded branch overlaps another required operation"));
            }
        }
        consumed += 2;
    }
    if !expected.is_empty() || consumed != boundaries.len() {
        return Err(invalid("missing bounded source branch or orphan marker"));
    }
    Ok(fallthroughs)
}
