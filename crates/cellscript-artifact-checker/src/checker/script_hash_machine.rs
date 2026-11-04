//! Finite canonical Script-hash machine admission.
use super::*;

pub(super) fn validate_script_hash_machine_contract(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let call_count = record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .filter(|operation| operation.call.as_ref().is_some_and(|call| call.target == "__ckb_script_hash"))
        .count();
    if call_count == 0 {
        return Ok(());
    }

    let entry = record
        .entries
        .iter()
        .find(|entry| entry.id == "runtime:__ckb_script_hash")
        .ok_or_else(|| script_hash_machine_error("missing __ckb_script_hash runtime entry"))?;
    let entry_block = record
        .blocks
        .iter()
        .find(|block| block.id == entry.entry_block)
        .ok_or_else(|| script_hash_machine_error("missing __ckb_script_hash entry block"))?;
    let blake_entry = record
        .entries
        .iter()
        .find(|entry| entry.id == "runtime:__ckb_hash_blake2b_var")
        .ok_or_else(|| script_hash_machine_error("missing bounded Blake2b runtime entry"))?;
    let blake_block = record
        .blocks
        .iter()
        .find(|block| block.id == blake_entry.entry_block)
        .ok_or_else(|| script_hash_machine_error("missing bounded Blake2b entry block"))?;
    if entry.frame_size_bytes != 560
        || entry.outgoing_argument_bytes != 0
        || entry_block.machine_label.as_deref() != Some("__ckb_script_hash")
        || entry_block.frame_size_bytes != 560
        || record.blocks.iter().filter(|block| block.owner_entry == entry.id).any(|block| block.frame_size_bytes != 560)
    {
        return Err(script_hash_machine_error("runtime entry no longer declares the bounded 512-byte preimage frame"));
    }

    let base = entry_block.range.start;
    let word = |delta: u64| -> Result<u32, CheckerError> {
        let address = base.checked_add(delta).ok_or_else(|| script_hash_machine_error("instruction address overflow"))?;
        elf.instructions
            .iter()
            .find(|instruction| instruction.address == address)
            .map(|instruction| instruction.word)
            .ok_or_else(|| script_hash_machine_error(format!("missing instruction at {address:#x}")))
    };
    let flow_targets = |delta: u64, target: u64| {
        base.checked_add(delta)
            .is_some_and(|address| elf.control_flow.iter().any(|edge| edge.address == address && edge.target == target))
    };
    let invalid = base + 300;
    let valid_hash_type = base + 100;
    if !is_addi(word(0)?, 2, 2, -560)
        || !is_sd(word(4)?, 1, 2, 552)
        || !is_sd(word(8)?, 10, 2, 512)
        || !is_sd(word(12)?, 11, 2, 520)
        || !is_sd(word(16)?, 12, 2, 528)
        || !is_sd(word(20)?, 13, 2, 536)
        || !is_sd(word(24)?, 14, 2, 544)
        || !is_beq(word(28)?, 10, 0)
        || !flow_targets(28, invalid)
        || !is_beq(word(32)?, 14, 0)
        || !flow_targets(32, invalid)
        || !is_addi(word(36)?, 5, 0, 460)
        || !is_sltu(word(40)?, 6, 13, 5)
        || !is_beq(word(44)?, 6, 0)
        || !flow_targets(44, invalid)
        || !is_beq(word(48)?, 13, 0)
        || !flow_targets(48, base + 56)
        || !is_beq(word(52)?, 12, 0)
        || !flow_targets(52, invalid)
    {
        return Err(script_hash_machine_error("pointer, output, or 459-byte Script args bound changed"));
    }
    if !is_beq(word(56)?, 11, 0)
        || !flow_targets(56, valid_hash_type)
        || !is_addi(word(60)?, 5, 0, 1)
        || !is_sub(word(64)?, 6, 11, 5)
        || !is_beq(word(68)?, 6, 0)
        || !flow_targets(68, valid_hash_type)
        || !is_addi(word(72)?, 5, 0, 2)
        || !is_sub(word(76)?, 6, 11, 5)
        || !is_beq(word(80)?, 6, 0)
        || !flow_targets(80, valid_hash_type)
        || !is_addi(word(84)?, 5, 0, 4)
        || !is_sub(word(88)?, 6, 11, 5)
        || !is_beq(word(92)?, 6, 0)
        || !flow_targets(92, valid_hash_type)
        || !is_jal_zero(word(96)?)
        || !flow_targets(96, invalid)
    {
        return Err(script_hash_machine_error("admitted CKB Script hash_type set changed"));
    }
    if !is_ld(word(100)?, 5, 2, 536)
        || !is_addi(word(104)?, 5, 5, 53)
        || !is_sw(word(108)?, 5, 2, 0)
        || !is_addi(word(112)?, 5, 0, 16)
        || !is_sw(word(116)?, 5, 2, 4)
        || !is_addi(word(120)?, 5, 0, 48)
        || !is_sw(word(124)?, 5, 2, 8)
        || !is_addi(word(128)?, 5, 0, 49)
        || !is_sw(word(132)?, 5, 2, 12)
    {
        return Err(script_hash_machine_error("canonical Molecule Script total size or table offsets changed"));
    }
    if !is_ld(word(136)?, 7, 2, 512)
        || !is_addi(word(140)?, 5, 0, 0)
        || !is_addi(word(144)?, 6, 0, 32)
        || !is_sltu(word(148)?, 6, 5, 6)
        || !is_beq(word(152)?, 6, 0)
        || !flow_targets(152, base + 184)
        || !is_add(word(156)?, 28, 7, 5)
        || !is_lbu(word(160)?, 29, 28, 0)
        || !is_addi(word(164)?, 28, 2, 16)
        || !is_add(word(168)?, 28, 28, 5)
        || !is_sb(word(172)?, 29, 28, 0)
        || !is_addi(word(176)?, 5, 5, 1)
        || !is_jal_zero(word(180)?)
        || !flow_targets(180, base + 144)
    {
        return Err(script_hash_machine_error("32-byte code_hash serialization changed"));
    }
    if !is_ld(word(184)?, 5, 2, 520)
        || !is_sb(word(188)?, 5, 2, 48)
        || !is_ld(word(192)?, 5, 2, 536)
        || !is_sb(word(196)?, 5, 2, 49)
        || !is_srli(word(200)?, 5, 5, 8)
        || !is_sb(word(204)?, 5, 2, 50)
        || !is_srli(word(208)?, 5, 5, 8)
        || !is_sb(word(212)?, 5, 2, 51)
        || !is_srli(word(216)?, 5, 5, 8)
        || !is_sb(word(220)?, 5, 2, 52)
    {
        return Err(script_hash_machine_error("hash_type or little-endian Bytes length serialization changed"));
    }
    if !is_ld(word(224)?, 7, 2, 528)
        || !is_ld(word(228)?, 30, 2, 536)
        || !is_addi(word(232)?, 5, 0, 0)
        || !is_sltu(word(236)?, 6, 5, 30)
        || !is_beq(word(240)?, 6, 0)
        || !flow_targets(240, base + 272)
        || !is_add(word(244)?, 28, 7, 5)
        || !is_lbu(word(248)?, 29, 28, 0)
        || !is_addi(word(252)?, 28, 2, 53)
        || !is_add(word(256)?, 28, 28, 5)
        || !is_sb(word(260)?, 29, 28, 0)
        || !is_addi(word(264)?, 5, 5, 1)
        || !is_jal_zero(word(268)?)
        || !flow_targets(268, base + 236)
    {
        return Err(script_hash_machine_error("bounded Script args serialization changed"));
    }
    if !is_addi(word(272)?, 10, 2, 0)
        || !is_ld(word(276)?, 11, 2, 536)
        || !is_addi(word(280)?, 11, 11, 53)
        || !is_ld(word(284)?, 12, 2, 544)
        || !is_auipc(word(288)?, 1)
        || !is_jalr_call(word(292)?)
        || !flow_targets(292, blake_block.range.start)
        || !is_jal_zero(word(296)?)
        || !flow_targets(296, base + 304)
        || !is_addi(word(300)?, 10, 0, 72)
        || !is_ld(word(304)?, 1, 2, 552)
        || !is_addi(word(308)?, 2, 2, 560)
        || word(312)? != 0x0000_8067
    {
        return Err(script_hash_machine_error("Blake2b delegation, error 72, or bounded-frame return changed"));
    }
    Ok(())
}

pub(super) fn script_hash_machine_error(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2417SyscallContractInvalid,
        format!("canonical Script hash machine contract: {}", message.into()),
    )
}
