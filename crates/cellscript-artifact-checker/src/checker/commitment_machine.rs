//! Typed committed-state machine admission and authenticated materialization.
use super::*;
#[cfg(test)]
mod tests;

pub(super) const COMMITMENT_DOMAIN_PREFIX: &[u8] = b"CellScriptPackedHashV0\0";
pub(super) const COMMITMENT_SCRATCH_BUFFER_BYTES: u64 = 512;
pub(super) const COMMITMENT_RUNTIME_SCRATCH_BYTES: u64 = (8 + COMMITMENT_SCRATCH_BUFFER_BYTES) * 2;
pub(super) const COMMITMENT_RUNTIME_TRAILER_BYTES: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitmentMachineKind {
    Open,
    Commit,
}

#[derive(Debug)]
pub(super) struct CommitmentMachineContract {
    pub(super) owner: String,
    pub(super) kind: CommitmentMachineKind,
    pub(super) inner_type: String,
    pub(super) width: u64,
    pub(super) destination: u32,
    pub(super) source: Option<u32>,
    pub(super) expected: Option<CommitmentMachinePointerSource>,
    pub(super) opening: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CommitmentMachinePointerSource {
    pub(super) stack_slot: i64,
    pub(super) displacement: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitmentMachineValue {
    Unknown,
    Constant(u64),
    StackAddress(i64),
    StackLoadedAddress { stack_slot: i64, displacement: i64 },
}

pub(super) fn validate_committed_state_machine_contract(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let contracts = committed_state_machine_contracts(record)?;
    if contracts.is_empty() {
        return Ok(());
    }

    let owners = contracts.iter().map(|contract| contract.owner.as_str()).collect::<BTreeSet<_>>();
    for owner in owners {
        let owner_contracts = contracts.iter().filter(|contract| contract.owner == owner).collect::<Vec<_>>();
        let mut markers = record
            .blocks
            .iter()
            .filter_map(|block| {
                if block.owner_entry != owner {
                    return None;
                }
                let label = block.machine_label.as_deref()?;
                if generated_label(label, ".Lcommitment_opening_header_ready_") {
                    Some((block, CommitmentMachineKind::Open))
                } else if generated_label(label, ".Lcommitment_commit_header_ready_") {
                    Some((block, CommitmentMachineKind::Commit))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        markers.sort_by_key(|(block, _)| block.range.start);
        if markers.len() != owner_contracts.len()
            || markers.iter().map(|(_, kind)| *kind).ne(owner_contracts.iter().map(|contract| contract.kind))
        {
            return Err(commitment_machine_error(format!(
                "entry '{owner}' does not preserve the typed open/recommit operation order"
            )));
        }

        let mut verified = generated_blocks(record, owner, ".Lcommitment_opening_verified_");
        verified.sort_by_key(|block| block.range.start);
        if verified.len() != owner_contracts.iter().filter(|contract| contract.kind == CommitmentMachineKind::Open).count() {
            return Err(commitment_machine_error(format!(
                "entry '{owner}' does not have one authenticated-success block for each typed opening"
            )));
        }

        let owner_end = record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == owner)
            .map(|block| block.range.end)
            .max()
            .ok_or_else(|| commitment_machine_error(format!("entry '{owner}' has no machine coverage")))?;
        let mut open_index = 0usize;
        for (index, (contract, (marker, _))) in owner_contracts.iter().zip(markers.iter()).enumerate() {
            let end = markers.get(index + 1).map_or(owner_end, |(next, _)| next.range.start);
            validate_commitment_header(record, elf, contract, marker)?;
            match contract.kind {
                CommitmentMachineKind::Open => {
                    let authenticated = verified[open_index];
                    open_index += 1;
                    if !(marker.range.start < authenticated.range.start && authenticated.range.start < end) {
                        return Err(commitment_machine_error(
                            "authenticated opening success block is outside its typed operation order",
                        ));
                    }
                    validate_one_commitment_open_machine_contract(record, elf, contract, marker.range.start, authenticated, end)?;
                }
                CommitmentMachineKind::Commit => {
                    validate_one_commitment_commit_machine_contract(record, elf, contract, marker.range.start, end)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn committed_state_machine_contracts(
    record: &VerifiedLoweringRecord,
) -> Result<Vec<CommitmentMachineContract>, CheckerError> {
    let mut contracts = Vec::new();
    for entry in &record.typed_semantics.entries {
        for block in &entry.blocks {
            for operation in &block.operations {
                let Some(call) = &operation.call else { continue };
                let (kind, inner_type) = if call.target == "__ckb_commitment_open" {
                    // The wrapper retains source-domain spelling. A materialized
                    // Hash result uses `hash` in typed wire; that is not its V0
                    // preimage domain. Typed validation checked the exact pair.
                    let inner = call
                        .params
                        .first()
                        .and_then(|ty| commitment_inner_type(ty))
                        .ok_or_else(|| commitment_machine_error("typed opening lacks its packed source domain"))?;
                    (CommitmentMachineKind::Open, inner)
                } else if call.target == "__ckb_hash_blake2b_packed" {
                    let Some(inner) = commitment_inner_type(&call.return_type) else { continue };
                    (CommitmentMachineKind::Commit, inner)
                } else {
                    continue;
                };
                let width = committed_state_type_width(inner_type, &record.typed_semantics.types).ok_or_else(|| {
                    commitment_machine_error(format!("typed committed value '{inner_type}' has no fixed-width layout"))
                })?;
                let destination = operation
                    .destinations
                    .first()
                    .copied()
                    .ok_or_else(|| commitment_machine_error("typed committed-state operation has no destination"))?;
                let local = |index: usize| operation.operands.get(index).and_then(|operand| operand.local);
                contracts.push(CommitmentMachineContract {
                    owner: entry.id.clone(),
                    kind,
                    inner_type: inner_type.to_string(),
                    width,
                    destination,
                    source: (kind == CommitmentMachineKind::Commit).then(|| local(0)).flatten(),
                    expected: if kind == CommitmentMachineKind::Open {
                        Some(commitment_machine_pointer_source(
                            entry,
                            local(0).ok_or_else(|| commitment_machine_error("typed opening has no commitment local"))?,
                            &record.typed_semantics.types,
                        )?)
                    } else {
                        None
                    },
                    opening: (kind == CommitmentMachineKind::Open).then(|| local(1)).flatten(),
                });
            }
        }
    }
    Ok(contracts)
}

pub(super) fn commitment_machine_pointer_source(
    entry: &crate::schema::TypedSemanticEntry,
    local: u32,
    types: &[TypedSemanticType],
) -> Result<CommitmentMachinePointerSource, CheckerError> {
    let mut current = local;
    let mut displacement = 0i64;
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current) {
            return Err(commitment_machine_error("commitment pointer provenance contains a field-access cycle"));
        }
        let producer =
            entry.blocks.iter().flat_map(|block| &block.operations).find(|operation| operation.destinations.contains(&current));
        let Some(operation) = producer else { break };
        if operation.opcode != "field-access" {
            break;
        }
        let crate::schema::TypedSemanticOperationDetail::Field { name } = &operation.detail else {
            return Err(commitment_machine_error("commitment field pointer has inconsistent typed semantics"));
        };
        let owner =
            operation.operands.first().ok_or_else(|| commitment_machine_error("commitment field pointer has no owner operand"))?;
        let owner_local =
            owner.local.ok_or_else(|| commitment_machine_error("commitment field pointer owner is not a typed local"))?;
        let offset = commitment_machine_field_offset(&owner.ty, name, types)
            .ok_or_else(|| commitment_machine_error("commitment field pointer has no fixed typed layout"))?;
        displacement = displacement
            .checked_add(offset)
            .ok_or_else(|| commitment_machine_error("commitment field pointer displacement overflowed"))?;
        current = owner_local;
    }
    Ok(CommitmentMachinePointerSource { stack_slot: i64::from(local_stack_offset(current)?), displacement })
}

pub(super) fn commitment_machine_field_offset(ty: &str, field: &str, types: &[TypedSemanticType]) -> Option<i64> {
    let ty = canonical_abi_type(strip_reference(ty));
    if let Some(layout) = types.iter().find(|layout| canonical_abi_type(&layout.name) == ty) {
        return layout.fields.iter().find(|candidate| candidate.name == field).map(|candidate| i64::from(candidate.offset));
    }
    if matches!(ty.as_str(), "address" | "hash" | "ScriptHash") && field == "0" {
        return Some(0);
    }
    if let Some(body) = ty.strip_prefix('[').and_then(|value| value.strip_suffix(']')) {
        let (element, length) = split_top_level_once(body, ';')?;
        let index = field.parse::<u64>().ok()?;
        if index >= length.parse::<u64>().ok()? {
            return None;
        }
        let width = committed_state_type_width(element, types)?;
        return index.checked_mul(width).and_then(|offset| i64::try_from(offset).ok());
    }
    if let Some(body) = ty.strip_prefix('(').and_then(|value| value.strip_suffix(')')) {
        let fields = split_top_level(body, ',');
        let index = field.parse::<usize>().ok()?;
        if index >= fields.len() {
            return None;
        }
        return fields[..index]
            .iter()
            .try_fold(0u64, |total, field_ty| total.checked_add(committed_state_type_width(field_ty, types)?))
            .and_then(|offset| i64::try_from(offset).ok());
    }
    None
}

pub(super) fn validate_commitment_header(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    contract: &CommitmentMachineContract,
    marker: &LoweringBlock,
) -> Result<(), CheckerError> {
    let entry = record
        .entries
        .iter()
        .find(|entry| entry.id == contract.owner)
        .ok_or_else(|| commitment_machine_error(format!("missing lowering entry '{}'", contract.owner)))?;
    let scratch = u64::from(entry.frame_size_bytes)
        .checked_sub(COMMITMENT_RUNTIME_TRAILER_BYTES + COMMITMENT_RUNTIME_SCRATCH_BYTES)
        .and_then(|offset| offset.checked_add(8))
        .ok_or_else(|| commitment_machine_error("entry frame is too small for the bounded commitment scratch region"))?;
    let mut header = COMMITMENT_DOMAIN_PREFIX.to_vec();
    header.extend_from_slice(contract.inner_type.as_bytes());
    header.push(0);
    header.extend_from_slice(
        &u32::try_from(contract.width).map_err(|_| commitment_machine_error("committed value width exceeds u32"))?.to_le_bytes(),
    );
    let digest_bytes = if contract.kind == CommitmentMachineKind::Open { 32 } else { 0 };
    let required = header
        .len()
        .checked_add(usize::try_from(contract.width).map_err(|_| commitment_machine_error("committed value width exceeds usize"))?)
        .and_then(|total| total.checked_add(digest_bytes))
        .ok_or_else(|| commitment_machine_error("commitment scratch requirement overflowed"))?;
    if contract.width == 0 || required > COMMITMENT_SCRATCH_BUFFER_BYTES as usize {
        return Err(commitment_machine_error("typed commitment exceeds the fixed 512-byte scratch contract"));
    }

    let mut cursor = marker.range.start;
    for (index, byte) in header.iter().enumerate().rev() {
        let offset = scratch
            .checked_add(u64::try_from(index).map_err(|_| commitment_machine_error("commitment header index overflowed"))?)
            .and_then(|offset| i32::try_from(offset).ok())
            .ok_or_else(|| commitment_machine_error("commitment header stack offset exceeds i32"))?;
        if (-2_048..=2_047).contains(&offset) {
            let load = cursor.checked_sub(8).ok_or_else(|| commitment_machine_error("commitment header underflowed machine text"))?;
            if !is_addi(commitment_word(elf, load)?, 5, 0, i32::from(*byte)) || !is_sb(commitment_word(elf, load + 4)?, 5, 2, offset) {
                return Err(commitment_machine_error("domain/type/width header bytes differ from typed semantics"));
            }
            cursor = load;
        } else {
            let load =
                cursor.checked_sub(20).ok_or_else(|| commitment_machine_error("large commitment header underflowed machine text"))?;
            let words = commitment_instructions_from(elf, load, 5)?;
            if !is_addi(words[0].word, 5, 0, i32::from(*byte))
                || !matches_large_stack_address(words, 1, 31, offset)
                || !is_sb(words[4].word, 5, 31, 0)
            {
                return Err(commitment_machine_error("large-stack domain/type/width header bytes differ from typed semantics"));
            }
            cursor = load;
        }
    }
    Ok(())
}

pub(super) fn validate_one_commitment_open_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    contract: &CommitmentMachineContract,
    start: u64,
    authenticated: &LoweringBlock,
    end: u64,
) -> Result<(), CheckerError> {
    let header_len = COMMITMENT_DOMAIN_PREFIX.len() + contract.inner_type.len() + 1 + 4;
    let total_width = u64::try_from(header_len)
        .ok()
        .and_then(|header| header.checked_add(contract.width))
        .ok_or_else(|| commitment_machine_error("opening preimage width overflowed"))?;
    let scratch = commitment_scratch_offset(record, &contract.owner)?;
    let digest = scratch.checked_add(total_width).ok_or_else(|| commitment_machine_error("opening digest offset overflowed"))?;
    let memcpy = exact_runtime_calls(record, elf, "__cellscript_memcpy_fixed", start, authenticated.range.start)?;
    let hashes = exact_runtime_calls(record, elf, "__ckb_hash_blake2b_var", start, authenticated.range.start)?;
    let comparisons = exact_runtime_calls(record, elf, "__cellscript_memcmp_fixed", start, authenticated.range.start)?;
    let ([copy], [hash], [compare]) = (memcpy.as_slice(), hashes.as_slice(), comparisons.as_slice()) else {
        return Err(commitment_machine_error(
            "typed opening must contain exactly one preimage copy, one Blake2b hash, and one digest comparison before authentication",
        ));
    };
    if !(*copy < *hash && *hash < *compare) {
        return Err(commitment_machine_error("typed opening copy/hash/compare sequence is reordered"));
    }
    let opening = contract.opening.ok_or_else(|| commitment_machine_error("typed opening has no direct witness local"))?;
    let expected = contract.expected.ok_or_else(|| commitment_machine_error("typed opening has no commitment pointer source"))?;
    if !has_exact_commitment_size_guard(record, elf, start, *copy, contract.width)? {
        return Err(commitment_machine_error("opening witness has no exact fixed-width guard"));
    }
    if !has_stack_load(elf, start, *copy, 10, local_stack_offset(opening)?) {
        return Err(commitment_machine_error("opening witness pointer is not loaded from the typed local"));
    }
    let copy_args = commitment_registers_before(elf, start, *copy);
    if copy_args[11] != CommitmentMachineValue::StackAddress(i64::try_from(scratch + header_len as u64).unwrap_or(i64::MAX))
        || copy_args[12] != CommitmentMachineValue::Constant(contract.width)
    {
        return Err(commitment_machine_error("opening preimage copy uses the wrong scratch offset or width"));
    }
    let hash_args = commitment_registers_before(elf, start, *hash);
    if hash_args[10] != CommitmentMachineValue::StackAddress(i64::try_from(scratch).unwrap_or(i64::MAX))
        || hash_args[11] != CommitmentMachineValue::Constant(total_width)
        || hash_args[12] != CommitmentMachineValue::StackAddress(i64::try_from(digest).unwrap_or(i64::MAX))
        || !hash_call_checks_status(record, elf, *hash)
    {
        return Err(commitment_machine_error("opening Blake2b call no longer binds the exact preimage, digest slot, and status"));
    }
    let compare_args = commitment_registers_before(elf, start, *compare);
    if compare_args[10] != CommitmentMachineValue::StackAddress(i64::try_from(digest).unwrap_or(i64::MAX))
        || compare_args[12] != CommitmentMachineValue::Constant(32)
        || compare_args[11]
            != (CommitmentMachineValue::StackLoadedAddress { stack_slot: expected.stack_slot, displacement: expected.displacement })
        || !is_beq(commitment_word(elf, compare + 4)?, 10, 0)
        || !flow_targets(elf, compare + 4, authenticated.range.start)
        || !jump_targets_runtime_error(record, elf, compare + 8, 73)
    {
        return Err(commitment_machine_error("opening digest comparison no longer gates authentication with stable error 73"));
    }
    let destination = local_stack_offset(contract.destination)?;
    let materialized = has_stack_store(elf, authenticated.range.start, end, 5, destination)
        .ok_or_else(|| commitment_machine_error("typed opening result is not materialized after authentication"))?;
    if has_stack_store(elf, start, authenticated.range.start, 5, destination).is_some() {
        return Err(commitment_machine_error(
            "typed opening result is materialized before authentication or not materialized afterwards",
        ));
    }
    commitment_materialization::validate(record, elf, contract, authenticated.range.start, compare + 4, materialized)
}

pub(super) fn validate_one_commitment_commit_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    contract: &CommitmentMachineContract,
    start: u64,
    end: u64,
) -> Result<(), CheckerError> {
    let header_len = COMMITMENT_DOMAIN_PREFIX.len() + contract.inner_type.len() + 1 + 4;
    let total_width = u64::try_from(header_len)
        .ok()
        .and_then(|header| header.checked_add(contract.width))
        .ok_or_else(|| commitment_machine_error("recommit preimage width overflowed"))?;
    let scratch = commitment_scratch_offset(record, &contract.owner)?;
    let copy = exact_runtime_calls(record, elf, "__cellscript_memcpy_fixed", start, end)?
        .into_iter()
        .next()
        .ok_or_else(|| commitment_machine_error("recommit block has no fixed-width preimage copy"))?;
    let hash = exact_runtime_calls(record, elf, "__ckb_hash_blake2b_var", start, end)?
        .into_iter()
        .find(|address| *address > copy)
        .ok_or_else(|| commitment_machine_error("recommit block has no Blake2b call after its preimage copy"))?;
    let copy_args = commitment_registers_before(elf, start, copy);
    if copy_args[11] != CommitmentMachineValue::StackAddress(i64::try_from(scratch + header_len as u64).unwrap_or(i64::MAX))
        || copy_args[12] != CommitmentMachineValue::Constant(contract.width)
    {
        return Err(commitment_machine_error("recommit preimage copy uses the wrong scratch offset or width"));
    }
    if let Some(source) = contract.source {
        let source_offset = local_stack_offset(source)?;
        if copy_args[10] != CommitmentMachineValue::StackAddress(i64::from(source_offset))
            && !has_stack_load(elf, start, copy, 10, source_offset)
        {
            return Err(commitment_machine_error("recommit preimage is not sourced from its typed input local"));
        }
    }
    let hash_args = commitment_registers_before(elf, start, hash);
    let CommitmentMachineValue::StackAddress(output_buffer) = hash_args[12] else {
        return Err(commitment_machine_error("recommit hash output is not a bounded stack buffer"));
    };
    if hash_args[10] != CommitmentMachineValue::StackAddress(i64::try_from(scratch).unwrap_or(i64::MAX))
        || hash_args[11] != CommitmentMachineValue::Constant(total_width)
        || !hash_call_checks_status(record, elf, hash)
    {
        return Err(commitment_machine_error("recommit Blake2b call no longer binds the exact typed preimage and status"));
    }
    let destination = local_stack_offset(contract.destination)?;
    let Some(store) = has_stack_store(elf, hash + 4, end, 5, destination) else {
        return Err(commitment_machine_error("recommit digest is not stored in its typed Commitment destination"));
    };
    let values = commitment_registers_before(elf, start, store);
    if values[5] != CommitmentMachineValue::StackAddress(output_buffer) {
        return Err(commitment_machine_error("recommit destination points at bytes other than the Blake2b output"));
    }
    Ok(())
}

pub(crate) fn committed_state_type_width(ty: &str, types: &[TypedSemanticType]) -> Option<u64> {
    let ty = canonical_abi_type(ty);
    let primitive = match ty.as_str() {
        "unit" => Some(0),
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" | "i32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "address" | "hash" | "ScriptHash" | "SighashAllDigest" => Some(32),
        _ => None,
    };
    if primitive.is_some() {
        return primitive;
    }
    if let Some(layout) = types.iter().find(|layout| canonical_abi_type(&layout.name) == ty) {
        return layout.encoded_size.map(u64::from);
    }
    if let Some(body) = ty.strip_prefix('[').and_then(|value| value.strip_suffix(']')) {
        let (element, length) = split_top_level_once(body, ';')?;
        return committed_state_type_width(element, types)?.checked_mul(length.parse::<u64>().ok()?);
    }
    if let Some(body) = ty.strip_prefix('(').and_then(|value| value.strip_suffix(')')) {
        return split_top_level(body, ',')
            .into_iter()
            .filter(|item| !item.is_empty())
            .try_fold(0u64, |total, item| total.checked_add(committed_state_type_width(item, types)?));
    }
    None
}

pub(super) fn split_top_level_once(value: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut angle = 0usize;
    let mut square = 0usize;
    let mut paren = 0usize;
    for (index, character) in value.char_indices() {
        match character {
            '<' => angle += 1,
            '>' => angle = angle.checked_sub(1)?,
            '[' => square += 1,
            ']' => square = square.checked_sub(1)?,
            '(' => paren += 1,
            ')' => paren = paren.checked_sub(1)?,
            current if current == delimiter && angle == 0 && square == 0 && paren == 0 => {
                let next = index + character.len_utf8();
                return Some((&value[..index], &value[next..]));
            }
            _ => {}
        }
    }
    None
}

pub(super) fn split_top_level(value: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = value;
    while let Some((head, tail)) = split_top_level_once(rest, delimiter) {
        parts.push(head);
        rest = tail;
    }
    parts.push(rest);
    parts
}

pub(super) fn commitment_inner_type(ty: &str) -> Option<&str> {
    ty.strip_prefix("Commitment<")?.strip_suffix('>')
}

pub(super) fn commitment_scratch_offset(record: &VerifiedLoweringRecord, owner: &str) -> Result<u64, CheckerError> {
    record
        .entries
        .iter()
        .find(|entry| entry.id == owner)
        .and_then(|entry| {
            u64::from(entry.frame_size_bytes)
                .checked_sub(COMMITMENT_RUNTIME_TRAILER_BYTES + COMMITMENT_RUNTIME_SCRATCH_BYTES)
                .and_then(|offset| offset.checked_add(8))
        })
        .ok_or_else(|| commitment_machine_error(format!("entry '{owner}' has no bounded commitment scratch offset")))
}

pub(super) fn exact_runtime_calls(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    name: &str,
    start: u64,
    end: u64,
) -> Result<Vec<u64>, CheckerError> {
    let entry = record
        .entries
        .iter()
        .find(|entry| entry.id == format!("runtime:{name}"))
        .ok_or_else(|| commitment_machine_error(format!("missing runtime helper '{name}'")))?;
    let target = record
        .blocks
        .iter()
        .find(|block| block.id == entry.entry_block)
        .map(|block| block.range.start)
        .ok_or_else(|| commitment_machine_error(format!("runtime helper '{name}' has no entry block")))?;
    let mut calls = elf
        .control_flow
        .iter()
        .filter(|edge| start <= edge.address && edge.address < end && edge.target == target)
        .filter_map(|edge| {
            let word = commitment_word(elf, edge.address).ok()?;
            let prior = edge.address.checked_sub(4).and_then(|address| commitment_word(elf, address).ok())?;
            (is_auipc(prior, 1) && is_jalr_call(word)).then_some(edge.address)
        })
        .collect::<Vec<_>>();
    calls.sort_unstable();
    Ok(calls)
}

pub(super) fn has_exact_commitment_size_guard(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    start: u64,
    end: u64,
    width: u64,
) -> Result<bool, CheckerError> {
    for instruction in elf.instructions.iter().filter(|instruction| start <= instruction.address && instruction.address < end) {
        if !is_sub(instruction.word, 10, 10, 11)
            || commitment_registers_before(elf, start, instruction.address)[11] != CommitmentMachineValue::Constant(width)
        {
            continue;
        }
        let branch = instruction.address + 4;
        if is_beq(commitment_word(elf, branch)?, 10, 0)
            && flow_targets(elf, branch, instruction.address + 12)
            && jump_targets_runtime_error(record, elf, instruction.address + 8, 4)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn hash_call_checks_status(record: &VerifiedLoweringRecord, elf: &ParsedElf, call: u64) -> bool {
    commitment_word(elf, call + 4).is_ok_and(|word| is_beq(word, 10, 0))
        && flow_targets(elf, call + 4, call + 12)
        && jump_targets_runtime_error(record, elf, call + 8, 1)
}

pub(super) fn has_stack_load(elf: &ParsedElf, start: u64, end: u64, destination: u32, offset: i32) -> bool {
    let instructions =
        elf.instructions.iter().filter(|instruction| start <= instruction.address && instruction.address < end).collect::<Vec<_>>();
    instructions.iter().enumerate().any(|(index, instruction)| {
        is_ld(instruction.word, destination, 2, offset)
            || index >= 3
                && is_ld(instruction.word, destination, 31, 0)
                && matches_large_stack_address(
                    &[instructions[index - 3].to_owned(), instructions[index - 2].to_owned(), instructions[index - 1].to_owned()],
                    0,
                    31,
                    offset,
                )
    })
}

pub(super) fn has_stack_store(elf: &ParsedElf, start: u64, end: u64, source: u32, offset: i32) -> Option<u64> {
    let instructions =
        elf.instructions.iter().filter(|instruction| start <= instruction.address && instruction.address < end).collect::<Vec<_>>();
    instructions.iter().enumerate().find_map(|(index, instruction)| {
        let direct = is_sd(instruction.word, source, 2, offset);
        let large = index >= 3
            && is_sd(instruction.word, source, 31, 0)
            && matches_large_stack_address(
                &[instructions[index - 3].to_owned(), instructions[index - 2].to_owned(), instructions[index - 1].to_owned()],
                0,
                31,
                offset,
            );
        (direct || large).then_some(instruction.address)
    })
}

pub(super) fn local_stack_offset(local: u32) -> Result<i32, CheckerError> {
    local
        .checked_mul(8)
        .and_then(|offset| i32::try_from(offset).ok())
        .ok_or_else(|| commitment_machine_error("typed local stack offset overflowed"))
}

pub(super) fn commitment_registers_before(elf: &ParsedElf, start: u64, address: u64) -> [CommitmentMachineValue; 32] {
    use std::collections::VecDeque;
    let mut unknown = [CommitmentMachineValue::Unknown; 32];
    unknown[0] = CommitmentMachineValue::Constant(0);
    let mut initial = unknown;
    initial[2] = CommitmentMachineValue::StackAddress(0);
    let words = elf
        .instructions
        .iter()
        .filter(|instruction| start <= instruction.address && instruction.address < address)
        .map(|instruction| (instruction.address, instruction.word))
        .collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect::<BTreeMap<_, _>>();
    let mut pending = VecDeque::from([(start, initial)]);
    // External entries cannot borrow facts established only on the normal prefix.
    for flow in &elf.control_flow {
        if !(start..address).contains(&flow.address) && start < flow.target && flow.target <= address {
            pending.push_back((flow.target, unknown));
        }
    }
    let mut incoming = BTreeMap::<u64, [CommitmentMachineValue; 32]>::new();
    let mut steps = 0usize;
    while let Some((pc, mut values)) = pending.pop_front() {
        steps += 1;
        if steps > words.len().saturating_add(1).saturating_mul(64).min(4_000_000) {
            return unknown;
        }
        if let Some(previous) = incoming.get(&pc) {
            for (value, old) in values.iter_mut().zip(previous) {
                if *value != *old {
                    *value = CommitmentMachineValue::Unknown;
                }
            }
            if &values == previous {
                continue;
            }
        }
        incoming.insert(pc, values);
        if pc == address {
            continue;
        }
        let Some(&word) = words.get(&pc) else { continue };
        let Some(instruction) = crate::machine::decode(word) else { return unknown };
        let exits = word == 0x00000073 && values[17] == CommitmentMachineValue::Constant(93);
        update_commitment_registers(&mut values, word);
        let next = pc + crate::machine::Instruction::BYTE_LENGTH;
        let target = || targets.get(&pc).copied().filter(|target| start <= *target && *target <= address);
        match instruction.operation() {
            crate::machine::Operation::Branch { .. } => {
                pending.push_back((next, values));
                if let Some(target) = target() {
                    pending.push_back((target, values));
                }
            }
            crate::machine::Operation::Jal { rd: 0, .. } => {
                if let Some(target) = target() {
                    pending.push_back((target, values));
                }
            }
            crate::machine::Operation::Jalr { rd: 0, .. } => {}
            _ if !exits => pending.push_back((next, values)),
            _ => {}
        }
    }
    incoming.get(&address).copied().unwrap_or(unknown)
}

pub(super) fn update_commitment_registers(values: &mut [CommitmentMachineValue; 32], word: u32) {
    let opcode = word & 0x7f;
    let destination = crate::machine::decode(word).and_then(|instruction| instruction.destination()).map(usize::from);
    let Some(destination) = destination else { return };
    let source1 = ((word >> 15) & 0x1f) as usize;
    let source2 = ((word >> 20) & 0x1f) as usize;
    let function = (word >> 12) & 0x7;
    let immediate = (word as i32 >> 20) as i64;
    let add = |left: CommitmentMachineValue, right: CommitmentMachineValue| match (left, right) {
        (CommitmentMachineValue::Constant(left), CommitmentMachineValue::Constant(right)) => {
            CommitmentMachineValue::Constant(left.wrapping_add(right))
        }
        (CommitmentMachineValue::StackAddress(offset), CommitmentMachineValue::Constant(value))
        | (CommitmentMachineValue::Constant(value), CommitmentMachineValue::StackAddress(offset)) => i64::try_from(value)
            .ok()
            .and_then(|value| offset.checked_add(value))
            .map_or(CommitmentMachineValue::Unknown, CommitmentMachineValue::StackAddress),
        (CommitmentMachineValue::StackLoadedAddress { stack_slot, displacement }, CommitmentMachineValue::Constant(value))
        | (CommitmentMachineValue::Constant(value), CommitmentMachineValue::StackLoadedAddress { stack_slot, displacement }) => {
            i64::try_from(value).ok().and_then(|value| displacement.checked_add(value)).map_or(
                CommitmentMachineValue::Unknown,
                |displacement| CommitmentMachineValue::StackLoadedAddress { stack_slot, displacement },
            )
        }
        _ => CommitmentMachineValue::Unknown,
    };
    let value = match opcode {
        0x37 => CommitmentMachineValue::Constant(((word & 0xffff_f000) as i32 as i64) as u64),
        0x13 if function == 0 => match values[source1] {
            CommitmentMachineValue::Constant(value) => CommitmentMachineValue::Constant(value.wrapping_add_signed(immediate)),
            CommitmentMachineValue::StackAddress(offset) => {
                offset.checked_add(immediate).map_or(CommitmentMachineValue::Unknown, CommitmentMachineValue::StackAddress)
            }
            CommitmentMachineValue::StackLoadedAddress { stack_slot, displacement } => {
                displacement.checked_add(immediate).map_or(CommitmentMachineValue::Unknown, |displacement| {
                    CommitmentMachineValue::StackLoadedAddress { stack_slot, displacement }
                })
            }
            CommitmentMachineValue::Unknown => CommitmentMachineValue::Unknown,
        },
        0x1b if function == 0 => match values[source1] {
            CommitmentMachineValue::Constant(value) => {
                CommitmentMachineValue::Constant(((value.wrapping_add_signed(immediate) as u32) as i32 as i64) as u64)
            }
            _ => CommitmentMachineValue::Unknown,
        },
        0x33 if function == 0 && (word >> 25) & 0x7f == 0 => add(values[source1], values[source2]),
        0x33 if function == 0 && (word >> 25) & 0x7f == 0x20 => match (values[source1], values[source2]) {
            (CommitmentMachineValue::Constant(left), CommitmentMachineValue::Constant(right)) => {
                CommitmentMachineValue::Constant(left.wrapping_sub(right))
            }
            _ => CommitmentMachineValue::Unknown,
        },
        0x03 if function == 3 => match values[source1] {
            CommitmentMachineValue::StackAddress(offset) => {
                offset.checked_add(immediate).map_or(CommitmentMachineValue::Unknown, |stack_slot| {
                    CommitmentMachineValue::StackLoadedAddress { stack_slot, displacement: 0 }
                })
            }
            _ => CommitmentMachineValue::Unknown,
        },
        0x03 | 0x17 | 0x6f | 0x67 => CommitmentMachineValue::Unknown,
        _ => {
            if crate::machine::decode(word).is_some_and(|instruction| instruction.effects().writes_register(destination as u32)) {
                CommitmentMachineValue::Unknown
            } else {
                return;
            }
        }
    };
    if destination != 0 {
        values[destination] = value;
    }
    if crate::machine::decode(word).is_some_and(|instruction| instruction.is_call()) {
        for register in [1usize, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 28, 29, 30, 31] {
            values[register] = CommitmentMachineValue::Unknown;
        }
        values[0] = CommitmentMachineValue::Constant(0);
    }
}

pub(super) fn commitment_word(elf: &ParsedElf, address: u64) -> Result<u32, CheckerError> {
    elf.instructions
        .iter()
        .find(|instruction| instruction.address == address)
        .map(|instruction| instruction.word)
        .ok_or_else(|| commitment_machine_error(format!("missing instruction at {address:#x}")))
}

pub(super) fn commitment_instructions_from(
    elf: &ParsedElf,
    address: u64,
    count: usize,
) -> Result<&[crate::elf::DecodedInstruction], CheckerError> {
    let start = elf
        .instructions
        .binary_search_by_key(&address, |instruction| instruction.address)
        .map_err(|_| commitment_machine_error(format!("missing instruction range at {address:#x}")))?;
    elf.instructions
        .get(start..start.saturating_add(count))
        .ok_or_else(|| commitment_machine_error(format!("truncated instruction range at {address:#x}")))
}

pub(super) fn commitment_machine_error(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("typed committed-state machine contract: {}", message.into()),
    )
}
