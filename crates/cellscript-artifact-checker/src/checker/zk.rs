//! Independent bounded machine checks for the exact ZK parent profile.
use super::*;
use crate::zk as wire;

fn error(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("ZK parent: {message}"))
}

fn checked(record: &VerifiedLoweringRecord, elf: &ParsedElf, address: u64, code: i32) -> Result<(), CheckerError> {
    if !is_beq(bounded_word(elf, address + 4)?, 10, 0)
        || !flow_targets(elf, address + 4, address + 12)
        || !jump_targets_runtime_error(record, elf, address + 8, code)
    {
        return Err(error("missing mandatory rejecting result branch"));
    }
    Ok(())
}

fn exact_length(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    start: u64,
    address: u64,
    size: i64,
    width: u64,
    code: i32,
) -> Result<(), CheckerError> {
    let branch = elf
        .instructions
        .iter()
        .find(|instruction| instruction.address >= address + 12 && instruction.word & 0x7f == 0x63)
        .ok_or_else(|| error("missing exact transferred length check"))?;
    let values = commitment_registers_before(elf, start, branch.address);
    if !is_beq(branch.word, 5, 6)
        || values[5] != (CommitmentMachineValue::StackLoadedAddress { stack_slot: size, displacement: 0 })
        || values[6] != CommitmentMachineValue::Constant(width)
        || !flow_targets(elf, branch.address, branch.address + 8)
        || !jump_targets_runtime_error(record, elf, branch.address + 4, code)
    {
        return Err(error("transferred length is not checked exactly"));
    }
    let mut cursor = address.saturating_sub(128).max(start);
    let mut last = None;
    while let Some(store) = has_stack_store(elf, cursor, address, 5, size as i32) {
        last = Some(store);
        cursor = store + 4;
    }
    let store = last.ok_or_else(|| error("missing bounded length initialization"))?;
    let block = block_for_address(record, store).ok_or_else(|| error("length store outside block"))?;
    if register_constant_before(elf, block, store, 5) != Some(width) {
        return Err(error("wrong bounded length initialization"));
    }
    Ok(())
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    for entry in &record.typed_semantics.entries {
        let calls: Vec<_> = entry
            .blocks
            .iter()
            .flat_map(|block| {
                block.operations.iter().enumerate().filter_map(move |(index, op)| {
                    op.call.as_ref().filter(|call| call.target == "__zk_require_transition").map(|_| (block, index, op))
                })
            })
            .collect();
        if calls.is_empty() {
            continue;
        }
        let [(block, index, call)] = calls.as_slice() else {
            return Err(error("more than one verifier call"));
        };
        if !entry.id.starts_with("action:")
            || block.id != entry.blocks[0].id
            || *index == 0
            || entry.blocks.iter().any(|other| other.successors.contains(&block.id))
            || call.operands.len() != 7
            || !call.destinations.is_empty()
            || call.operands[1].ty != "ZkTransitionProof"
            || call.operands[2].ty != "CellDepView"
        {
            return Err(error("typed profile, proof, call bound or unconditional placement differs"));
        }
        let preflight = &block.operations[index - 1];
        if !entry.params.iter().any(|parameter| {
            Some(parameter.binding_id) == call.operands[1].local
                && parameter.source == "witness"
                && parameter.ty == "ZkTransitionProof"
        }) {
            return Err(error("proof is not a direct witness parameter"));
        }
        if preflight.call.as_ref().is_none_or(|call| call.target != "__ckb_require_cell_dep_exact_verifier_handle")
            || preflight.operands.len() != 3
            || preflight.operands[0] != call.operands[2]
            || preflight.operands[2] != call.operands[3]
        {
            return Err(error("exact handle preflight does not bind this verifier call"));
        }
        let hash = |index: usize| -> Result<[u8; 32], CheckerError> {
            let Some(TypedSemanticConstant::Hash(hash)) = &call.operands[index].constant else {
                return Err(error("identity is not a literal hash"));
            };
            if hash.len() != 64 || !hash.is_ascii() {
                return Err(error("identity hash width"));
            }
            let mut bytes = [0; 32];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&hash[index * 2..index * 2 + 2], 16).map_err(|_| error("invalid identity hash"))?;
            }
            Ok(bytes)
        };
        let _ = hash(3)?;
        let expected = wire::Request {
            verification_key: hash(4)?,
            proof: [0; 128],
            statement: wire::Statement {
                domain: hash(5)?,
                action: hash(6)?,
                script_hash: [0; 32],
                old_data_hash: [0; 32],
                new_data_hash: [0; 32],
                input_transaction_hash: [0; 32],
                input_output_index: 0,
                transaction_hash: [0; 32],
            },
        }
        .encode();
        let begins = generated_blocks(record, &entry.id, ".Lzk_begin_");
        let initialized = generated_blocks(record, &entry.id, ".Lzk_request_initialized_");
        let verified = generated_blocks(record, &entry.id, ".Lzk_verified_");
        let ([begin], [initialized], [verified]) = (begins.as_slice(), initialized.as_slice(), verified.as_slice()) else {
            return Err(error("missing exact request or checked-result machine boundary"));
        };
        let start = begin.range.start;
        let end = verified.range.start;
        let scratch = commitment_scratch_offset(record, &entry.id)? as i64;
        if !(start < initialized.range.start && initialized.range.start < end) {
            return Err(error("request order changed"));
        }
        // Every initial request word is independently tied to typed identities
        // and canonical offsets; dynamic fields must begin as zero.
        for (index, word) in expected.chunks_exact(8).enumerate() {
            let offset = i32::try_from(scratch + (index * 8) as i64).map_err(|_| error("scratch overflow"))?;
            let store = has_stack_store(elf, start, initialized.range.start, 5, offset)
                .ok_or_else(|| error("request word not initialized"))?;
            let mut range = (*begin).clone();
            range.range.end = initialized.range.start;
            if register_constant_before(elf, &range, store, 5) != Some(u64::from_le_bytes(word.try_into().unwrap())) {
                return Err(error("profile, VK, domain, action or request bytes differ from typed semantics"));
            }
        }
        let syscall_sites: Vec<_> = record.syscall_sites.iter().filter(|site| start <= site.address && site.address < end).collect();
        let constant = |site: &SyscallSite, register| {
            block_for_address(record, site.address).and_then(|block| register_constant_before(elf, block, site.address, register))
        };
        let copies = exact_runtime_calls(record, elf, "__cellscript_memcpy_fixed", start, end)?;
        let [copy] = copies.as_slice() else {
            return Err(error("missing exact proof copy"));
        };
        let registers = commitment_registers_before(elf, start, *copy);
        let proof_local = call.operands[1].local.ok_or_else(|| error("missing proof local"))?;
        let source_id =
            entry.locals.iter().find(|local| local.id == proof_local).ok_or_else(|| error("missing proof binding"))?.source_id;
        let source_slot = local_stack_offset(u32::try_from(source_id).map_err(|_| error("proof local overflow"))?)?;
        if registers[10] != (CommitmentMachineValue::StackLoadedAddress { stack_slot: i64::from(source_slot), displacement: 0 })
            || registers[11] != CommitmentMachineValue::StackAddress(scratch + 108)
            || registers[12] != CommitmentMachineValue::Constant(128)
        {
            return Err(error("proof copy source, destination or width changed"));
        }
        let clocks: Vec<_> = syscall_sites.iter().filter(|site| constant(site, 17) == Some(2042)).collect();
        let [clock_end] = clocks.as_slice() else {
            return Err(error("cycle measurement count changed"));
        };
        if !is_addi(bounded_word(elf, start - 12)?, 17, 0, 2042)
            || bounded_word(elf, start - 8)? != 0x73
            || !is_sd(bounded_word(elf, start - 4)?, 10, 2, (scratch - 104) as i32)
            || !is_ld(bounded_word(elf, clock_end.address + 4)?, 5, 2, (scratch - 104) as i32)
            || !is_sub(bounded_word(elf, clock_end.address + 8)?, 5, 10, 5)
            || !is_bgeu(bounded_word(elf, end - 8)?, 6, 5)
            || !flow_targets(elf, end - 8, end)
            || !jump_targets_runtime_error(record, elf, end - 4, 84)
            || block_for_address(record, end - 8).and_then(|block| register_constant_before(elf, block, end - 8, 6))
                != Some(250_000_000)
        {
            return Err(error("cycle accounting or budget rejection changed"));
        }
        if syscall_sites.len() != wire::REQUEST_WORDS + 10 {
            return Err(error("unexpected syscall in ZK region"));
        }
        let writes: Vec<_> = syscall_sites.iter().filter(|site| constant(site, 17) == Some(2605)).collect();
        if writes.len() != wire::REQUEST_WORDS {
            return Err(error("request write count changed"));
        }
        for (index, site) in writes.iter().enumerate() {
            let registers = commitment_registers_before(elf, start, site.address);
            if registers[11] != CommitmentMachineValue::StackAddress(scratch + (index * 8) as i64)
                || registers[10] != (CommitmentMachineValue::StackLoadedAddress { stack_slot: scratch - 128, displacement: 0 })
                || registers[12] != CommitmentMachineValue::StackAddress(scratch - 112)
            {
                return Err(error("request write points at wrong bytes"));
            }
            checked(record, elf, site.address, 77)?;
            exact_length(record, elf, start, site.address, scratch - 112, 8, 77)?;
        }
        let mut transport = Vec::new();
        for (helper, code) in [("__ckb_pipe", 75), ("__ckb_spawn_with_fd1", 76), ("__ckb_close", 78), ("__ckb_wait", 79)] {
            let calls = exact_runtime_calls(record, elf, helper, start, end)?;
            let [address] = calls.as_slice() else {
                return Err(error("missing or duplicated Spawn/IPC operation"));
            };
            checked(record, elf, *address, code)?;
            transport.push(*address);
            let registers = commitment_registers_before(elf, start, *address);
            if helper == "__ckb_spawn_with_fd1" {
                let dependency = call.operands[2].local.ok_or_else(|| error("missing dependency local"))?;
                let local =
                    entry.locals.iter().find(|local| local.id == dependency).ok_or_else(|| error("missing dependency binding"))?;
                let slot = local_stack_offset(u32::try_from(local.source_id).map_err(|_| error("dependency local overflow"))?)?;
                let before_index = commitment_registers_before(elf, start, address - 16);
                if before_index[10] != (CommitmentMachineValue::StackLoadedAddress { stack_slot: i64::from(slot), displacement: 0 })
                    || !is_slli(bounded_word(elf, address - 16)?, 10, 10, 32)
                    || !is_srli(bounded_word(elf, address - 12)?, 10, 10, 32)
                    || !is_ld(bounded_word(elf, address - 8)?, 11, 2, (scratch - 136) as i32)
                {
                    return Err(error("spawn dependency differs from exact handle preflight"));
                }
            }
            let expected_slot = match helper {
                "__ckb_close" => Some((10, scratch - 128)),
                "__ckb_wait" => Some((10, scratch - 120)),
                "__ckb_spawn_with_fd1" => Some((11, scratch - 136)),
                _ => None,
            };
            if expected_slot.is_some_and(|(register, slot)| {
                registers[register] != (CommitmentMachineValue::StackLoadedAddress { stack_slot: slot, displacement: 0 })
            }) {
                return Err(error("descriptor or PID ownership changed"));
            }
        }
        if !(transport[0] < transport[1]
            && transport[1] < writes[0].address
            && writes.last().unwrap().address < transport[2]
            && transport[2] < transport[3])
        {
            return Err(error("Spawn/write/close/wait order changed"));
        }
        if !is_sd(bounded_word(elf, transport[0] + 12)?, 11, 2, (scratch - 136) as i32)
            || !is_sd(bounded_word(elf, transport[0] + 16)?, 12, 2, (scratch - 128) as i32)
            || !is_sd(bounded_word(elf, transport[1] + 12)?, 11, 2, (scratch - 120) as i32)
        {
            return Err(error("pipe descriptors or spawned PID are not preserved"));
        }
        let loads: Vec<_> =
            syscall_sites.iter().filter(|site| matches!(constant(site, 17), Some(2061 | 2062 | 2081 | 2083))).collect();
        let comparisons = exact_runtime_calls(record, elf, "__cellscript_memcmp_fixed", start, end)?;
        if comparisons.len() != 2 {
            return Err(error("missing Type-group identity comparisons"));
        }
        for address in comparisons {
            let registers = commitment_registers_before(elf, start, address);
            if registers[10] != CommitmentMachineValue::StackAddress(scratch + 300)
                || registers[11] != CommitmentMachineValue::StackAddress(scratch + 464)
                || registers[12] != CommitmentMachineValue::Constant(32)
            {
                return Err(error("wrong Type-group identity comparison"));
            }
            checked(record, elf, address, 74)?;
        }
        let expected_loads = [
            (2062, 300, 32, 0, 0, 0),
            (2081, 332, 32, 0, 0x0100000000000001, 1),
            (2081, 464, 32, 0, 0x0100000000000001, 5),
            (2081, 464, 32, 1, 0x0100000000000001, 1),
            (2081, 364, 32, 0, 0x0100000000000002, 1),
            (2081, 464, 32, 0, 0x0100000000000002, 5),
            (2081, 464, 32, 1, 0x0100000000000002, 1),
            (2083, 396, 36, 0, 0x0100000000000001, 0),
            (2061, 432, 32, 0, 0, 0),
        ];
        if loads.len() != expected_loads.len() {
            return Err(error("transaction context load count changed"));
        }
        for (site, (number, offset, width, index, source, field)) in loads.iter().zip(expected_loads) {
            let registers = commitment_registers_before(elf, start, site.address);
            if constant(site, 17) != Some(number)
                || registers[10] != CommitmentMachineValue::StackAddress(scratch + offset)
                || registers[11] != CommitmentMachineValue::StackAddress(scratch - 112)
                || constant(site, 12) != Some(0)
                || constant(site, 13) != Some(index)
                || constant(site, 14) != Some(source)
                || constant(site, 15) != Some(field)
            {
                return Err(error("transaction field, source, index or statement offset changed"));
            }
            if index == 1 {
                let store = has_stack_store(elf, site.address.saturating_sub(80), site.address, 5, (scratch - 112) as i32)
                    .ok_or_else(|| error("missing group probe bound"))?;
                let owner = block_for_address(record, store).ok_or_else(|| error("group probe store outside block"))?;
                if register_constant_before(elf, owner, store, 5) != Some(32) {
                    return Err(error("group probe bound changed"));
                }
                if !is_addi(bounded_word(elf, site.address + 4)?, 10, 10, -1) {
                    return Err(error("group bound does not require end-of-source"));
                }
                checked(record, elf, site.address + 4, 74)?;
            } else {
                checked(record, elf, site.address, 74)?;
                exact_length(record, elf, start, site.address, scratch - 112, width, 74)?;
            }
        }
        // Every conditional edge is one of the mandatory checks above. Each
        // has exactly one fatal fallthrough jump, and the only other transfers
        // are the seven independently bound runtime calls. Extra jumps cannot
        // bypass the checked result or skip any request word.
        let instructions: Vec<_> =
            elf.instructions.iter().filter(|instruction| start <= instruction.address && instruction.address < end).collect();
        let branches = instructions.iter().filter(|instruction| instruction.word & 0x7f == 0x63).count();
        let transfers = instructions.iter().filter(|instruction| matches!(instruction.word & 0x7f, 0x6f | 0x67)).count();
        if branches != 139
            || transfers != 146
            || elf
                .control_flow
                .iter()
                .any(|edge| start < edge.target && edge.target < end && !(start <= edge.address && edge.address < end))
        {
            return Err(error("unaccounted control flow can bypass verifier checks"));
        }
    }
    Ok(())
}
