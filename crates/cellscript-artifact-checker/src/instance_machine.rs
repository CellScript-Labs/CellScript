//! Closed executable profile for instance-policy v1. Hashes below authorize
//! complete helper bodies, including their rejecting paths and transitive
//! hash/copy/compare dependencies. Only direct-call relocation immediates are
//! normalized, to the exact checked callee identity. Sidecar digests are never
//! used as an authority. This is a frozen machine profile, not a claim of
//! general compiler or source equivalence.
use crate::{ckb_blake2b256, hex_encode, CheckerError, CheckerRejectionCode, EntryKind, ParsedElf, VerifiedLoweringRecord};
use std::collections::BTreeMap;

const MAIN: &str = "__ckb_require_instance_script_participant_from_args";
// v126 frame compaction changes only each body's paired SP immediates:
// instance 1808 -> 1792, open handle 992 -> 976. The compiler's machine-delta
// regression reconstructs the previous complete-body identities by reverting
// exactly those two words; all other instructions and call identities match.
const PROFILE: [(&str, &str); 6] = [
    (MAIN, "1ff44c167045e426e2b40a6f4e7294d8bb7be7e56b5e9921093e4b8589177539"),
    ("__ckb_require_cell_dep_open_script_handle", "809b33e9466bf13a9649ed5e0d8750345d260aa66232f48adb2365da811c6cef"),
    ("__ckb_hash_blake2b_var", "d150e62cd6bd057dbf699f9c4a60b3e7b54cccb4bdfb77b2709a81fcf0dc7319"),
    ("__cellscript_blake2b_compress", "7eb8c0955f6a5fda2496a11ae01faec53911becdb5a66d01c7f885a31f18205a"),
    ("__cellscript_memcpy_fixed", "79baa2495af6eeab947d46da99db1996d7c6e9419e9b95a1ece63cbe97c92713"),
    ("__cellscript_memcmp_fixed", "b4dac7528698a2d45e2ad153d0900fb8afc3edabd71e0e8cf3dd29c568a62705"),
];

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("instance-policy machine profile: {}", message.into()),
    )
}

pub(crate) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let present = record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .any(|op| op.call.as_ref().is_some_and(|call| call.target == MAIN));
    if !present {
        return Ok(());
    }
    validate_profile(record, elf, &PROFILE)
}

fn validate_profile(record: &VerifiedLoweringRecord, elf: &ParsedElf, profile: &[(&str, &str)]) -> Result<(), CheckerError> {
    let mut entries = BTreeMap::new();
    let mut ranges = BTreeMap::new();
    for &(name, _) in profile {
        let identity = format!("runtime:{name}");
        let entry = record
            .entries
            .iter()
            .find(|entry| entry.id == identity && entry.kind == EntryKind::Runtime)
            .ok_or_else(|| invalid(format!("missing helper {name}")))?;
        let mut blocks = record.blocks.iter().filter(|block| block.owner_entry == identity).collect::<Vec<_>>();
        blocks.sort_by_key(|block| block.range.start);
        let first = blocks.first().ok_or_else(|| invalid("empty helper body"))?;
        let last = blocks.last().ok_or_else(|| invalid("empty helper body"))?;
        if first.id != entry.entry_block || blocks.windows(2).any(|pair| pair[0].range.end != pair[1].range.start) {
            return Err(invalid("helper entry or contiguous code ownership changed"));
        }
        if entries.insert(first.range.start, name).is_some() {
            return Err(invalid("runtime helpers alias the same entry address"));
        }
        ranges.insert(name, (first.range.start, last.range.end));
    }
    let mut mismatches = Vec::new();
    for &(name, expected) in profile {
        let (start, end) = ranges[name];
        let begin = elf.instructions.partition_point(|instruction| instruction.address < start);
        let stop = elf.instructions.partition_point(|instruction| instruction.address < end);
        let instructions = &elf.instructions[begin..stop];
        if instructions.is_empty() || instructions.len() > 16384 || end - start != instructions.len() as u64 * 4 {
            return Err(invalid("helper instruction range exceeds its closed profile"));
        }
        let mut preimage = b"cellscript-instance-runtime-shape-v1\0".to_vec();
        preimage.extend_from_slice(name.as_bytes());
        preimage.push(0);
        let mut index = 0;
        while index < instructions.len() {
            let instruction = instructions[index];
            if instruction.word & 0x7f == 0x17 {
                let next = instructions.get(index + 1).ok_or_else(|| invalid("truncated runtime relocation"))?;
                if !crate::checker::is_auipc(instruction.word, 1) || !crate::checker::is_jalr_call(next.word) {
                    return Err(invalid("unrecognized address-dependent runtime instruction"));
                }
                let displacement = i64::from((instruction.word & 0xfffff000) as i32) + i64::from((next.word as i32) >> 20);
                let target =
                    instruction.address.checked_add_signed(displacement).ok_or_else(|| invalid("call address overflow"))? & !1;
                let callee = entries.get(&target).ok_or_else(|| invalid("runtime call escapes the verified dependency closure"))?;
                preimage.push(1);
                preimage.extend_from_slice(&(callee.len() as u16).to_le_bytes());
                preimage.extend_from_slice(callee.as_bytes());
                index += 2;
            } else {
                preimage.push(0);
                preimage.extend_from_slice(&instruction.word.to_le_bytes());
                index += 1;
            }
        }
        let actual = hex_encode(&ckb_blake2b256(&preimage));
        if actual != expected {
            mismatches.push(format!("{name}={actual}"));
        }
    }
    if !mismatches.is_empty() {
        return Err(invalid(format!("unrecognized executable bodies: {}", mismatches.join(", "))));
    }
    Ok(())
}
