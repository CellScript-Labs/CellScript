//! Conservative, single-VM stack bounds from the independently decoded ELF.
//! This is a static CFG calculation, not an observed stack high-water mark.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cellscript::{strip_vm_abi_trailer, CompileResult};
use cellscript_artifact_checker::{parse_elf, CheckerBudgets};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum StaticStackBound {
    Bounded { static_call_chain_stack_bound_bytes: u64 },
    Unknown { reason: String },
}

pub fn measure(compiled: &CompileResult) -> StaticStackBound {
    match bound(compiled) {
        Ok(bytes) => StaticStackBound::Bounded { static_call_chain_stack_bound_bytes: bytes },
        Err(reason) => StaticStackBound::Unknown { reason },
    }
}

fn bound(compiled: &CompileResult) -> Result<u64, String> {
    compiled.verified_lowering_record.as_ref().ok_or("missing verified lowering record")?;
    let elf = parse_elf(strip_vm_abi_trailer(&compiled.artifact_bytes), CheckerBudgets::default().instructions)
        .map_err(|error| format!("unknown machine stack effect: {error}"))?;
    let mut analysis = StackAnalysis {
        instructions: elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect(),
        adjustments: elf.stack_adjustments.iter().map(|adjustment| (adjustment.address, adjustment.delta)).collect(),
        targets: elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect(),
        active_calls: BTreeSet::new(),
        memo: BTreeMap::new(),
    };
    analysis.call_bound(elf.entry)
}

struct StackAnalysis {
    instructions: BTreeMap<u64, u32>,
    adjustments: BTreeMap<u64, i64>,
    targets: BTreeMap<u64, u64>,
    active_calls: BTreeSet<u64>,
    memo: BTreeMap<u64, u64>,
}

impl StackAnalysis {
    fn call_bound(&mut self, entry: u64) -> Result<u64, String> {
        if let Some(bound) = self.memo.get(&entry) {
            return Ok(*bound);
        }
        if self.active_calls.len() >= 256 {
            return Err("static call-chain analysis depth budget exceeded".into());
        }
        if !self.active_calls.insert(entry) {
            return Err(format!("recursive call graph at {entry:#x}"));
        }
        let mut pending = VecDeque::from([(entry, 0i64, None)]);
        let mut incoming: BTreeMap<u64, (i64, Option<BTreeSet<u64>>)> = BTreeMap::new();
        let mut maximum = 0;
        while let Some((address, mut depth, mut a7)) = pending.pop_front() {
            if let Some((previous, old_a7)) = incoming.get(&address).cloned() {
                if previous != depth {
                    return Err(format!("inconsistent stack displacement at CFG join {address:#x}: {previous} / {depth}"));
                }
                if old_a7 == a7 || old_a7.is_none() {
                    continue;
                }
                a7 = match (old_a7.as_ref(), a7.as_ref()) {
                    (Some(old), Some(new)) => {
                        let values: BTreeSet<_> = old.union(new).copied().collect();
                        (values.len() <= 16).then_some(values)
                    }
                    _ => None,
                };
                if a7 == old_a7 {
                    continue;
                }
            }
            incoming.insert(address, (depth, a7.clone()));
            let word = *self.instructions.get(&address).ok_or_else(|| format!("unknown callee or successor {address:#x}"))?;
            if let Some(delta) = self.adjustments.get(&address) {
                depth = depth.checked_sub(*delta).ok_or("stack displacement overflow")?;
                if depth < 0 {
                    return Err(format!("callee releases caller-owned stack at {address:#x}"));
                }
                maximum = maximum.max(depth as u64);
            }
            if word == 0x0000_8067 {
                if depth != 0 {
                    return Err(format!("unbalanced returning callee at {address:#x}"));
                }
                continue;
            }
            if word == 0x0000_0073 {
                match a7.as_ref() {
                    Some(numbers) if numbers.len() == 1 && numbers.contains(&93) => continue,
                    // EXEC replaces a VM; SPAWN creates an independent VM.
                    // Neither external program's stack can be inferred from
                    // this ELF or added to this call chain.
                    Some(numbers) if numbers.contains(&2043) || numbers.contains(&2601) => {
                        return Err("external EXEC/SPAWN requires separate VM stack accounting".into())
                    }
                    None => return Err(format!("unknown syscall transition at {address:#x}")),
                    _ => {}
                }
                // The pinned CKB syscalls preserve a7. Loads return their
                // status through a0 and write payloads to guest memory;
                // clearing a7 here would lose a loop-invariant syscall ID.
            }
            let opcode = word & 0x7f;
            let rd = (word >> 7) & 0x1f;
            if rd == 17 && matches!(opcode, 0x03 | 0x13 | 0x17 | 0x1b | 0x33 | 0x37 | 0x3b) {
                let rs1 = (word >> 15) & 0x1f;
                let input = match rs1 {
                    0 => Some(BTreeSet::from([0])),
                    17 => a7,
                    _ => None,
                };
                a7 = match (opcode, (word >> 12) & 7) {
                    (0x37, _) => Some(BTreeSet::from([((word & 0xffff_f000) as i32 as i64) as u64])),
                    (0x17, _) => Some(BTreeSet::from([address.wrapping_add(((word & 0xffff_f000) as i32 as i64) as u64)])),
                    (0x13, 0) => {
                        input.map(|values| values.into_iter().map(|value| value.wrapping_add(((word as i32) >> 20) as u64)).collect())
                    }
                    (0x13, 1) if word >> 26 == 0 => {
                        input.map(|values| values.into_iter().map(|value| value << ((word >> 20) & 63)).collect())
                    }
                    _ => None,
                };
            }
            if opcode == 0x63 {
                let target = *self.targets.get(&address).ok_or("unresolved conditional branch")?;
                pending.push_back((target, depth, a7.clone()));
            } else if matches!(opcode, 0x6f | 0x67) {
                let target = *self.targets.get(&address).ok_or("unknown indirect control flow")?;
                if rd == 0 {
                    pending.push_back((target, depth, a7));
                    continue;
                }
                if rd != 1 {
                    return Err("unsupported call return-address convention".into());
                }
                let callee_bound = self.call_bound(target)?;
                maximum = maximum.max((depth as u64).checked_add(callee_bound).ok_or("call-chain bound overflow")?);
                a7 = None;
            }
            pending.push_back((address.checked_add(4).ok_or("instruction address overflow")?, depth, a7));
        }
        self.active_calls.remove(&entry);
        self.memo.insert(entry, maximum);
        Ok(maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> StackAnalysis {
        StackAnalysis {
            // Caller frame (32), temporary outgoing arguments (16), callee
            // frame (48). The resulting bound must include all three.
            instructions: BTreeMap::from([
                (0, 0xfe010113),
                (4, 0xff010113),
                (8, 0x000000ef),
                (12, 0x01010113),
                (16, 0x02010113),
                (20, 0x00008067),
                (64, 0xfd010113),
                (68, 0x03010113),
                (72, 0x00008067),
            ]),
            adjustments: BTreeMap::from([(0, -32), (4, -16), (12, 16), (16, 32), (64, -48), (68, 48)]),
            targets: BTreeMap::from([(8, 64)]),
            active_calls: BTreeSet::new(),
            memo: BTreeMap::new(),
        }
    }

    #[test]
    fn temporary_outgoing_reservations_count_toward_nested_bound() {
        assert_eq!(graph().call_bound(0).unwrap(), 96);
    }

    #[test]
    fn recursion_and_growing_stack_loops_have_no_claimed_bound() {
        let mut recursive = graph();
        recursive.targets.insert(8, 0);
        assert!(recursive.call_bound(0).unwrap_err().contains("recursive"));
        let mut growing = graph();
        growing.instructions.insert(8, 0x0000006f);
        growing.targets.insert(8, 4);
        assert!(growing.call_bound(0).unwrap_err().contains("inconsistent stack"));
    }

    #[test]
    fn unknown_callees_and_unbalanced_returns_have_no_claimed_bound() {
        let mut missing = graph();
        missing.targets.insert(8, 128);
        assert!(missing.call_bound(0).unwrap_err().contains("unknown callee"));
        let mut unbalanced = graph();
        unbalanced.adjustments.remove(&68);
        assert!(unbalanced.call_bound(0).unwrap_err().contains("unbalanced"));
    }
}
