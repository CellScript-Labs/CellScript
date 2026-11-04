//! ELF bindings, control flow, stack discipline and syscall sites.
use super::*;

pub(super) fn validate_elf_binding(artifact: &[u8], record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    if !elf.text.range().contains_range(record.text_range) {
        return Err(CheckerError::new(CheckerRejectionCode::V2412ElfSectionInvalid, "record text range is outside ELF .text"));
    }
    if record.artifact_size_bytes != artifact.len() as u64 || record.text_range.start < elf.entry {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2409ArtifactIdentityMismatch,
            "record artifact size/text identity disagrees with ELF",
        ));
    }
    validate_startup(record, elf)?;
    Ok(())
}

/// The producer excludes its five-word bootstrap from lowering blocks. Bind
/// that entire prefix independently, including the VM's real entry and exit.
pub(super) fn validate_startup(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let invalid = || {
        CheckerError::new(
            CheckerRejectionCode::V2414ControlFlowInvalid,
            "ELF startup does not enter the checked entry and exit profile",
        )
    };
    if elf.entry != elf.text.address
        || record.text_range.start != elf.entry.checked_add(20).ok_or_else(invalid)?
        || record.text_range.end != elf.text.range().end
    {
        return Err(invalid());
    }
    let words = elf.instructions.get(..5).ok_or_else(invalid)?;
    if words[2].word != 0x000008b7 || words[3].word != 0x05d88893 || words[4].word != 0x00000073 {
        return Err(invalid());
    }
    // Explicit-parameter ABI fallback is a terminal rejection, never success.
    if words[0].word == 0x00000537 && words[1].word == 0x01950513 {
        return Ok(());
    }
    let contract = &record.typed_semantics.foundation.entry_contract;
    let selected = record
        .entries
        .iter()
        .find(|entry| entry.name == "_cellscript_entry")
        .or_else(|| record.entries.iter().find(|entry| entry.id == contract.exact_entry));
    if !is_auipc(words[0].word, 1) || !is_jalr_call(words[1].word) {
        return Err(invalid());
    }
    match selected {
        Some(selected) => {
            let target = entry_start(record, &selected.id)?;
            if !flow_targets(elf, elf.entry + 4, target) {
                return Err(invalid());
            }
        }
        None => {
            // Helper-only modules have no action or lock entry, so the record
            // declares exact_entry "none" and the trampoline calls the first
            // emitted global function. That call must still land on a checked
            // block boundary instead of unchecked bytes.
            if contract.exact_entry != "none" {
                return Err(invalid());
            }
            let Some(flow) = elf.control_flow.iter().find(|flow| flow.address == elf.entry + 4) else {
                return Err(invalid());
            };
            if !record.blocks.iter().any(|block| block.range.start == flow.target) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

pub(super) fn validate_block_digests(artifact: &[u8], record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    for block in &record.blocks {
        let bytes = elf.bytes_for_range(artifact, block.range).map_err(map_elf_error)?;
        let digest = domain_hash_bytes("cellscript-machine-block-v1", bytes);
        if digest != block.byte_digest {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2415BlockDigestMismatch,
                format!("machine bytes for block '{}' do not match its digest", block.id),
            ));
        }
    }
    Ok(())
}

/// Frame-slot pairing for the return address, scoped to the entry (function)
/// that owns each lowering block. Within one entry's blocks — laid out in
/// address order — every `ld ra, off(sp)` must have an earlier `sd ra,
/// off(sp)`, and the slot's last writer before the load must be that `sd ra`:
/// any overlapping store poisons it unless it saves the full return address
/// again. This includes narrow stores and stores starting before the slot.
/// This check models offsets through the same sp/fp base, not arbitrary
/// register aliases or complete control-flow dataflow. Cross-function leakage cannot
/// pair: each entry's slot map starts empty, and layout-interleaved callee
/// bodies belong to their own entries.
pub(super) fn validate_ra_frame_slots(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let reject = |message: String| CheckerError::new(CheckerRejectionCode::V2414ControlFlowInvalid, message);
    // Group blocks by owning entry, keeping each function's address order.
    let mut by_entry: BTreeMap<&str, Vec<&LoweringBlock>> = BTreeMap::new();
    for block in &record.blocks {
        by_entry.entry(block.owner_entry.as_str()).or_default().push(block);
    }
    let instruction_at = |address: u64| -> Option<&crate::elf::DecodedInstruction> {
        elf.instructions.binary_search_by_key(&address, |instruction| instruction.address).ok().map(|index| &elf.instructions[index])
    };
    for (entry, blocks) in by_entry {
        // Slot maps are keyed per base register: CellScript machines mix
        // sp-relative saves with fp-relative restores inside one frame
        // (`addi fp, sp, K`), and cross-base aliasing cannot pair statically
        // anyway, so pairing requires the same base and the same offset.
        let mut slots: BTreeMap<u32, BTreeMap<i64, bool>> = BTreeMap::new();
        for block in blocks {
            let mut address = block.range.start;
            while address < block.range.end {
                let Some(instruction) = instruction_at(address) else {
                    break;
                };
                let word = instruction.word;
                let opcode = word & 0x7f;
                let funct3 = (word >> 12) & 0x7;
                let rs1 = (word >> 15) & 0x1f;
                let rs2 = (word >> 20) & 0x1f;
                let rd = (word >> 7) & 0x1f;
                // sb/sh/sw/sd can all overlap the saved eight-byte address.
                if opcode == 0x23 && funct3 <= 3 && matches!(rs1, 2 | 8) {
                    let offset = sign_extend_12((((word >> 25) & 0x7f) << 5) | ((word >> 7) & 0x1f));
                    record_ra_frame_store(slots.entry(rs1).or_default(), offset, 1 << funct3, rs2 == 1);
                }
                // ld ra, off(base)
                if opcode == 0x03 && funct3 == 3 && matches!(rs1, 2 | 8) && rd == 1 {
                    let offset = sign_extend_12(word >> 20);
                    match slots.get(&rs1).and_then(|base| base.get(&offset)) {
                        Some(true) => {}
                        Some(false) => {
                            return Err(reject(format!(
                                "ld ra at {address:#x} in '{entry}' restores frame slot {offset} overwritten without a full return-address save"
                            )));
                        }
                        None => {
                            return Err(reject(format!(
                                "ld ra at {address:#x} in '{entry}' restores frame slot {offset} that the function never saved"
                            )));
                        }
                    }
                }
                address += 4;
            }
        }
    }
    Ok(())
}

/// Offsets are decoded signed 12-bit immediates and widths are 1/2/4/8 bytes.
pub(super) fn record_ra_frame_store(slots: &mut BTreeMap<i64, bool>, offset: i64, width: i64, stores_ra: bool) {
    // [saved, saved + 8) overlaps [offset, offset + width) exactly when
    // offset - 7 <= saved < offset + width. Adjacent writes remain harmless.
    for (_, valid) in slots.range_mut((offset - 7)..(offset + width)) {
        *valid = false;
    }
    if width == 8 && stores_ra {
        slots.insert(offset, true);
    }
}

pub(super) fn sign_extend_12(value: u32) -> i64 {
    (i64::from(value) << 52) >> 52
}

pub(super) fn validate_control_flow(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let blocks = record.blocks.iter().map(|block| (block.id.as_str(), block)).collect::<BTreeMap<_, _>>();
    let find_block = |address| record.blocks.iter().find(|block| block.range.contains(address));
    for edge in record.edges.iter().filter(|edge| matches!(edge.kind, EdgeKind::Fallthrough | EdgeKind::ConditionalFallthrough)) {
        if blocks[edge.from.as_str()].range.end != blocks[edge.to.as_str()].range.start {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2414ControlFlowInvalid,
                format!("declared fallthrough '{} -> {}' differs from physical instruction order", edge.from, edge.to),
            ));
        }
    }
    for flow in elf.control_flow.iter().filter(|flow| record.text_range.contains(flow.address)) {
        let Some(from) = find_block(flow.address) else {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2414ControlFlowInvalid,
                format!("instruction at {:#x} is not covered by a lowering block", flow.address),
            ));
        };
        let Some(to) = find_block(flow.target) else {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2414ControlFlowInvalid,
                format!("target {:#x} is outside lowering blocks", flow.target),
            ));
        };
        // Mid-block branch targets are deliberate machine-binding
        // territory: the typed machine binding proofs (V2420) reject jumps
        // into the middle of guarded sequences, so the CFG layer must not
        // preempt that contract with a different rejection code.
        let allowed_kinds: &[EdgeKind] = match flow.kind {
            DecodedControlFlowKind::ConditionalBranch => {
                &[EdgeKind::ConditionalTaken, EdgeKind::ConditionalFallthrough, EdgeKind::Fallthrough]
            }
            DecodedControlFlowKind::DirectJump => &[EdgeKind::Jump, EdgeKind::Call, EdgeKind::ConditionalTaken],
        };
        let edge_exists = from.id == to.id
            || record.edges.iter().any(|edge| edge.from == from.id && edge.to == to.id && allowed_kinds.contains(&edge.kind));
        if !edge_exists || !blocks.contains_key(to.id.as_str()) {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2414ControlFlowInvalid,
                format!("decoded flow '{} -> {}' is absent from the lowering CFG", from.id, to.id),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_machine_terminators(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let instructions = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    for block in &record.blocks {
        let address = block.range.end.checked_sub(4).ok_or_else(|| {
            CheckerError::new(
                CheckerRejectionCode::V2414ControlFlowInvalid,
                format!("block '{}' is too short for a terminator", block.id),
            )
        })?;
        let word = instructions.get(&address).copied().ok_or_else(|| {
            CheckerError::new(
                CheckerRejectionCode::V2414ControlFlowInvalid,
                format!("block '{}' end does not address a decoded instruction", block.id),
            )
        })?;
        let opcode = word & 0x7f;
        let rd = (word >> 7) & 0x1f;
        let valid = match block.terminator {
            MachineTerminator::Return => word == 0x0000_8067,
            MachineTerminator::Jump => opcode == 0x6f && rd == 0,
            MachineTerminator::ConditionalBranch => opcode == 0x63 || (opcode == 0x6f && rd == 0),
            MachineTerminator::Fallthrough => word != 0x0000_8067 && !matches!(opcode, 0x63 | 0x6f),
        };
        if !valid {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2414ControlFlowInvalid,
                format!("decoded final instruction of block '{}' disagrees with its terminator", block.id),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_stack_discipline(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    terminal_sink: Option<&str>,
) -> Result<(), CheckerError> {
    let blocks = record.blocks.iter().map(|block| (block.id.as_str(), block)).collect::<BTreeMap<_, _>>();
    let mut outgoing = BTreeMap::<&str, Vec<&LoweringEdge>>::new();
    for edge in &record.edges {
        outgoing.entry(edge.from.as_str()).or_default().push(edge);
    }
    let mut entry_delta = BTreeMap::<&str, i64>::new();
    let mut pending = record.entries.iter().map(|entry| (entry.entry_block.as_str(), 0_i64)).collect::<Vec<_>>();
    while let Some((block_id, incoming_delta)) = pending.pop() {
        // This exception is granted only after decoding the complete, memory-free,
        // non-returning EXIT sink. Arbitrary named runtime helpers get no exemption.
        if terminal_sink == Some(block_id) {
            continue;
        }
        if let Some(previous) = entry_delta.insert(block_id, incoming_delta) {
            if previous != incoming_delta {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2407AbiOrStackInvalid,
                    format!("block '{block_id}' has inconsistent incoming stack-pointer deltas {previous} and {incoming_delta}"),
                ));
            }
            continue;
        }
        let block = blocks[block_id];
        let mut delta = incoming_delta;
        for adjustment in elf.stack_adjustments.iter().filter(|adjustment| block.range.contains(adjustment.address)) {
            if adjustment.delta % 16 != 0 {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2407AbiOrStackInvalid,
                    format!("stack adjustment {} at {:#x} violates 16-byte alignment", adjustment.delta, adjustment.address),
                ));
            }
            delta = delta.checked_add(adjustment.delta).ok_or_else(|| {
                CheckerError::new(
                    CheckerRejectionCode::V2407AbiOrStackInvalid,
                    format!("stack-pointer delta overflows in block '{block_id}'"),
                )
            })?;
            if delta > 0 || delta.unsigned_abs() > u64::from(block.frame_size_bytes) {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2407AbiOrStackInvalid,
                    format!("stack-pointer delta {delta} in block '{block_id}' exceeds declared frame {}", block.frame_size_bytes),
                ));
            }
        }
        if block.terminator == MachineTerminator::Return && delta != 0 {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2407AbiOrStackInvalid,
                format!("return block '{block_id}' leaves stack-pointer delta {delta}"),
            ));
        }
        for edge in outgoing.get(block_id).into_iter().flatten() {
            pending.push((edge.to.as_str(), if edge.kind == EdgeKind::Call { 0 } else { delta }));
        }
    }
    Ok(())
}

pub(super) fn validate_syscalls(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let actual = elf.syscall_addresses.iter().copied().filter(|address| record.text_range.contains(*address)).collect::<Vec<_>>();
    let declared = record.syscall_sites.iter().map(|site| site.address).collect::<Vec<_>>();
    if actual != declared {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2417SyscallContractInvalid,
            "declared syscall sites do not exactly match decoded ecall instructions",
        ));
    }
    let blocks = record.blocks.iter().map(|block| (block.id.as_str(), block)).collect::<BTreeMap<_, _>>();
    for site in &record.syscall_sites {
        let block = blocks.get(site.block_id.as_str()).copied();
        if site.contract.is_empty()
            || site.source_domain.is_empty()
            || site.index_domain.is_empty()
            || site.buffer_limit_bytes == 0
            || !site.return_code_checked
            || block.is_none_or(|block| !block.range.contains(site.address))
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2417SyscallContractInvalid,
                format!("syscall site at {:#x} has an invalid bounded contract", site.address),
            ));
        }
        validate_header_dep_syscall_site(record, elf, site, block.expect("validated syscall block"))?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) struct HeaderDepSyscallContract {
    pub(super) name: &'static str,
    pub(super) syscall_number: u64,
    pub(super) field_id: Option<i32>,
    pub(super) buffer_bytes: u32,
    pub(super) result_stack_offset: Option<i32>,
}

pub(super) fn header_dep_syscall_contract(owner_entry: &str) -> Option<HeaderDepSyscallContract> {
    match owner_entry {
        "runtime:__ckb_header_dep_epoch_number" => Some(HeaderDepSyscallContract {
            name: "ckb-header-dep-epoch-number-v1",
            syscall_number: 2082,
            field_id: Some(0),
            buffer_bytes: 8,
            result_stack_offset: None,
        }),
        "runtime:__ckb_header_dep_epoch_start_block_number" => Some(HeaderDepSyscallContract {
            name: "ckb-header-dep-epoch-start-block-number-v1",
            syscall_number: 2082,
            field_id: Some(1),
            buffer_bytes: 8,
            result_stack_offset: None,
        }),
        "runtime:__ckb_header_dep_epoch_length" => Some(HeaderDepSyscallContract {
            name: "ckb-header-dep-epoch-length-v1",
            syscall_number: 2082,
            field_id: Some(2),
            buffer_bytes: 8,
            result_stack_offset: None,
        }),
        "runtime:__ckb_header_dep_block_number" => Some(HeaderDepSyscallContract {
            name: "ckb-header-dep-block-number-v1",
            syscall_number: 2072,
            field_id: None,
            buffer_bytes: 208,
            result_stack_offset: Some(32),
        }),
        "runtime:__ckb_header_dep_timestamp_millis" => Some(HeaderDepSyscallContract {
            name: "ckb-header-dep-timestamp-millis-v1",
            syscall_number: 2072,
            field_id: None,
            buffer_bytes: 208,
            result_stack_offset: Some(24),
        }),
        _ => None,
    }
}

pub(super) fn validate_header_dep_syscall_site(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    site: &SyscallSite,
    block: &LoweringBlock,
) -> Result<(), CheckerError> {
    let Some(expected) = header_dep_syscall_contract(&block.owner_entry) else {
        if site.contract.starts_with("ckb-header-dep-") {
            return Err(header_dep_machine_error(site, "a HeaderDep syscall contract is attached to the wrong runtime helper"));
        }
        return Ok(());
    };
    if site.syscall_number != Some(expected.syscall_number)
        || site.contract != expected.name
        || site.source_domain != "HeaderDepView/source=HeaderDep"
        || site.index_domain != "u32-source-view"
        || !site.return_code_checked
        || site.buffer_limit_bytes != expected.buffer_bytes
    {
        return Err(header_dep_machine_error(site, "the declared HeaderDep syscall contract differs from its helper"));
    }

    let word = |delta: i64| -> Result<u32, CheckerError> {
        let address =
            site.address.checked_add_signed(delta).ok_or_else(|| header_dep_machine_error(site, "instruction address overflow"))?;
        elf.instructions
            .iter()
            .find(|instruction| instruction.address == address)
            .map(|instruction| instruction.word)
            .ok_or_else(|| header_dep_machine_error(site, format!("missing instruction at {address:#x}")))
    };
    let syscall_lower = i32::try_from(expected.syscall_number).expect("CKB syscall fits i32") - 4096;
    if word(0)? != 0x0000_0073 || !is_lui(word(-8)?, 17, 0x1000) || !is_addi(word(-4)?, 17, 17, syscall_lower) {
        return Err(header_dep_machine_error(site, "the machine code does not materialize the declared HeaderDep syscall number"));
    }
    if !is_beq(word(4)?, 10, 0) || !is_addi(word(8)?, 10, 0, 45) || !machine_error_jumps_to_abort(record, elf, site.address + 8, 45) {
        return Err(header_dep_machine_error(site, "the HeaderDep syscall status does not terminate with error 45"));
    }

    if let Some(field_id) = expected.field_id {
        if !is_addi(word(-48)?, 5, 0, 4)
            || !is_bne(word(-44)?, 7, 5)
            || !is_addi(word(-40)?, 5, 0, 8)
            || !is_sd(word(-36)?, 5, 2, 8)
            || !is_addi(word(-20)?, 13, 6, 0)
            || !is_addi(word(-16)?, 14, 7, 0)
            || !is_addi(word(-12)?, 15, 0, field_id)
        {
            return Err(header_dep_machine_error(
                site,
                "the HeaderDep field selector, source, index, or 8-byte buffer contract changed",
            ));
        }
        if !is_ld(word(16)?, 10, 2, 8)
            || !is_addi(word(20)?, 11, 0, 8)
            || !is_sub(word(24)?, 10, 10, 11)
            || !is_beq(word(28)?, 10, 0)
            || !machine_error_jumps_to_abort(record, elf, site.address + 32, 4)
        {
            return Err(header_dep_machine_error(site, "the HeaderDep scalar exact-width check does not terminate with error 4"));
        }
    } else {
        if !is_addi(word(-44)?, 5, 0, 4)
            || !is_bne(word(-40)?, 7, 5)
            || !is_addi(word(-36)?, 5, 0, 208)
            || !is_sd(word(-32)?, 5, 2, 0)
            || !is_addi(word(-16)?, 13, 6, 0)
            || !is_addi(word(-12)?, 14, 7, 0)
        {
            return Err(header_dep_machine_error(site, "the full HeaderDep source, index, or 208-byte buffer contract changed"));
        }
        let result_stack_offset = expected.result_stack_offset.expect("full HeaderDep field offset");
        if !is_ld(word(16)?, 5, 2, 0)
            || !is_addi(word(20)?, 6, 0, 208)
            || !is_bne(word(24)?, 5, 6)
            || !is_ld(word(28)?, 10, 2, result_stack_offset)
            || !machine_error_jumps_to_abort(record, elf, site.address + 40, 4)
        {
            return Err(header_dep_machine_error(site, "the full HeaderDep exact-width or RawHeader field-offset contract changed"));
        }
    }

    if !owner_has_abort_error(record, elf, &block.owner_entry, 44) {
        return Err(header_dep_machine_error(site, "an invalid HeaderDep source does not terminate with error 44"));
    }
    Ok(())
}

pub(super) fn header_dep_machine_error(site: &SyscallSite, message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2417SyscallContractInvalid,
        format!("HeaderDep syscall site at {:#x}: {}", site.address, message.into()),
    )
}

pub(super) fn machine_error_jumps_to_abort(record: &VerifiedLoweringRecord, elf: &ParsedElf, address: u64, code: i32) -> bool {
    let Some(first) = elf.instructions.iter().find(|instruction| instruction.address == address) else {
        return false;
    };
    let Some(second) = elf.instructions.iter().find(|instruction| instruction.address == address + 4) else {
        return false;
    };
    let Some(abort_entry) = record.entries.iter().find(|entry| entry.id == "runtime:__cellscript_abort") else {
        return false;
    };
    let Some(abort_block) = record.blocks.iter().find(|block| block.id == abort_entry.entry_block) else {
        return false;
    };
    is_addi(first.word, 10, 0, code)
        && is_jal_zero(second.word)
        && elf.control_flow.iter().any(|edge| edge.address == second.address && edge.target == abort_block.range.start)
}

pub(super) fn owner_has_abort_error(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner_entry: &str, code: i32) -> bool {
    record.blocks.iter().filter(|block| block.owner_entry == owner_entry).any(|block| {
        elf.instructions
            .iter()
            .filter(|instruction| block.range.contains(instruction.address))
            .any(|instruction| machine_error_jumps_to_abort(record, elf, instruction.address, code))
    })
}
