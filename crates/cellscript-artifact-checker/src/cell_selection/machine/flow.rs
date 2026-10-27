//! Successful exits from a typed block must pass its verifications in order.
//! Checked selection bodies are atomic here; their internal edges and failures
//! have already been recognized independently by the instruction grammar.
use super::*;
use std::collections::VecDeque;

pub(crate) type Markers<'a> = BTreeMap<(&'a str, u32, u32), (u64, u64, u32)>;
pub(crate) type Fallthroughs<'a> = BTreeMap<(&'a str, u32), u64>;

/// Source operation order with room for all 32 members retired by one owner
/// drop, or an extent check followed by one borrowed collection operation.
pub(crate) fn operation_position(operation: u32, part: u32) -> Result<u32, CheckerError> {
    if part >= 33 {
        return Err(invalid("too many checked parts in one source operation"));
    }
    operation
        .checked_mul(33)
        .and_then(|position| position.checked_add(part))
        .ok_or_else(|| invalid("checked source operation position overflows"))
}

pub(crate) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    markers: &Markers<'_>,
    abort: u64,
) -> Result<(), CheckerError> {
    validate_with_bodies(record, elf, markers, abort, &BTreeMap::new())
}

/// Additional bodies come only from completed independent instruction/lifetime
/// proofs. They do not satisfy mandatory sites, and must not hide one.
pub(crate) fn validate_with_bodies(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    markers: &Markers<'_>,
    abort: u64,
    bodies: &BTreeMap<u64, u64>,
) -> Result<(), CheckerError> {
    validate_with_fallthroughs(record, elf, markers, abort, bodies, &BTreeMap::new())
}

/// Empty source terminators still constrain every successful exit from their
/// block. Moving evidence labels to a successor cannot erase that obligation.
pub(crate) fn validate_with_fallthroughs(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    markers: &Markers<'_>,
    abort: u64,
    bodies: &BTreeMap<u64, u64>,
    fallthroughs: &Fallthroughs<'_>,
) -> Result<(), CheckerError> {
    let atomic = compose_bodies(markers, bodies)?;
    validate_paths(record, elf, markers, abort, &atomic, fallthroughs)
}

fn compose_bodies(markers: &Markers<'_>, bodies: &BTreeMap<u64, u64>) -> Result<BTreeMap<u64, u64>, CheckerError> {
    let required_ranges = markers.values().map(|&(start, end, _)| (start, end)).collect::<BTreeMap<_, _>>();
    let mut atomic = BTreeMap::new();
    for (&start, &end) in bodies {
        if start >= end {
            return Err(invalid("empty or reversed checked body"));
        }
        if let Some((&required_start, &required_end)) = required_ranges.range(..end).next_back() {
            if required_start <= start && end <= required_end {
                continue;
            }
            if start < required_end {
                return Err(invalid("checked body would hide a required verification"));
            }
        }
        if let Some((&previous, &previous_end)) = atomic.last_key_value()
            && start < previous_end
        {
            if end <= previous_end {
                continue;
            }
            return Err(invalid(format!("checked bodies partially overlap at {previous:#x}/{start:#x}")));
        }
        atomic.insert(start, end);
    }
    Ok(atomic)
}

fn validate_paths(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    markers: &Markers<'_>,
    abort: u64,
    atomic: &BTreeMap<u64, u64>,
    fallthroughs: &Fallthroughs<'_>,
) -> Result<(), CheckerError> {
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect::<BTreeMap<_, _>>();
    let blocks = record.blocks.iter().map(|block| (block.range.start, block)).collect::<BTreeMap<_, _>>();
    let mut typed_at = BTreeMap::new();
    let mut current = BTreeMap::new();
    for (address, block) in &blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        typed_at.insert(*address, (block.owner_entry.as_str(), current.get(block.owner_entry.as_str()).copied()));
    }
    let owner = |address: u64| typed_at.range(..=address).next_back().map(|(_, owner)| *owner);
    let failures = record.verifier_failure_exits.iter().map(|exit| exit.address).chain([abort]).collect::<BTreeSet<_>>();
    // A shared return tail is outside any typed verification prefix even when
    // physical block attribution inherits the last IR block. Recognize every
    // restore instruction; labels alone never authorize an alternate entry.
    let return_tails = record
        .blocks
        .iter()
        .filter_map(|block| {
            let entry = record.entries.iter().find(|entry| entry.id == block.owner_entry)?;
            // The declared budget includes temporary spill/outgoing frames.
            // A return restores only the frame allocated by this prologue.
            let frame = crate::nested_machine::owned_frame(record, elf, entry).ok()?;
            (block.lowering_block_id.is_none() && checked_return_tail(block.range.start, block.range.end, frame, &words, &targets))
                .then_some((entry.id.as_str(), block.range.start))
        })
        .collect::<BTreeSet<_>>();
    let mut required = BTreeMap::<(&str, u32), Vec<(u64, u64)>>::new();
    for (&(owner, block, _operation), &(start, end, _offset)) in markers {
        required.entry((owner, block)).or_default().push((start, end));
    }
    for &key in fallthroughs.keys() {
        required.entry(key).or_default();
    }
    for ((entry_id, block_id), sites) in required {
        let start = record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == entry_id && block.lowering_block_id == Some(block_id))
            .map(|block| block.range.start)
            .min()
            .ok_or_else(|| invalid("missing typed block entry"))?;
        let lowering = record.entries.iter().find(|entry| entry.id == entry_id).ok_or_else(|| invalid("missing callable entry"))?;
        let prologue = record
            .blocks
            .iter()
            .find(|block| block.id == lowering.entry_block)
            .ok_or_else(|| invalid("missing callable prologue"))?
            .range
            .start;
        for edge in &elf.control_flow {
            if owner(edge.target).is_some_and(|(target_owner, _)| target_owner == entry_id)
                && !owner(edge.address).is_some_and(|(source_owner, _)| source_owner == entry_id)
                && edge.target != prologue
            {
                return Err(invalid("external entry bypasses the verification owner's prologue"));
            }
            if owner(edge.target) == Some((entry_id, Some(block_id)))
                && owner(edge.address) != Some((entry_id, Some(block_id)))
                && edge.target != start
                && !failures.contains(&edge.target)
                && !return_tails.contains(&(entry_id, edge.target))
            {
                return Err(invalid(format!(
                    "control flow bypasses the typed block's verification prefix: entry={entry_id} block={block_id} start={start:#x} edge={:#x}->{:#x} source={:?}",
                    edge.address, edge.target, owner(edge.address)
                )));
            }
        }
        let complete = |count: usize, pc: u64, target: Option<u64>| {
            if fallthroughs.get(&(entry_id, block_id)).is_some_and(|expected| Some(*expected) != target) {
                return Err(invalid("empty source terminator exits to a different successor"));
            }
            if count == sites.len() {
                Ok(())
            } else {
                Err(invalid(format!(
                    "successful path bypasses a typed verification: entry={entry_id} block={block_id} pc={pc:#x} visited={count} required={} next={:?}",
                    sites.len(), sites.get(count)
                )))
            }
        };
        let mut states = BTreeMap::from([(start, 0usize)]);
        let mut pending = VecDeque::from([start]);
        let mut steps = 0usize;
        while let Some(pc) = pending.pop_front() {
            steps += 1;
            if steps > 262144 {
                return Err(invalid("verification CFG analysis budget exceeded"));
            }
            let mut count = states[&pc];
            let mut next = vec![pc + 4];
            let mut decode = true;
            if let Some(index) = sites.iter().position(|site| site.0 == pc) {
                if index != count {
                    return Err(invalid("typed verification order differs"));
                }
                count += 1;
                // A checked terminator can have no preparation instructions.
                // Count its mandatory site, then expose its actual successors.
                if sites[index].1 != pc {
                    next = vec![sites[index].1];
                    decode = false;
                }
            } else if let Some(&end) = atomic.get(&pc) {
                if owner(end - 4) != Some((entry_id, Some(block_id))) {
                    return Err(invalid("checked body crosses the current typed block"));
                }
                next = vec![end];
                decode = false;
            }
            if decode {
                let word = *words.get(&pc).ok_or_else(|| invalid("verification CFG leaves decoded instructions"))?;
                match word & 0x7f {
                    0x63 => next.push(*targets.get(&pc).ok_or_else(|| invalid("unresolved conditional edge"))?),
                    0x6f if word >> 7 & 31 == 0 => next = vec![*targets.get(&pc).ok_or_else(|| invalid("unresolved direct edge"))?],
                    0x6f if word >> 7 & 31 == 1 => {} // A returning checked call resumes at pc + 4.
                    0x67 if word == 0x00008067 => {
                        complete(count, pc, None)?;
                        next.clear();
                    }
                    0x67 if word >> 7 & 31 == 1 && targets.contains_key(&pc) => {}
                    0x67 if word >> 7 & 31 == 0 && targets.contains_key(&pc) => next = vec![targets[&pc]],
                    0x67 => return Err(invalid("unproved indirect verification CFG edge")),
                    0x73 => {
                        // Returning syscall bodies must be independently checked
                        // and composed as atomic sites. A sidecar's syscall
                        // number alone does not prove the machine's a7 value.
                        // Unrecognized exit/exec/syscalls cannot bypass work.
                        complete(count, pc, None)?;
                        next.clear();
                    }
                    _ => {}
                }
            }
            for target in next {
                if failures.contains(&target) {
                    continue;
                }
                if owner(target) != Some((entry_id, Some(block_id))) || target == start {
                    complete(count, pc, Some(target))?;
                    continue;
                }
                if sites.iter().any(|&(begin, end)| begin < target && target < end) {
                    return Err(invalid("CFG enters an incomplete verification"));
                }
                match states.get_mut(&target) {
                    Some(previous) if count < *previous => {
                        *previous = count;
                        pending.push_back(target);
                    }
                    Some(_) => {}
                    None => {
                        states.insert(target, count);
                        pending.push_back(target);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Match only a complete frame restore ending in RET. No stores, calls, value
/// changes or conditional edges can hide in an exempted return tail.
pub(crate) fn checked_return_tail(start: u64, end: u64, frame: u32, words: &BTreeMap<u64, u32>, targets: &BTreeMap<u64, u64>) -> bool {
    if frame < 16 || !frame.is_multiple_of(16) {
        return false;
    }
    let mut cursor = super::cursor::Cursor::new(start, end, words, targets);
    let check = (|| {
        cursor.stack_load(1, frame - 8)?;
        cursor.stack_load(8, frame - 16)?;
        cursor.stack_adjust(i64::from(frame))?;
        cursor.exact(0x00008067)?;
        Ok::<_, CheckerError>(())
    })();
    check.is_ok() && cursor.pc == end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_body_composition_cannot_hide_or_partially_cover_required_operations() {
        let required = BTreeMap::from([(("helper:test", 0, 1), (16, 32, 0)), (("helper:test", 0, 2), (48, 64, 0))]);
        for (start, end) in [(0, 20), (20, 36), (0, 36), (28, 52), (0, 80), (64, 64), (80, 64)] {
            assert!(compose_bodies(&required, &BTreeMap::from([(start, end)])).is_err(), "{start}..{end}");
        }
        let bodies = BTreeMap::from([(0, 16), (4, 12), (16, 32), (20, 28), (32, 48), (64, 80)]);
        assert_eq!(compose_bodies(&required, &bodies).unwrap(), BTreeMap::from([(0, 16), (32, 48), (64, 80)]));
        assert!(compose_bodies(&required, &BTreeMap::from([(0, 12), (4, 16)])).is_err());
    }

    #[test]
    fn shared_return_tail_requires_exact_frame_restoration_and_no_other_effects() {
        for frame in [16u32, 32, 2032, 2048] {
            let load = |rd: u32, offset: u32| (offset << 20) | (2 << 15) | (3 << 12) | (rd << 7) | 3;
            let mut code = vec![load(1, frame - 8), load(8, frame - 16)];
            if frame <= 2047 {
                code.push((frame << 20) | (2 << 15) | (2 << 7) | 0x13);
            } else {
                code.extend([0x00001fb7, 0x800f8f9b, 0x01f10133]);
            }
            code.push(0x00008067);
            let accepts = |code: &[u32], expected| {
                let words = code.iter().enumerate().map(|(i, word)| (0x100 + i as u64 * 4, *word)).collect();
                checked_return_tail(0x100, 0x100 + code.len() as u64 * 4, expected, &words, &BTreeMap::new())
            };
            assert!(accepts(&code, frame), "frame {frame}");
            assert!(!accepts(&code, frame + 16));
            for i in 0..code.len() {
                let mut changed = code.clone();
                changed[i] ^= 1 << 7;
                assert!(!accepts(&changed, frame), "frame {frame}, instruction {i}");
            }
            assert!(!accepts(&code[1..], frame));
            let mut value_change = vec![0x00000513];
            value_change.extend(&code);
            assert!(!accepts(&value_change, frame));
        }
    }
}
