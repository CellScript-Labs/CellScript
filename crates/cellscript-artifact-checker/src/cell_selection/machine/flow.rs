//! Successful exits from a typed block must pass its verifications in order.
//! Checked selection bodies are atomic here; their internal edges and failures
//! have already been recognized independently by the instruction grammar.
use super::*;
use std::collections::VecDeque;

pub(crate) type Markers<'a> = BTreeMap<(&'a str, u32, u32), (u64, u64, u32)>;

pub(crate) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    markers: &Markers<'_>,
    abort: u64,
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
        let complete = |count: usize| {
            if count == sites.len() {
                Ok(())
            } else {
                Err(invalid(format!(
                    "successful path bypasses a typed verification: entry={entry_id} block={block_id} visited={count} required={}",
                    sites.len()
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
            if let Some(index) = sites.iter().position(|site| site.0 == pc) {
                if index != count {
                    return Err(invalid("typed verification order differs"));
                }
                count += 1;
                next = vec![sites[index].1];
            } else {
                let word = *words.get(&pc).ok_or_else(|| invalid("verification CFG leaves decoded instructions"))?;
                match word & 0x7f {
                    0x63 => next.push(*targets.get(&pc).ok_or_else(|| invalid("unresolved conditional edge"))?),
                    0x6f if word >> 7 & 31 == 0 => next = vec![*targets.get(&pc).ok_or_else(|| invalid("unresolved direct edge"))?],
                    0x6f if word >> 7 & 31 == 1 => {} // A returning checked call resumes at pc + 4.
                    0x67 if word == 0x00008067 => {
                        complete(count)?;
                        next.clear();
                    }
                    0x67 if word >> 7 & 31 == 1 && targets.contains_key(&pc) => {}
                    0x67 if word >> 7 & 31 == 0 && targets.contains_key(&pc) => next = vec![targets[&pc]],
                    0x67 => return Err(invalid("unproved indirect verification CFG edge")),
                    0x73 => {
                        let number = record.syscall_sites.iter().find(|site| site.address == pc).and_then(|site| site.syscall_number);
                        if !matches!(
                            number,
                            Some(
                                2041 | 2042 | 2051 | 2052 | 2061 | 2062 | 2071 | 2072 | 2073 | 2074 | 2081 | 2082 | 2083 | 2092 | 2177
                            )
                        ) {
                            // An exit/exec/unknown syscall cannot silently bypass
                            // an outstanding verification or return to this CFG.
                            complete(count)?;
                            next.clear();
                        }
                    }
                    _ => {}
                }
            }
            for target in next {
                if failures.contains(&target) {
                    continue;
                }
                if owner(target) != Some((entry_id, Some(block_id))) || target == start {
                    complete(count)?;
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
fn checked_return_tail(start: u64, end: u64, frame: u32, words: &BTreeMap<u64, u32>, targets: &BTreeMap<u64, u64>) -> bool {
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
