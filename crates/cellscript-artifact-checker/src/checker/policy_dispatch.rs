//! Policy-witness dispatch, bounded entry adapters and their machine contracts.
use super::*;

pub(super) const POLICY_WRAPPER_ENTRY_ID: &str = "wrapper:_cellscript_entry";
pub(super) const POLICY_ENTRY_FRAME_BYTES: u32 = 4_192;
pub(super) const POLICY_ARGS_POINTER_OFFSET: i32 = 4_144;
pub(super) const POLICY_ARGS_LENGTH_OFFSET: i32 = 4_152;
pub(super) const POLICY_TAG_OFFSET: i32 = 4_160;
pub(super) const POLICY_FOUND_OFFSET: i32 = 4_168;
pub(super) const POLICY_RA_OFFSET: i32 = 4_184;
pub(super) const POLICY_HASH_BUFFER_OFFSET: i32 = 4_112;
pub(super) const POLICY_RECORD_FIXED_BYTES: i32 = 61;
pub(super) const POLICY_TYPE_ROLE: i32 = 1;
pub(super) const POLICY_WITNESS_MAGIC: &[u8; 8] = b"CSPOLv1\0";
pub(super) const ENTRY_WITNESS_MAGIC: &[u8; 8] = b"CSARGv1\0";
pub(super) const CKB_LOAD_SCRIPT_HASH: u64 = 2_062;
pub(super) const CKB_LOAD_WITNESS: u64 = 2_074;
pub(super) const CKB_LOAD_CELL_BY_FIELD: u64 = 2_081;
pub(super) const CKB_LOAD_CELL_DATA: u64 = 2_092;
pub(super) const CKB_GROUP_FLAG: u64 = 0x0100_0000_0000_0000;
pub(super) const CKB_GROUP_INPUT: u64 = CKB_GROUP_FLAG | 1;
pub(super) const CKB_GROUP_OUTPUT: u64 = CKB_GROUP_FLAG | 2;

#[derive(Debug)]
pub(super) struct BoundedGroupInputMachineContract<'a> {
    pub(super) owner: &'a str,
    pub(super) maximum: u64,
    pub(super) element_width: u64,
}

#[derive(Debug)]
pub(super) struct BoundedOutputMachineContract {
    pub(super) owner: String,
    pub(super) maximum: u64,
    pub(super) element_width: u64,
    pub(super) output_width: u64,
    pub(super) capacity_floor: u64,
    pub(super) field_count: usize,
    pub(super) lock_plan_offset: u64,
}

pub(super) fn validate_bounded_output_plan_machine_contract(
    metadata: &Value,
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
) -> Result<(), CheckerError> {
    let typed_load_count = record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .filter(|operation| operation.opcode == "bounded-plan-load")
        .count();
    let contracts = bounded_output_contracts_from_metadata(metadata, record)?;
    if contracts.is_empty() && typed_load_count == 0 {
        return Ok(());
    }
    if contracts.len() != typed_load_count {
        return Err(bounded_output_machine_error(
            "typed bounded Plan loads and metadata contracts do not have one-to-one correspondence",
        ));
    }

    for owner in contracts.iter().map(|contract| contract.owner.as_str()).collect::<BTreeSet<_>>() {
        let owner_contracts = contracts.iter().filter(|contract| contract.owner == owner).collect::<Vec<_>>();
        let typed_entry = record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.id == owner)
            .ok_or_else(|| bounded_output_machine_error(format!("metadata owner '{owner}' has no typed entry")))?;
        let typed_loads = typed_entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .filter(|operation| operation.opcode == "bounded-plan-load")
            .collect::<Vec<_>>();
        let typed_verifies = typed_entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .filter(|operation| operation.opcode == "bounded-output-verify")
            .collect::<Vec<_>>();
        let typed_ends = typed_entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .filter(|operation| operation.opcode == "bounded-output-end")
            .collect::<Vec<_>>();
        if typed_loads.len() != owner_contracts.len()
            || typed_verifies.len() != owner_contracts.len()
            || typed_ends.len() != owner_contracts.len()
        {
            return Err(bounded_output_machine_error(format!(
                "entry '{owner}' must have one typed load, verify, and exact-count end for each bounded output contract"
            )));
        }

        let headers = generated_blocks(record, owner, ".Lbounded_plan_header_ok_");
        let magic = generated_blocks(record, owner, ".Lbounded_plan_magic_ok_");
        let counts = generated_blocks(record, owner, ".Lbounded_plan_count_ok_");
        let lengths = generated_blocks(record, owner, ".Lbounded_plan_length_ok_");
        let found = generated_blocks(record, owner, ".Lbounded_plan_element_found_");
        let done = generated_blocks(record, owner, ".Lbounded_plan_load_done_");
        let exact = generated_blocks(record, owner, ".Lbounded_output_count_exact_");
        let type_only = generated_blocks(record, owner, ".Lbounded_output_type_only_");
        let capacity_ok = generated_blocks(record, owner, ".Lbounded_output_capacity_ok_");
        let expected = owner_contracts.len();
        if headers.len() != expected
            || magic.len() != expected * 8
            || counts.len() != expected
            || lengths.len() != expected
            || found.len() != expected
            || done.len() != expected
            || exact.len() != expected
            || type_only.len() != expected
            || capacity_ok.len() != expected
        {
            return Err(bounded_output_machine_error(format!(
                "entry '{owner}' does not have one complete decoder and output verifier per typed contract"
            )));
        }
        for (index, contract) in owner_contracts.into_iter().enumerate() {
            validate_one_bounded_output_machine_contract(
                record,
                elf,
                contract,
                headers[index],
                &magic[index * 8..index * 8 + 8],
                counts[index],
                lengths[index],
                found[index],
                done[index],
                exact[index],
                type_only[index],
                capacity_ok[index],
            )?;
        }
    }
    Ok(())
}

pub(super) fn bounded_output_contracts_from_metadata(
    metadata: &Value,
    record: &VerifiedLoweringRecord,
) -> Result<Vec<BoundedOutputMachineContract>, CheckerError> {
    fn text<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
        value.get(field).and_then(Value::as_str)
    }
    fn number(value: &Value, field: &str) -> Option<u64> {
        value.get(field).and_then(Value::as_u64)
    }
    let Some(collections) =
        metadata.get("runtime").and_then(|runtime| runtime.get("collection_instantiations")).and_then(Value::as_array)
    else {
        return Ok(Vec::new());
    };
    let mut contracts = Vec::new();
    for collection in collections {
        let Some(contract) = collection.get("bounded_output_plan") else {
            continue;
        };
        if text(collection, "scope_kind") != Some("action")
            || text(collection, "status") != Some("checked-runtime")
            || text(collection, "source") != Some("witness")
            || text(collection, "ownership") != Some("bounded-output-plan")
            || text(collection, "evidence_tier") != Some("checked-runtime")
            || text(contract, "schema") != Some("cellscript-bounded-output-plan-contract")
            || number(contract, "version") != Some(1)
            || text(contract, "codec") != Some("molecule-fixvec-fixed-item-v1")
            || text(contract, "codec_magic_hex") != Some("0x435342504c763100")
            || text(contract, "placement") != Some("entry-witness-args-v1-param-payload")
            || contract.get("allow_zero").and_then(Value::as_bool) != Some(true)
            || text(contract, "output_source") != Some("ckb-group-output")
            || text(contract, "ordering") != Some("plan-index-equals-group-output-index")
            || text(contract, "correspondence") != Some("exactly-one-group-output-per-plan-element")
            || text(contract, "type_script_policy") != Some("exact-current-script-hash")
            || text(contract, "lock_policy") != Some("exact-plan-field-hash")
            || text(contract, "identity_policy") != Some("fresh-output-outpoint-plus-type-group-ordinal")
        {
            return Err(bounded_output_machine_error("metadata bounded output contract identity is invalid"));
        }
        let scope_name = text(collection, "scope_name")
            .ok_or_else(|| bounded_output_machine_error("bounded output metadata has no action name"))?;
        let element_type =
            text(contract, "element_ty").ok_or_else(|| bounded_output_machine_error("bounded output metadata has no Plan type"))?;
        let output_type =
            text(contract, "output_ty").ok_or_else(|| bounded_output_machine_error("bounded output metadata has no Resource type"))?;
        let maximum =
            number(contract, "max_elements").ok_or_else(|| bounded_output_machine_error("bounded output metadata has no maximum"))?;
        let element_width = number(contract, "element_width_bytes")
            .ok_or_else(|| bounded_output_machine_error("bounded output metadata has no Plan width"))?;
        let output_width = number(contract, "output_width_bytes")
            .ok_or_else(|| bounded_output_machine_error("bounded output metadata has no output width"))?;
        let capacity_floor = number(contract, "capacity_floor_shannons")
            .ok_or_else(|| bounded_output_machine_error("bounded output metadata has no capacity floor"))?;
        if !(1..=1024).contains(&maximum)
            || element_width == 0
            || maximum.checked_mul(element_width).and_then(|bytes| bytes.checked_add(12)).is_none_or(|bytes| bytes > 4084)
            || !(1..=512).contains(&output_width)
            || capacity_floor == 0
            || number(collection, "max_elements") != Some(maximum)
            || number(collection, "element_width_bytes") != Some(element_width)
            || number(collection, "output_cardinality_max") != Some(maximum)
        {
            return Err(bounded_output_machine_error("metadata bounded output resource bounds are invalid"));
        }
        let plan_layout = record
            .typed_semantics
            .types
            .iter()
            .find(|ty| ty.name == element_type && ty.kind == "struct" && ty.encoded_size == u32::try_from(element_width).ok())
            .ok_or_else(|| bounded_output_machine_error("metadata Plan layout does not match typed semantics"))?;
        let output_layout = record
            .typed_semantics
            .types
            .iter()
            .find(|ty| {
                ty.name == output_type
                    && ty.kind == "resource"
                    && ty.identity_policy == "none"
                    && ty.encoded_size == u32::try_from(output_width).ok()
            })
            .ok_or_else(|| bounded_output_machine_error("metadata Resource layout does not match typed semantics"))?;
        let field_bindings = contract
            .get("field_bindings")
            .and_then(Value::as_array)
            .filter(|bindings| bindings.len() == output_layout.fields.len() && !bindings.is_empty())
            .ok_or_else(|| bounded_output_machine_error("metadata field bindings are incomplete"))?;
        let mut seen = BTreeSet::new();
        let mut covered = vec![false; output_width as usize];
        for binding in field_bindings {
            let output_field_name = text(binding, "output_field")
                .ok_or_else(|| bounded_output_machine_error("metadata field binding has no output field"))?;
            let plan_field_name =
                text(binding, "plan_field").ok_or_else(|| bounded_output_machine_error("metadata field binding has no Plan field"))?;
            let output_offset = number(binding, "output_offset_bytes");
            let plan_offset = number(binding, "plan_offset_bytes");
            let width = number(binding, "width_bytes");
            let binding_type = text(binding, "ty");
            let output_field = output_layout.fields.iter().find(|field| field.name == output_field_name);
            let plan_field = plan_layout.fields.iter().find(|field| field.name == plan_field_name);
            if !seen.insert(output_field_name)
                || output_field.is_none_or(|field| {
                    Some(u64::from(field.offset)) != output_offset
                        || field.width_bytes.map(u64::from) != width
                        || binding_type.is_none_or(|ty| canonical_abi_type(ty) != canonical_abi_type(&field.ty))
                })
                || plan_field.is_none_or(|field| {
                    Some(u64::from(field.offset)) != plan_offset
                        || field.width_bytes.map(u64::from) != width
                        || binding_type.is_none_or(|ty| canonical_abi_type(ty) != canonical_abi_type(&field.ty))
                })
            {
                return Err(bounded_output_machine_error("metadata field binding does not match typed fixed layouts"));
            }
            let Some((start, end)) = output_offset.zip(width).and_then(|(offset, width)| {
                let start = usize::try_from(offset).ok()?;
                let end = usize::try_from(offset.checked_add(width)?).ok()?;
                (end <= covered.len()).then_some((start, end))
            }) else {
                return Err(bounded_output_machine_error("metadata field binding is outside output data"));
            };
            for byte in &mut covered[start..end] {
                if *byte {
                    return Err(bounded_output_machine_error("metadata output field bindings overlap"));
                }
                *byte = true;
            }
        }
        if covered.iter().any(|covered| !covered) {
            return Err(bounded_output_machine_error("metadata output field bindings do not cover exact output data"));
        }
        let lock = contract
            .get("lock_binding")
            .ok_or_else(|| bounded_output_machine_error("metadata bounded output contract has no lock binding"))?;
        let lock_name = text(lock, "plan_field");
        let lock_offset = number(lock, "plan_offset_bytes");
        let lock_width = number(lock, "width_bytes");
        let lock_type = text(lock, "ty");
        if lock_width != Some(32)
            || plan_layout.fields.iter().find(|field| Some(field.name.as_str()) == lock_name).is_none_or(|field| {
                Some(u64::from(field.offset)) != lock_offset
                    || field.width_bytes.map(u64::from) != lock_width
                    || !matches!(canonical_abi_type(lock_type.unwrap_or_default()).as_str(), "address" | "hash")
            })
        {
            return Err(bounded_output_machine_error("metadata lock binding does not match a 32-byte Plan field"));
        }
        contracts.push(BoundedOutputMachineContract {
            owner: format!("action:{scope_name}"),
            maximum,
            element_width,
            output_width,
            capacity_floor,
            field_count: field_bindings.len(),
            lock_plan_offset: lock_offset.expect("validated bounded output lock offset"),
        });
    }
    Ok(contracts)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_one_bounded_output_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    contract: &BoundedOutputMachineContract,
    header: &LoweringBlock,
    magic: &[&LoweringBlock],
    count_ok: &LoweringBlock,
    length_ok: &LoweringBlock,
    found: &LoweringBlock,
    done: &LoweringBlock,
    exact: &LoweringBlock,
    type_only: &LoweringBlock,
    capacity_ok: &LoweringBlock,
) -> Result<(), CheckerError> {
    if magic.len() != 8
        || !(header.range.start < magic[0].range.start
            && magic.windows(2).all(|pair| pair[0].range.start < pair[1].range.start)
            && magic[7].range.start < count_ok.range.start
            && count_ok.range.start < length_ok.range.start
            && length_ok.range.start < found.range.start
            && found.range.start < done.range.start)
    {
        return Err(bounded_output_machine_error("bounded Plan decoder blocks are not in canonical order"));
    }

    let plan_pointer_offset = ld_stack_offset(bounded_word(elf, header.range.start - 24)?, 29)
        .ok_or_else(|| bounded_output_machine_error("bounded Plan decoder does not load its exact witness pointer"))?;
    let _plan_size_offset = ld_stack_offset(bounded_word(elf, header.range.start - 20)?, 30)
        .ok_or_else(|| bounded_output_machine_error("bounded Plan decoder does not load its exact witness length"))?;
    if !is_ld(bounded_word(elf, header.range.start - 20)?, 30, 2, _plan_size_offset)
        || !is_addi(bounded_word(elf, header.range.start - 16)?, 5, 0, 12)
        || !is_sltu(bounded_word(elf, header.range.start - 12)?, 7, 30, 5)
        || !is_beq(bounded_word(elf, header.range.start - 8)?, 7, 0)
        || !flow_targets(elf, header.range.start - 8, header.range.start)
        || !jump_targets_runtime_error(record, elf, header.range.start - 4, 25)
    {
        return Err(bounded_output_machine_error("bounded Plan header no longer requires the exact 12-byte minimum"));
    }
    for (byte_index, (block, expected)) in magic.iter().zip(b"CSBPLv1\0").enumerate() {
        let start = block.range.start;
        if !is_lbu(bounded_word(elf, start - 20)?, 5, 29, byte_index as i32)
            || !is_addi(bounded_word(elf, start - 16)?, 6, 0, i32::from(*expected))
            || !is_sub(bounded_word(elf, start - 12)?, 7, 5, 6)
            || !is_beq(bounded_word(elf, start - 8)?, 7, 0)
            || !flow_targets(elf, start - 8, start)
            || !jump_targets_runtime_error(record, elf, start - 4, 25)
        {
            return Err(bounded_output_machine_error("bounded Plan magic comparison is incomplete"));
        }
    }

    let maximum = i32::try_from(contract.maximum)
        .map_err(|_| bounded_output_machine_error("bounded Plan maximum does not fit a machine immediate"))?;
    if !is_addi(bounded_word(elf, count_ok.range.start - 16)?, 5, 0, maximum)
        || !is_sltu(bounded_word(elf, count_ok.range.start - 12)?, 7, 5, 28)
        || !is_beq(bounded_word(elf, count_ok.range.start - 8)?, 7, 0)
        || !flow_targets(elf, count_ok.range.start - 8, count_ok.range.start)
        || !jump_targets_runtime_error(record, elf, count_ok.range.start - 4, 21)
    {
        return Err(bounded_output_machine_error("bounded Plan count is not the typed count <= N contract"));
    }
    let width = i32::try_from(contract.element_width)
        .map_err(|_| bounded_output_machine_error("bounded Plan width does not fit a machine immediate"))?;
    if !is_addi(bounded_word(elf, length_ok.range.start - 24)?, 5, 0, width)
        || !is_mul(bounded_word(elf, length_ok.range.start - 20)?, 6, 28, 5)
        || !is_addi(bounded_word(elf, length_ok.range.start - 16)?, 6, 6, 12)
        || !is_sub(bounded_word(elf, length_ok.range.start - 12)?, 7, 30, 6)
        || !is_beq(bounded_word(elf, length_ok.range.start - 8)?, 7, 0)
        || !flow_targets(elf, length_ok.range.start - 8, length_ok.range.start)
        || !jump_targets_runtime_error(record, elf, length_ok.range.start - 4, 25)
    {
        return Err(bounded_output_machine_error("bounded Plan does not enforce exact 12 + count * width length"));
    }
    let index_offset = ld_stack_offset(bounded_word(elf, length_ok.range.start)?, 5)
        .ok_or_else(|| bounded_output_machine_error("bounded Plan loop ordinal is not loaded from its typed stack slot"))?;
    let found_words = instructions_from_bounded(elf, found.range.start, 10)?;
    let destination_offset = sd_stack_offset(found_words[5].word, 29)
        .ok_or_else(|| bounded_output_machine_error("bounded Plan element pointer is not stored"))?;
    let element_size_offset = sd_stack_offset(found_words[7].word, 6)
        .ok_or_else(|| bounded_output_machine_error("bounded Plan element width is not stored"))?;
    let presence_offset = sd_stack_offset(found_words[9].word, 6)
        .ok_or_else(|| bounded_output_machine_error("bounded Plan presence bit is not stored"))?;
    if !is_addi(found_words[0].word, 6, 0, width)
        || !is_mul(found_words[1].word, 7, 5, 6)
        || !is_addi(found_words[2].word, 7, 7, 12)
        || !is_ld(found_words[3].word, 29, 2, plan_pointer_offset)
        || !is_add(found_words[4].word, 29, 29, 7)
        || !is_addi(found_words[6].word, 6, 0, width)
        || !is_addi(found_words[8].word, 6, 0, 1)
        || destination_offset == element_size_offset
        || destination_offset == presence_offset
        || element_size_offset == presence_offset
        || found.range.end != done.range.start
    {
        return Err(bounded_output_machine_error("bounded Plan element pointer/width/presence result is not canonical"));
    }

    let owner_sites = record
        .syscall_sites
        .iter()
        .filter(|site| block_for_address(record, site.address).is_some_and(|block| block.owner_entry == contract.owner))
        .collect::<Vec<_>>();
    let end_site = owner_sites
        .iter()
        .copied()
        .filter(|site| done.range.start < site.address && site.address < exact.range.start)
        .max_by_key(|site| site.address)
        .ok_or_else(|| bounded_output_machine_error("bounded output exact-count check has no GroupOutput capacity probe"))?;
    let end_abi = validate_bounded_group_input_syscall(
        record,
        elf,
        end_site.address,
        CKB_LOAD_CELL_BY_FIELD,
        Some(CKB_GROUP_OUTPUT),
        Some(0),
        Some(8),
    )?;
    if !is_addi(bounded_word(elf, exact.range.start - 16)?, 5, 0, 1)
        || !is_sub(bounded_word(elf, exact.range.start - 12)?, 6, 10, 5)
        || !is_beq(bounded_word(elf, exact.range.start - 8)?, 6, 0)
        || !flow_targets(elf, exact.range.start - 8, exact.range.start)
        || !jump_targets_runtime_error(record, elf, exact.range.start - 4, 21)
    {
        return Err(bounded_output_machine_error(
            "bounded output count no longer requires the first absent GroupOutput at the plan count",
        ));
    }

    let verify_sites = owner_sites
        .iter()
        .copied()
        .filter(|site| exact.range.end <= site.address && site.address < capacity_ok.range.start)
        .collect::<Vec<_>>();
    if verify_sites.len() != 4 {
        return Err(bounded_output_machine_error(
            "bounded output verification must contain data, current-Script, Lock-hash, and capacity syscalls",
        ));
    }
    let data_abi = validate_bounded_group_input_syscall(
        record,
        elf,
        verify_sites[0].address,
        CKB_LOAD_CELL_DATA,
        Some(CKB_GROUP_OUTPUT),
        None,
        Some(512),
    )?;
    validate_bounded_output_syscall_result(record, elf, verify_sites[0].address, data_abi.size_offset, contract.output_width, 21)?;
    let current_abi =
        validate_bounded_group_input_syscall(record, elf, verify_sites[1].address, CKB_LOAD_SCRIPT_HASH, None, None, Some(32))?;
    validate_bounded_output_syscall_result(record, elf, verify_sites[1].address, current_abi.size_offset, 32, 1)?;
    let lock_abi = validate_bounded_group_input_syscall(
        record,
        elf,
        verify_sites[2].address,
        CKB_LOAD_CELL_BY_FIELD,
        Some(CKB_GROUP_OUTPUT),
        Some(3),
        Some(32),
    )?;
    validate_bounded_output_syscall_result(record, elf, verify_sites[2].address, lock_abi.size_offset, 32, 12)?;
    let capacity_abi = validate_bounded_group_input_syscall(
        record,
        elf,
        verify_sites[3].address,
        CKB_LOAD_CELL_BY_FIELD,
        Some(CKB_GROUP_OUTPUT),
        Some(0),
        Some(8),
    )?;
    validate_bounded_output_syscall_result(record, elf, verify_sites[3].address, capacity_abi.size_offset, 8, 26)?;
    if data_abi.index_offset != Some(index_offset)
        || lock_abi.index_offset != Some(index_offset)
        || capacity_abi.index_offset != Some(index_offset)
        || end_abi.index_offset != Some(index_offset)
    {
        return Err(bounded_output_machine_error(
            "plan decode, data, Lock, capacity, and exact-count probes do not share one group-relative ordinal",
        ));
    }

    let data_error_guards = elf
        .instructions
        .iter()
        .filter(|instruction| verify_sites[0].address + 32 <= instruction.address && instruction.address < verify_sites[1].address)
        .filter(|instruction| jump_targets_runtime_error(record, elf, instruction.address, 3))
        .collect::<Vec<_>>();
    if data_error_guards.len() != contract.field_count
        || data_error_guards.iter().any(|instruction| !guarded_runtime_error_jump(elf, instruction.address))
    {
        return Err(bounded_output_machine_error(
            "bounded output data does not have one guarded exact comparison for every typed field",
        ));
    }

    let role_words = instructions_from_bounded(elf, type_only.range.start - 68, 17)?;
    if !is_ld(role_words[0].word, 5, 2, lock_abi.buffer_offset)
        || !is_ld(role_words[1].word, 6, 2, current_abi.buffer_offset)
        || !is_sub(role_words[2].word, 7, 5, 6)
    {
        return Err(bounded_output_machine_error("bounded output Lock/Type role comparison does not start at word zero"));
    }
    for word_index in 1..4 {
        let start = 3 + (word_index - 1) * 4;
        let byte_offset =
            i32::try_from(word_index * 8).map_err(|_| bounded_output_machine_error("bounded output hash word offset overflowed"))?;
        if !is_ld(role_words[start].word, 5, 2, lock_abi.buffer_offset + byte_offset)
            || !is_ld(role_words[start + 1].word, 6, 2, current_abi.buffer_offset + byte_offset)
            || !is_sub(role_words[start + 2].word, 5, 5, 6)
            || !is_or(role_words[start + 3].word, 7, 7, 5)
        {
            return Err(bounded_output_machine_error("bounded output Lock/Type role comparison omits a hash word"));
        }
    }
    if !is_bne(role_words[15].word, 7, 0)
        || !flow_targets(elf, role_words[15].address, type_only.range.start)
        || !jump_targets_runtime_error(record, elf, role_words[16].address, 47)
    {
        return Err(bounded_output_machine_error(
            "bounded GroupOutput Lock hash is no longer required to differ from the current Type Script hash",
        ));
    }
    let lock_value_errors = elf
        .instructions
        .iter()
        .filter(|instruction| type_only.range.start <= instruction.address && instruction.address < verify_sites[3].address)
        .filter(|instruction| jump_targets_runtime_error(record, elf, instruction.address, 12))
        .collect::<Vec<_>>();
    if lock_value_errors.len() != 1 || !guarded_runtime_error_jump(elf, lock_value_errors[0].address) {
        return Err(bounded_output_machine_error("bounded output Lock hash is not compared with the exact Plan field"));
    }
    let lock_start = type_only.range.start;
    let lock_end = lock_value_errors[0].address;
    let pointer_load = elf
        .instructions
        .iter()
        .find(|instruction| {
            lock_start <= instruction.address && instruction.address < lock_end && is_ld(instruction.word, 11, 2, destination_offset)
        })
        .ok_or_else(|| {
            bounded_output_machine_error("bounded output Lock comparison does not load the decoded Plan element pointer")
        })?;
    let pointer_ready = if contract.lock_plan_offset == 0 {
        pointer_load.address
    } else if contract.lock_plan_offset <= 2047 {
        let address = pointer_load.address + 4;
        let offset = i32::try_from(contract.lock_plan_offset)
            .map_err(|_| bounded_output_machine_error("bounded output Lock Plan offset does not fit a machine immediate"))?;
        if !is_addi(bounded_word(elf, address)?, 11, 11, offset) {
            return Err(bounded_output_machine_error("bounded output Lock comparison uses the wrong Plan field offset"));
        }
        address
    } else {
        let block = block_for_address(record, pointer_load.address)
            .ok_or_else(|| bounded_output_machine_error("bounded output Lock pointer adjustment is outside machine coverage"))?;
        elf.instructions
            .iter()
            .filter(|instruction| pointer_load.address < instruction.address && instruction.address < lock_end)
            .find(|instruction| {
                let word = instruction.word;
                if word & 0x7f != 0x33 || (word >> 7) & 0x1f != 11 || (word >> 15) & 0x1f != 11 {
                    return false;
                }
                let scratch = (word >> 20) & 0x1f;
                register_constant_before(elf, block, instruction.address, scratch as usize) == Some(contract.lock_plan_offset)
            })
            .map(|instruction| instruction.address)
            .ok_or_else(|| bounded_output_machine_error("bounded output Lock comparison uses the wrong large Plan field offset"))?
    };
    let comparison = elf
        .instructions
        .iter()
        .find(|instruction| {
            pointer_ready < instruction.address
                && instruction.address + 20 < lock_end
                && is_addi(instruction.word, 10, 2, data_abi.buffer_offset)
                && bounded_word(elf, instruction.address + 4).is_ok_and(|word| is_addi(word, 12, 0, 32))
                && bounded_word(elf, instruction.address + 8).is_ok_and(|word| is_auipc(word, 1))
                && bounded_word(elf, instruction.address + 12).is_ok_and(is_jalr_call)
                && bounded_word(elf, instruction.address + 16).is_ok_and(|word| is_bne(word, 10, 0))
        })
        .ok_or_else(|| bounded_output_machine_error("bounded output Lock comparison is not an exact 32-byte Plan-field comparison"))?;
    if !flow_targets(elf, comparison.address + 16, lock_value_errors[0].address) {
        return Err(bounded_output_machine_error("bounded output Lock mismatch branch no longer reaches error 12"));
    }
    let capacity_branch = capacity_ok.range.start - 8;
    let branch_block = block_for_address(record, capacity_branch)
        .ok_or_else(|| bounded_output_machine_error("bounded output capacity comparison is outside machine coverage"))?;
    if !is_bgeu(bounded_word(elf, capacity_branch)?, 5, 6)
        || !flow_targets(elf, capacity_branch, capacity_ok.range.start)
        || register_constant_before(elf, branch_block, capacity_branch, 6) != Some(contract.capacity_floor)
        || !jump_targets_runtime_error(record, elf, capacity_ok.range.start - 4, 26)
    {
        return Err(bounded_output_machine_error(
            "bounded output capacity is not checked against the metadata-declared positive floor",
        ));
    }
    for code in [1, 3, 4, 12, 21, 25, 26, 47] {
        if !owner_has_abort_error(record, elf, &contract.owner, code) {
            return Err(bounded_output_machine_error(format!(
                "entry '{}' no longer has stable bounded output runtime error {code}",
                contract.owner
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_bounded_output_syscall_result(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    address: u64,
    size_offset: i32,
    expected_width: u64,
    status_error: i32,
) -> Result<(), CheckerError> {
    let width = i32::try_from(expected_width)
        .map_err(|_| bounded_output_machine_error("bounded output syscall width does not fit a machine immediate"))?;
    if !is_beq(bounded_word(elf, address + 4)?, 10, 0)
        || !flow_targets(elf, address + 4, address + 12)
        || !jump_targets_runtime_error(record, elf, address + 8, status_error)
        || !is_ld(bounded_word(elf, address + 12)?, 10, 2, size_offset)
        || !is_addi(bounded_word(elf, address + 16)?, 11, 0, width)
        || !is_sub(bounded_word(elf, address + 20)?, 10, 10, 11)
        || !is_beq(bounded_word(elf, address + 24)?, 10, 0)
        || !flow_targets(elf, address + 24, address + 32)
        || !jump_targets_runtime_error(record, elf, address + 28, 4)
    {
        return Err(bounded_output_machine_error("bounded output syscall no longer requires success and its exact typed width"));
    }
    Ok(())
}

pub(super) fn guarded_runtime_error_jump(elf: &ParsedElf, address: u64) -> bool {
    let prior = elf.control_flow.iter().filter(|edge| edge.address + 8 >= address && edge.address < address).collect::<Vec<_>>();
    prior.iter().any(|edge| bounded_word(elf, edge.address).is_ok_and(|word| word & 0x7f == 0x63) && edge.target >= address)
}

pub(super) fn bounded_output_machine_error(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("bounded GroupOutput machine contract: {}", message.into()),
    )
}

pub(super) fn validate_bounded_group_input_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
) -> Result<(), CheckerError> {
    let mut contracts = Vec::new();
    for entry in &record.typed_semantics.entries {
        for block in &entry.blocks {
            for operation in &block.operations {
                if operation.opcode != "bounded-cell-load" {
                    continue;
                }
                let TypedSemanticOperationDetail::Collection { declared_type } = &operation.detail else {
                    return Err(bounded_group_input_machine_error("bounded Cell load has no collection contract"));
                };
                let (element, maximum) = parse_bounded_cell_set_contract(declared_type)
                    .ok_or_else(|| bounded_group_input_machine_error("bounded Cell load has an invalid declared type"))?;
                let element_width = record
                    .typed_semantics
                    .types
                    .iter()
                    .find(|ty| ty.name == element)
                    .and_then(|ty| ty.encoded_size)
                    .map(u64::from)
                    .ok_or_else(|| bounded_group_input_machine_error("bounded Cell element has no fixed encoded width"))?;
                contracts.push(BoundedGroupInputMachineContract { owner: entry.id.as_str(), maximum, element_width });
            }
        }
    }
    if contracts.is_empty() {
        return Ok(());
    }

    let owners = contracts.iter().map(|contract| contract.owner).collect::<BTreeSet<_>>();
    for owner in owners {
        let owner_contracts = contracts.iter().filter(|contract| contract.owner == owner).collect::<Vec<_>>();
        let loaded = generated_blocks(record, owner, ".Lbounded_cell_loaded_");
        let count_ok = generated_blocks(record, owner, ".Lbounded_cell_count_ok_");
        let out_of_bound = generated_blocks(record, owner, ".Lbounded_cell_out_of_bound_");
        let done = generated_blocks(record, owner, ".Lbounded_cell_load_done_");
        if loaded.len() != owner_contracts.len()
            || count_ok.len() != owner_contracts.len()
            || out_of_bound.len() != owner_contracts.len()
            || done.len() != owner_contracts.len()
        {
            return Err(bounded_group_input_machine_error(format!(
                "entry '{owner}' does not have one complete machine scan for each typed bounded Cell load"
            )));
        }
        for (index, contract) in owner_contracts.into_iter().enumerate() {
            validate_one_bounded_group_input_machine_contract(
                record,
                elf,
                contract,
                loaded[index],
                count_ok[index],
                out_of_bound[index],
                done[index],
            )?;
        }
    }
    Ok(())
}

pub(super) fn parse_bounded_cell_set_contract(declared_type: &str) -> Option<(&str, u64)> {
    let body = declared_type.strip_prefix("BoundedCellSet<")?.strip_suffix('>')?;
    let (element, maximum) = body.rsplit_once(',')?;
    Some((element.trim(), maximum.trim().parse().ok()?))
}

pub(super) fn generated_blocks<'a>(record: &'a VerifiedLoweringRecord, owner: &str, prefix: &str) -> Vec<&'a LoweringBlock> {
    let mut blocks = record
        .blocks
        .iter()
        .filter(|block| {
            block.owner_entry == owner && block.machine_label.as_deref().is_some_and(|label| generated_label(label, prefix))
        })
        .collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    blocks
}

pub(super) fn validate_one_bounded_group_input_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    contract: &BoundedGroupInputMachineContract<'_>,
    loaded: &LoweringBlock,
    count_ok: &LoweringBlock,
    out_of_bound: &LoweringBlock,
    done: &LoweringBlock,
) -> Result<(), CheckerError> {
    if !(loaded.range.start < count_ok.range.start
        && count_ok.range.start < out_of_bound.range.start
        && out_of_bound.range.start < done.range.start)
    {
        return Err(bounded_group_input_machine_error("bounded Cell scan blocks are not in canonical order"));
    }
    let data_site = record
        .syscall_sites
        .iter()
        .filter(|site| site.address < loaded.range.start)
        .filter(|site| block_for_address(record, site.address).is_some_and(|block| block.owner_entry == contract.owner))
        .max_by_key(|site| site.address)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell scan has no LOAD_CELL_DATA syscall"))?;
    let identity_sites = record
        .syscall_sites
        .iter()
        .filter(|site| loaded.range.start < site.address && site.address < done.range.start)
        .collect::<Vec<_>>();
    if identity_sites.len() != 3 {
        return Err(bounded_group_input_machine_error(
            "bounded Cell scan must contain current-Script, Type-hash, and Lock-hash syscalls",
        ));
    }
    let data_abi = validate_bounded_group_input_syscall(
        record,
        elf,
        data_site.address,
        CKB_LOAD_CELL_DATA,
        Some(CKB_GROUP_INPUT),
        None,
        Some(512),
    )?;
    let current_hash_abi =
        validate_bounded_group_input_syscall(record, elf, identity_sites[0].address, CKB_LOAD_SCRIPT_HASH, None, None, Some(32))?;
    let type_hash_abi = validate_bounded_group_input_syscall(
        record,
        elf,
        identity_sites[1].address,
        CKB_LOAD_CELL_BY_FIELD,
        Some(CKB_GROUP_INPUT),
        Some(5),
        Some(32),
    )?;
    let lock_hash_abi = validate_bounded_group_input_syscall(
        record,
        elf,
        identity_sites[2].address,
        CKB_LOAD_CELL_BY_FIELD,
        Some(CKB_GROUP_INPUT),
        Some(3),
        Some(32),
    )?;
    let index_offset = data_abi
        .index_offset
        .ok_or_else(|| bounded_group_input_machine_error("LOAD_CELL_DATA does not use the typed loop ordinal"))?;
    if type_hash_abi.index_offset != Some(index_offset) || lock_hash_abi.index_offset != Some(index_offset) {
        return Err(bounded_group_input_machine_error(
            "bounded Cell data, Type hash, and Lock hash syscalls do not use the same typed loop ordinal",
        ));
    }

    let data = data_site.address;
    if !is_beq(bounded_word(elf, data + 4)?, 10, 0)
        || !flow_targets(elf, data + 4, loaded.range.start)
        || !is_addi(bounded_word(elf, data + 8)?, 5, 0, 1)
        || !is_sub(bounded_word(elf, data + 12)?, 6, 10, 5)
        || !is_beq(bounded_word(elf, data + 16)?, 6, 0)
        || !flow_targets(elf, data + 16, out_of_bound.range.start)
        || !jump_targets_runtime_error(record, elf, data + 20, 3)
    {
        return Err(bounded_group_input_machine_error(
            "LOAD_CELL_DATA status no longer distinguishes success, end-of-group, and failure",
        ));
    }

    let maximum = i32::try_from(contract.maximum)
        .map_err(|_| bounded_group_input_machine_error("bounded Cell maximum does not fit its machine immediate"))?;
    let loaded_words = instructions_from_bounded(elf, loaded.range.start, 4)?;
    if !is_ld(loaded_words[0].word, 28, 2, index_offset)
        || !is_addi(loaded_words[1].word, 5, 0, maximum)
        || !is_sltu(loaded_words[2].word, 6, 28, 5)
        || !is_bne(loaded_words[3].word, 6, 0)
        || !flow_targets(elf, loaded_words[3].address, count_ok.range.start)
        || !jump_targets_runtime_error(record, elf, loaded.range.end, 21)
    {
        return Err(bounded_group_input_machine_error("runtime cardinality is not the typed strict index < N contract"));
    }

    let width = i32::try_from(contract.element_width)
        .map_err(|_| bounded_group_input_machine_error("bounded Cell width does not fit its machine immediate"))?;
    let count_words = instructions_from_bounded(elf, count_ok.range.start, 4)?;
    if !is_ld(count_words[0].word, 10, 2, data_abi.size_offset)
        || !is_addi(count_words[1].word, 11, 0, width)
        || !is_sub(count_words[2].word, 10, 10, 11)
        || !is_beq(count_words[3].word, 10, 0)
        || !jump_targets_runtime_error(record, elf, count_ok.range.end, 4)
    {
        return Err(bounded_group_input_machine_error("bounded Cell data is no longer decoded with the typed exact width"));
    }

    let type_words = generated_blocks(record, contract.owner, ".Lbounded_cell_type_word_ok_")
        .into_iter()
        .filter(|block| loaded.range.start < block.range.start && block.range.start < done.range.start)
        .collect::<Vec<_>>();
    if type_words.len() != 4 {
        return Err(bounded_group_input_machine_error("bounded Cell Type-hash identity must compare all four 64-bit words"));
    }
    for (word_index, block) in type_words.into_iter().enumerate() {
        let start = block.range.start;
        let byte_offset = i32::try_from(word_index * 8)
            .map_err(|_| bounded_group_input_machine_error("bounded Cell Type-hash word offset overflowed"))?;
        if !is_ld(bounded_word(elf, start - 20)?, 5, 2, type_hash_abi.buffer_offset + byte_offset)
            || !is_ld(bounded_word(elf, start - 16)?, 6, 2, current_hash_abi.buffer_offset + byte_offset)
            || !is_sub(bounded_word(elf, start - 12)?, 7, 5, 6)
            || !is_beq(bounded_word(elf, start - 8)?, 7, 0)
            || !flow_targets(elf, start - 8, start)
            || !jump_targets_runtime_error(record, elf, start - 4, 17)
        {
            return Err(bounded_group_input_machine_error("bounded Cell Type-hash identity comparison is incomplete"));
        }
    }

    let lock_distinct = generated_blocks(record, contract.owner, ".Lbounded_cell_lock_is_distinct_")
        .into_iter()
        .find(|block| loaded.range.start < block.range.start && block.range.start < done.range.start)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell scan has no Lock/Type role separation"))?;
    validate_bounded_hash_syscall_result(record, elf, identity_sites[0].address, current_hash_abi.size_offset, 1)?;
    validate_bounded_hash_syscall_result(record, elf, identity_sites[1].address, type_hash_abi.size_offset, 47)?;
    validate_bounded_hash_syscall_result(record, elf, identity_sites[2].address, lock_hash_abi.size_offset, 47)?;

    let lock_fold_start = identity_sites[2].address + 32;
    let lock_fold = instructions_from_bounded(elf, lock_fold_start, 17)?;
    if !is_ld(lock_fold[0].word, 5, 2, lock_hash_abi.buffer_offset)
        || !is_ld(lock_fold[1].word, 6, 2, current_hash_abi.buffer_offset)
        || !is_sub(lock_fold[2].word, 7, 5, 6)
    {
        return Err(bounded_group_input_machine_error("bounded Cell Lock/Type comparison does not begin with word zero"));
    }
    for word_index in 1..4 {
        let start = 3 + (word_index - 1) * 4;
        let byte_offset = i32::try_from(word_index * 8)
            .map_err(|_| bounded_group_input_machine_error("bounded Cell Lock-hash word offset overflowed"))?;
        if !is_ld(lock_fold[start].word, 5, 2, lock_hash_abi.buffer_offset + byte_offset)
            || !is_ld(lock_fold[start + 1].word, 6, 2, current_hash_abi.buffer_offset + byte_offset)
            || !is_sub(lock_fold[start + 2].word, 5, 5, 6)
            || !is_or(lock_fold[start + 3].word, 7, 7, 5)
        {
            return Err(bounded_group_input_machine_error(
                "bounded Cell Lock/Type comparison does not fold all corresponding hash words",
            ));
        }
    }
    if lock_fold[15].address != lock_distinct.range.start - 8
        || !is_bne(lock_fold[15].word, 7, 0)
        || !flow_targets(elf, lock_distinct.range.start - 8, lock_distinct.range.start)
        || lock_fold[16].address != lock_distinct.range.start - 4
        || !jump_targets_runtime_error(record, elf, lock_fold[16].address, 47)
    {
        return Err(bounded_group_input_machine_error(
            "bounded Cell Lock hash is no longer required to differ from the current Type Script hash",
        ));
    }

    let success_words = instructions_from_bounded(elf, lock_distinct.range.start, 5)?;
    let destination_offset = sd_stack_offset(success_words[1].word, 5)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell success does not store its element pointer"))?;
    let found_offset = sd_stack_offset(success_words[3].word, 5)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell success does not store its presence bit"))?;
    if destination_offset == found_offset
        || !is_addi(success_words[0].word, 5, 2, data_abi.buffer_offset)
        || !is_addi(success_words[2].word, 5, 0, 1)
        || !is_jal_zero(success_words[4].word)
        || !flow_targets(elf, success_words[4].address, done.range.start)
        || lock_distinct.range.end != out_of_bound.range.start
    {
        return Err(bounded_group_input_machine_error(
            "bounded Cell success no longer returns the loaded element and canonical presence bit",
        ));
    }

    let out_words = instructions_from_bounded(elf, out_of_bound.range.start, 2)?;
    if !is_sd(out_words[0].word, 0, 2, destination_offset)
        || !is_sd(out_words[1].word, 0, 2, found_offset)
        || out_of_bound.range.end != done.range.start
    {
        return Err(bounded_group_input_machine_error("end-of-group no longer produces the canonical absent element"));
    }
    for code in [1, 3, 4, 17, 21, 47] {
        if !owner_has_abort_error(record, elf, contract.owner, code) {
            return Err(bounded_group_input_machine_error(format!(
                "entry '{}' no longer has stable runtime error {code}",
                contract.owner
            )));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct BoundedGroupInputSyscallAbi {
    pub(super) buffer_offset: i32,
    pub(super) size_offset: i32,
    pub(super) index_offset: Option<i32>,
}

pub(super) fn validate_bounded_group_input_syscall(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    address: u64,
    syscall: u64,
    source: Option<u64>,
    field: Option<u64>,
    initialized_size: Option<i32>,
) -> Result<BoundedGroupInputSyscallAbi, CheckerError> {
    let block = block_for_address(record, address)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell syscall is outside machine coverage"))?;
    if register_constant_before(elf, block, address, 17) != Some(syscall)
        || register_constant_before(elf, block, address, 12) != Some(0)
        || source.is_some_and(|source| register_constant_before(elf, block, address, 14) != Some(source))
        || field.is_some_and(|field| register_constant_before(elf, block, address, 15) != Some(field))
    {
        return Err(bounded_group_input_machine_error(format!("bounded Cell syscall ABI changed at {address:#x}")));
    }
    let (a0_address, a0_word) = last_register_definition_before(elf, block, address, 10)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell syscall has no a0 buffer definition"))?;
    let (_, a1_word) = last_register_definition_before(elf, block, address, 11)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell syscall has no a1 size definition"))?;
    let buffer_offset = stack_address_offset(a0_word, 10)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell syscall a0 is not a canonical stack buffer"))?;
    let size_offset = stack_address_offset(a1_word, 11)
        .ok_or_else(|| bounded_group_input_machine_error("bounded Cell syscall a1 is not a canonical stack size slot"))?;
    if buffer_offset != size_offset + 8 {
        return Err(bounded_group_input_machine_error("bounded Cell syscall buffer and size slots are not adjacent"));
    }
    if let Some(initialized_size) = initialized_size
        && (a0_address < block.range.start + 8
            || !is_addi(bounded_word(elf, a0_address - 8)?, 5, 0, initialized_size)
            || !is_sd(bounded_word(elf, a0_address - 4)?, 5, 2, size_offset))
    {
        return Err(bounded_group_input_machine_error("bounded Cell syscall size slot is not initialized for its canonical buffer"));
    }

    let index_offset =
        if source.is_some() {
            let (a3_address, a3_word) = last_register_definition_before(elf, block, address, 13)
                .ok_or_else(|| bounded_group_input_machine_error("bounded Cell syscall has no a3 index definition"))?;
            if !is_addi(a3_word, 13, 28, 0) {
                return Err(bounded_group_input_machine_error("bounded Cell syscall a3 is not the typed loop ordinal"));
            }
            let (_, index_word) = last_register_definition_before(elf, block, a3_address, 28)
                .ok_or_else(|| bounded_group_input_machine_error("bounded Cell typed loop ordinal has no stack definition"))?;
            Some(ld_stack_offset(index_word, 28).ok_or_else(|| {
                bounded_group_input_machine_error("bounded Cell typed loop ordinal is not loaded from its stack slot")
            })?)
        } else {
            None
        };
    Ok(BoundedGroupInputSyscallAbi { buffer_offset, size_offset, index_offset })
}

pub(super) fn validate_bounded_hash_syscall_result(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    address: u64,
    size_offset: i32,
    status_error: i32,
) -> Result<(), CheckerError> {
    if !is_beq(bounded_word(elf, address + 4)?, 10, 0)
        || !flow_targets(elf, address + 4, address + 12)
        || !jump_targets_runtime_error(record, elf, address + 8, status_error)
        || !is_ld(bounded_word(elf, address + 12)?, 10, 2, size_offset)
        || !is_addi(bounded_word(elf, address + 16)?, 11, 0, 32)
        || !is_sub(bounded_word(elf, address + 20)?, 10, 10, 11)
        || !is_beq(bounded_word(elf, address + 24)?, 10, 0)
        || !flow_targets(elf, address + 24, address + 32)
        || !jump_targets_runtime_error(record, elf, address + 28, 4)
    {
        return Err(bounded_group_input_machine_error(
            "bounded Cell identity syscall no longer requires success and an exact 32-byte hash",
        ));
    }
    Ok(())
}

pub(crate) fn last_register_definition_before(
    elf: &ParsedElf,
    block: &LoweringBlock,
    address: u64,
    register: u32,
) -> Option<(u64, u32)> {
    elf.instructions
        .iter()
        .filter(|instruction| block.range.start <= instruction.address && instruction.address < address)
        .rev()
        .find(|instruction| instruction_writes_register(instruction.word, register))
        .map(|instruction| (instruction.address, instruction.word))
}

pub(super) fn instruction_writes_register(word: u32, register: u32) -> bool {
    let opcode = word & 0x7f;
    let destination = (word >> 7) & 0x1f;
    (word == 0x00000073 && register == 10)
        || (matches!(opcode, 0x67 | 0x6f) && destination != 0 && caller_saved_register(register))
        || crate::machine::decode(word).is_some_and(|instruction| instruction.effects().writes_register(register))
}

pub(super) fn caller_saved_register(register: u32) -> bool {
    crate::machine::caller_saved(register)
}

pub(crate) fn stack_address_offset(word: u32, register: u32) -> Option<i32> {
    (word & 0x7f == 0x13 && (word >> 12) & 0x7 == 0 && (word >> 7) & 0x1f == register && (word >> 15) & 0x1f == 2)
        .then_some((word as i32) >> 20)
}

pub(super) fn ld_stack_offset(word: u32, register: u32) -> Option<i32> {
    (word & 0x7f == 0x03 && (word >> 12) & 0x7 == 0x3 && (word >> 7) & 0x1f == register && (word >> 15) & 0x1f == 2)
        .then_some((word as i32) >> 20)
}

pub(super) fn sd_stack_offset(word: u32, source: u32) -> Option<i32> {
    if word & 0x7f != 0x23 || (word >> 12) & 0x7 != 0x3 || (word >> 20) & 0x1f != source || (word >> 15) & 0x1f != 2 {
        return None;
    }
    let immediate = (((word >> 25) & 0x7f) << 5) | ((word >> 7) & 0x1f);
    Some(((immediate as i32) << 20) >> 20)
}

pub(super) fn bounded_word(elf: &ParsedElf, address: u64) -> Result<u32, CheckerError> {
    elf.instructions
        .iter()
        .find(|instruction| instruction.address == address)
        .map(|instruction| instruction.word)
        .ok_or_else(|| bounded_group_input_machine_error(format!("missing instruction at {address:#x}")))
}

pub(super) fn instructions_from_bounded(
    elf: &ParsedElf,
    address: u64,
    count: usize,
) -> Result<&[crate::elf::DecodedInstruction], CheckerError> {
    let start = elf
        .instructions
        .binary_search_by_key(&address, |instruction| instruction.address)
        .map_err(|_| bounded_group_input_machine_error(format!("missing instruction range at {address:#x}")))?;
    elf.instructions
        .get(start..start.saturating_add(count))
        .ok_or_else(|| bounded_group_input_machine_error(format!("truncated instruction range at {address:#x}")))
}

pub(crate) fn jump_targets_runtime_error(record: &VerifiedLoweringRecord, elf: &ParsedElf, address: u64, code: i32) -> bool {
    let Some(target_address) = elf.control_flow.iter().find(|edge| edge.address == address).map(|edge| edge.target) else {
        return false;
    };
    record.runtime_error_exits.iter().any(|exit| exit.code == code && exit.address == target_address)
        && bounded_word(elf, address).is_ok_and(is_jal_zero)
        && machine_error_jumps_to_abort(record, elf, target_address, code)
}

pub(super) fn bounded_group_input_machine_error(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("bounded GroupInput machine contract: {}", message.into()),
    )
}

pub(super) fn validate_policy_dispatch_machine_contract(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    if !matches!(record.typed_semantics.foundation.entry_contract.dispatch, EntryDispatchContract::PolicyWitnessV1(_)) {
        return Ok(());
    }
    if let Some((logical_record, logical_elf)) =
        crate::policy_machine::normalize_relaxed_branches(record, elf).map_err(policy_machine_error)?
    {
        validate_policy_dispatch_logical_machine_contract(&logical_record, &logical_elf)
    } else {
        validate_policy_dispatch_logical_machine_contract(record, elf)
    }
}

pub(super) fn validate_policy_dispatch_logical_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
) -> Result<(), CheckerError> {
    let EntryDispatchContract::PolicyWitnessV1(contract) = &record.typed_semantics.foundation.entry_contract.dispatch else {
        return Ok(());
    };
    let wrapper = record
        .entries
        .iter()
        .find(|entry| entry.id == POLICY_WRAPPER_ENTRY_ID)
        .ok_or_else(|| policy_machine_error("missing policy entry wrapper"))?;
    if wrapper.kind != EntryKind::Wrapper
        || wrapper.name != "_cellscript_entry"
        || wrapper.frame_size_bytes != POLICY_ENTRY_FRAME_BYTES
        || wrapper.outgoing_argument_bytes != 0
        || record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == POLICY_WRAPPER_ENTRY_ID)
            .any(|block| block.frame_size_bytes != POLICY_ENTRY_FRAME_BYTES)
    {
        return Err(policy_machine_error("policy entry wrapper frame or entry contract changed"));
    }

    let fail = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_witness_fail_")?;
    let done = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_entry_done_")?;
    let record_loop = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_record_")?;
    let args_valid = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_record_args_valid_")?;
    let key_loop = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_key_order_loop_")?;
    let ordered = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_key_ordered_")?;
    let hash_loop = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_current_hash_loop_")?;
    let next_record = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_next_record_")?;

    validate_policy_entry_syscalls(record, elf, fail)?;
    validate_policy_envelope_machine_contract(record, elf, fail)?;
    validate_policy_dynvec_machine_contract(elf, record_loop, fail)?;
    validate_policy_record_layout_machine_contract(record, elf, args_valid, fail)?;
    validate_policy_key_order_machine_contract(elf, args_valid, key_loop, ordered, fail)?;
    validate_policy_selector_machine_contract(elf, record_loop, ordered, hash_loop, next_record, fail)?;
    validate_policy_tag_dispatch_machine_contract(record, elf, contract, next_record, fail, done)?;
    validate_policy_action_adapters(record, elf, contract)?;

    if !machine_error_jumps_to_abort(record, elf, fail.range.start, 25) {
        return Err(policy_machine_error("policy rejection no longer terminates with entry-witness error 25"));
    }
    let done_words = instructions_from(elf, done.range.start, 8)?;
    if !matches_large_stack_load(done_words, 0, 1, POLICY_RA_OFFSET)
        || !matches_large_sp_adjust(done_words, 4, POLICY_ENTRY_FRAME_BYTES as i32)
        || done_words.get(7).is_none_or(|instruction| instruction.word != 0x0000_8067)
    {
        return Err(policy_machine_error("policy completion no longer restores the exact wrapper frame"));
    }
    Ok(())
}

pub(crate) fn unique_policy_block<'a>(
    record: &'a VerifiedLoweringRecord,
    owner: &str,
    label_prefix: &str,
) -> Result<&'a LoweringBlock, CheckerError> {
    let mut matches = record.blocks.iter().filter(|block| {
        block.owner_entry == owner && block.machine_label.as_deref().is_some_and(|label| generated_label(label, label_prefix))
    });
    let block =
        matches.next().ok_or_else(|| policy_machine_error(format!("missing machine block '{label_prefix}*' for '{owner}'")))?;
    if matches.next().is_some() {
        return Err(policy_machine_error(format!("ambiguous machine block '{label_prefix}*' for '{owner}'")));
    }
    Ok(block)
}

pub(super) fn generated_label(label: &str, prefix: &str) -> bool {
    label.strip_prefix(prefix).is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
}

pub(super) fn policy_machine_error(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("policy dispatch machine contract: {}", message.into()),
    )
}

pub(super) fn instruction_at(elf: &ParsedElf, address: u64) -> Result<&crate::elf::DecodedInstruction, CheckerError> {
    elf.instructions
        .iter()
        .find(|instruction| instruction.address == address)
        .ok_or_else(|| policy_machine_error(format!("missing instruction at {address:#x}")))
}

pub(super) fn instructions_from(
    elf: &ParsedElf,
    address: u64,
    count: usize,
) -> Result<&[crate::elf::DecodedInstruction], CheckerError> {
    let start = elf
        .instructions
        .binary_search_by_key(&address, |instruction| instruction.address)
        .map_err(|_| policy_machine_error(format!("missing instruction range at {address:#x}")))?;
    elf.instructions
        .get(start..start.saturating_add(count))
        .ok_or_else(|| policy_machine_error(format!("truncated instruction range at {address:#x}")))
}

pub(crate) fn flow_targets(elf: &ParsedElf, address: u64, target: u64) -> bool {
    elf.control_flow.iter().any(|edge| edge.address == address && edge.target == target)
}

pub(crate) fn block_for_address(record: &VerifiedLoweringRecord, address: u64) -> Option<&LoweringBlock> {
    record.blocks.iter().find(|block| block.range.contains(address))
}

pub(super) fn validate_policy_entry_syscalls(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    fail: &LoweringBlock,
) -> Result<(), CheckerError> {
    let wrapper_blocks = record.blocks.iter().filter(|block| block.owner_entry == POLICY_WRAPPER_ENTRY_ID).collect::<Vec<_>>();
    let mut sites = elf
        .syscall_addresses
        .iter()
        .copied()
        .filter(|address| wrapper_blocks.iter().any(|block| block.range.contains(*address)))
        .collect::<Vec<_>>();
    sites.sort_unstable();
    if sites.len() != 4 {
        return Err(policy_machine_error(
            "policy selector must use exactly two witness loads, one empty-group probe, and one current Script hash load",
        ));
    }
    let expected = [
        (CKB_LOAD_WITNESS, CKB_GROUP_INPUT, None),
        (CKB_LOAD_CELL_BY_FIELD, CKB_GROUP_INPUT, Some(0_u64)),
        (CKB_LOAD_WITNESS, CKB_GROUP_OUTPUT, None),
        (CKB_LOAD_SCRIPT_HASH, 0, None),
    ];
    for (address, (syscall, source, field)) in sites.iter().copied().zip(expected) {
        let block =
            block_for_address(record, address).ok_or_else(|| policy_machine_error("policy syscall is outside machine coverage"))?;
        if register_constant_before(elf, block, address, 17) != Some(syscall)
            || register_constant_before(elf, block, address, 12) != Some(0)
            || (syscall != CKB_LOAD_SCRIPT_HASH
                && (register_constant_before(elf, block, address, 13) != Some(0)
                    || register_constant_before(elf, block, address, 14) != Some(source)))
            || field.is_some_and(|field| register_constant_before(elf, block, address, 15) != Some(field))
        {
            return Err(policy_machine_error("policy witness/current-Script selector syscall ABI changed"));
        }
    }
    if !is_beq(instruction_at(elf, sites[0] + 4)?.word, 10, 0)
        || !is_addi(instruction_at(elf, sites[1] + 4)?.word, 5, 0, 1)
        || !is_beq(instruction_at(elf, sites[1] + 8)?.word, 10, 5)
        || !is_jal_zero(instruction_at(elf, sites[1] + 12)?.word)
        || !flow_targets(elf, sites[1] + 12, fail.range.start)
        || !is_bne(instruction_at(elf, sites[2] + 4)?.word, 10, 0)
        || !flow_targets(elf, sites[2] + 4, fail.range.start)
        || !is_bne(instruction_at(elf, sites[3] + 4)?.word, 10, 0)
        || !flow_targets(elf, sites[3] + 4, fail.range.start)
    {
        return Err(policy_machine_error("policy syscall status/fallback control flow changed"));
    }
    let output_only = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_output_only_")?;
    let loaded = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_witness_loaded_")?;
    if !flow_targets(elf, sites[0] + 4, loaded.range.start) || !flow_targets(elf, sites[1] + 8, output_only.range.start) {
        return Err(policy_machine_error("policy GroupOutput fallback is no longer gated by an empty GroupInput"));
    }
    Ok(())
}

pub(super) fn validate_policy_envelope_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    fail: &LoweringBlock,
) -> Result<(), CheckerError> {
    let copied = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lentry_witness_v2_copy_done_")?;
    let current_hash_syscall = elf
        .syscall_addresses
        .iter()
        .copied()
        .find(|address| {
            block_for_address(record, *address).is_some_and(|block| block.owner_entry == POLICY_WRAPPER_ENTRY_ID)
                && block_for_address(record, *address).and_then(|block| register_constant_before(elf, block, *address, 17))
                    == Some(CKB_LOAD_SCRIPT_HASH)
        })
        .ok_or_else(|| policy_machine_error("missing current Script hash syscall"))?;
    let mut has_minimum = false;
    let mut has_maximum = false;
    for instruction in elf
        .instructions
        .iter()
        .filter(|instruction| copied.range.start <= instruction.address && instruction.address < current_hash_syscall)
    {
        let Some(block) = block_for_address(record, instruction.address) else { continue };
        if is_bltu(instruction.word, 5, 6)
            && register_constant_before(elf, block, instruction.address, 6) == Some(77)
            && flow_targets(elf, instruction.address, fail.range.start)
        {
            has_minimum = true;
        }
        if is_bltu(instruction.word, 6, 5)
            && register_constant_before(elf, block, instruction.address, 6) == Some(4_076)
            && flow_targets(elf, instruction.address, fail.range.start)
        {
            has_maximum = true;
        }
    }
    if !has_minimum || !has_maximum {
        return Err(policy_machine_error("policy witness bundle 77..4076-byte bounds changed"));
    }
    let magic_start = elf
        .instructions
        .iter()
        .find(|instruction| {
            copied.range.start <= instruction.address
                && instruction.address < current_hash_syscall
                && is_lbu(instruction.word, 5, 2, 8)
        })
        .map(|instruction| instruction.address)
        .ok_or_else(|| policy_machine_error("missing policy witness magic check"))?;
    for (index, byte) in POLICY_WITNESS_MAGIC.iter().copied().enumerate() {
        let address = magic_start + (index as u64) * 12;
        if !is_lbu(instruction_at(elf, address)?.word, 5, 2, 8 + index as i32)
            || !is_addi(instruction_at(elf, address + 4)?.word, 6, 0, i32::from(byte))
            || !is_bne(instruction_at(elf, address + 8)?.word, 5, 6)
            || !flow_targets(elf, address + 8, fail.range.start)
        {
            return Err(policy_machine_error("canonical CSPOLv1 witness magic changed"));
        }
    }
    let after_hash = instructions_from(elf, current_hash_syscall + 4, 7)?;
    if !is_bne(after_hash[0].word, 10, 0)
        || !flow_targets(elf, after_hash[0].address, fail.range.start)
        || !matches_large_stack_load(after_hash, 1, 5, 4_104)
        || !is_addi(after_hash[5].word, 6, 0, 32)
        || !is_bne(after_hash[6].word, 5, 6)
        || !flow_targets(elf, after_hash[6].address, fail.range.start)
    {
        return Err(policy_machine_error("current Script hash exact-width check changed"));
    }
    Ok(())
}

pub(super) fn validate_policy_record_layout_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    args_valid: &LoweringBlock,
    fail: &LoweringBlock,
) -> Result<(), CheckerError> {
    let start = elf
        .instructions
        .iter()
        .find(|instruction| {
            instruction.address < args_valid.range.start
                && block_for_address(record, instruction.address).is_some_and(|block| block.owner_entry == POLICY_WRAPPER_ENTRY_ID)
                && is_add(instruction.word, 15, 14, 5)
        })
        .map(|instruction| instruction.address)
        .ok_or_else(|| policy_machine_error("missing policy record base calculation"))?;
    let words = instructions_from(elf, start, 102)?;
    if !matches_u32_le_load(words, 1, 28, 15, 0, 29)
        || !is_bne(words[11].word, 7, 28)
        || !flow_targets(elf, words[11].address, fail.range.start)
    {
        return Err(policy_machine_error("policy record total-size validation changed"));
    }
    let mut cursor = 12;
    for (offset, expected) in [(4, 20), (8, 21), (12, 53), (16, 57)] {
        if !matches_u32_le_load(words, cursor, 28, 15, offset, 29)
            || !is_addi(words[cursor + 10].word, 29, 0, expected)
            || !is_bne(words[cursor + 11].word, 28, 29)
            || !flow_targets(elf, words[cursor + 11].address, fail.range.start)
        {
            return Err(policy_machine_error("policy record Molecule table offsets changed"));
        }
        cursor += 12;
    }
    if !matches_u32_le_load(words, cursor, 28, 15, 57, 29)
        || !is_addi(words[cursor + 10].word, 7, 7, -POLICY_RECORD_FIXED_BYTES)
        || !is_bne(words[cursor + 11].word, 7, 28)
        || !flow_targets(elf, words[cursor + 11].address, fail.range.start)
    {
        return Err(policy_machine_error("policy record args length binding changed"));
    }
    cursor += 12;
    if !is_lbu(words[cursor].word, 5, 15, 20)
        || !is_addi(words[cursor + 1].word, 6, 0, 2)
        || !is_bgeu(words[cursor + 2].word, 5, 6)
        || !flow_targets(elf, words[cursor + 2].address, fail.range.start)
        || !is_beq(words[cursor + 3].word, 28, 0)
        || !flow_targets(elf, words[cursor + 3].address, args_valid.range.start)
        || !is_addi(words[cursor + 4].word, 29, 0, ENTRY_WITNESS_MAGIC.len() as i32)
        || !is_bltu(words[cursor + 5].word, 28, 29)
        || !flow_targets(elf, words[cursor + 5].address, fail.range.start)
    {
        return Err(policy_machine_error("policy record role or empty/CSARG args framing changed"));
    }
    cursor += 6;
    for (index, byte) in ENTRY_WITNESS_MAGIC.iter().copied().enumerate() {
        if !is_lbu(words[cursor].word, 5, 15, POLICY_RECORD_FIXED_BYTES + index as i32)
            || !is_addi(words[cursor + 1].word, 6, 0, i32::from(byte))
            || !is_bne(words[cursor + 2].word, 5, 6)
            || !flow_targets(elf, words[cursor + 2].address, fail.range.start)
        {
            return Err(policy_machine_error("selected policy record no longer requires canonical CSARGv1 framing"));
        }
        cursor += 3;
    }
    if words[cursor - 1].address + 4 != args_valid.range.start {
        return Err(policy_machine_error("policy record validation admits an unchecked path before key ordering"));
    }
    Ok(())
}

pub(super) fn validate_policy_dynvec_machine_contract(
    elf: &ParsedElf,
    record_loop: &LoweringBlock,
    fail: &LoweringBlock,
) -> Result<(), CheckerError> {
    let start =
        record_loop.range.start.checked_sub(160).ok_or_else(|| policy_machine_error("policy DynVec scanner prefix underflows"))?;
    let words = instructions_from(elf, start, 40)?;
    if !matches_large_stack_store(words, 0, 0, POLICY_FOUND_OFFSET)
        || !is_addi(words[4].word, 14, 2, 16)
        || !is_ld(words[5].word, 6, 2, 0)
        || !is_addi(words[6].word, 6, 6, -8)
        || !matches_u32_le_load(words, 7, 5, 14, 0, 29)
        || !is_bne(words[17].word, 5, 6)
        || !flow_targets(elf, words[17].address, fail.range.start)
        || !is_add(words[18].word, 17, 14, 5)
        || !matches_u32_le_load(words, 19, 6, 14, 4, 29)
        || !is_addi(words[29].word, 7, 0, 8)
        || !is_bltu(words[30].word, 6, 7)
        || !flow_targets(elf, words[30].address, fail.range.start)
        || !is_addi(words[31].word, 7, 0, 36)
        || !is_bltu(words[32].word, 7, 6)
        || !flow_targets(elf, words[32].address, fail.range.start)
        || !is_addi(words[33].word, 7, 0, 3)
        || !is_and(words[34].word, 7, 6, 7)
        || !is_bne(words[35].word, 7, 0)
        || !flow_targets(elf, words[35].address, fail.range.start)
        || !is_bltu(words[36].word, 5, 6)
        || !flow_targets(elf, words[36].address, fail.range.start)
        || !is_add(words[37].word, 13, 14, 6)
        || !is_addi(words[38].word, 12, 14, 4)
        || !is_addi(words[39].word, 16, 0, 0)
        || words[39].address + 4 != record_loop.range.start
    {
        return Err(policy_machine_error("bounded one-to-eight-record canonical DynVec scanner changed"));
    }
    Ok(())
}

pub(super) fn validate_policy_key_order_machine_contract(
    elf: &ParsedElf,
    args_valid: &LoweringBlock,
    key_loop: &LoweringBlock,
    ordered: &LoweringBlock,
    fail: &LoweringBlock,
) -> Result<(), CheckerError> {
    let prefix = instructions_from(elf, args_valid.range.start, 4)?;
    if !is_beq(prefix[0].word, 16, 0)
        || !flow_targets(elf, prefix[0].address, ordered.range.start)
        || !is_addi(prefix[1].word, 5, 16, 0)
        || !is_addi(prefix[2].word, 6, 15, 20)
        || !is_addi(prefix[3].word, 7, 0, 33)
        || prefix[3].address + 4 != key_loop.range.start
    {
        return Err(policy_machine_error("policy record key-order initialization changed"));
    }
    let loop_words = instructions_from(elf, key_loop.range.start, 9)?;
    if !is_lbu(loop_words[0].word, 28, 5, 0)
        || !is_lbu(loop_words[1].word, 29, 6, 0)
        || !is_bltu(loop_words[2].word, 28, 29)
        || !flow_targets(elf, loop_words[2].address, ordered.range.start)
        || !is_bltu(loop_words[3].word, 29, 28)
        || !flow_targets(elf, loop_words[3].address, fail.range.start)
        || !is_addi(loop_words[4].word, 5, 5, 1)
        || !is_addi(loop_words[5].word, 6, 6, 1)
        || !is_addi(loop_words[6].word, 7, 7, -1)
        || !is_bne(loop_words[7].word, 7, 0)
        || !flow_targets(elf, loop_words[7].address, key_loop.range.start)
        || !is_jal_zero(loop_words[8].word)
        || !flow_targets(elf, loop_words[8].address, fail.range.start)
    {
        return Err(policy_machine_error("policy record keys are no longer strictly ordered and duplicate-rejecting"));
    }
    Ok(())
}

pub(super) fn validate_policy_selector_machine_contract(
    elf: &ParsedElf,
    record_loop: &LoweringBlock,
    ordered: &LoweringBlock,
    hash_loop: &LoweringBlock,
    next_record: &LoweringBlock,
    fail: &LoweringBlock,
) -> Result<(), CheckerError> {
    let ordered_words = instructions_from(elf, ordered.range.start, 9)?;
    if !is_addi(ordered_words[0].word, 16, 15, 20)
        || !is_lbu(ordered_words[1].word, 5, 15, 20)
        || !is_addi(ordered_words[2].word, 6, 0, POLICY_TYPE_ROLE)
        || !is_bne(ordered_words[3].word, 5, 6)
        || !flow_targets(elf, ordered_words[3].address, next_record.range.start)
        || !is_addi(ordered_words[4].word, 5, 15, 21)
        || !matches_large_stack_address(ordered_words, 5, 6, POLICY_HASH_BUFFER_OFFSET)
        || !is_addi(ordered_words[8].word, 7, 0, 32)
        || ordered_words[8].address + 4 != hash_loop.range.start
    {
        return Err(policy_machine_error("policy selector no longer binds Type role and the complete current Script hash"));
    }
    let words = instructions_from(elf, hash_loop.range.start, 56)?;
    if !is_lbu(words[0].word, 28, 5, 0)
        || !is_lbu(words[1].word, 29, 6, 0)
        || !is_bne(words[2].word, 28, 29)
        || !flow_targets(elf, words[2].address, next_record.range.start)
        || !is_addi(words[3].word, 5, 5, 1)
        || !is_addi(words[4].word, 6, 6, 1)
        || !is_addi(words[5].word, 7, 7, -1)
        || !is_bne(words[6].word, 7, 0)
        || !flow_targets(elf, words[6].address, hash_loop.range.start)
        || !matches_large_stack_load(words, 7, 5, POLICY_FOUND_OFFSET)
        || !is_bne(words[11].word, 5, 0)
        || !flow_targets(elf, words[11].address, fail.range.start)
        || !is_addi(words[12].word, 5, 0, 1)
        || !matches_large_stack_store(words, 13, 5, POLICY_FOUND_OFFSET)
        || !matches_u32_le_load(words, 17, 5, 15, 53, 29)
        || !matches_large_stack_store(words, 27, 5, POLICY_TAG_OFFSET)
        || !matches_u32_le_load(words, 31, 5, 15, 57, 29)
        || !matches_large_stack_store(words, 41, 5, POLICY_ARGS_LENGTH_OFFSET)
        || !is_addi(words[45].word, 5, 15, POLICY_RECORD_FIXED_BYTES)
        || !matches_large_stack_store(words, 46, 5, POLICY_ARGS_POINTER_OFFSET)
        || words[49].address + 4 != next_record.range.start
    {
        return Err(policy_machine_error("policy selector tag/args extraction or single-match guard changed"));
    }
    let next = instructions_from(elf, next_record.range.start, 6)?;
    if !is_bltu(next[0].word, 12, 13)
        || !flow_targets(elf, next[0].address, record_loop.range.start)
        || !matches_large_stack_load(next, 1, 5, POLICY_FOUND_OFFSET)
        || !is_beq(next[5].word, 5, 0)
        || !flow_targets(elf, next[5].address, fail.range.start)
    {
        return Err(policy_machine_error("policy scanner no longer validates every record and requires exactly one matching Script"));
    }
    Ok(())
}

pub(super) fn validate_policy_tag_dispatch_machine_contract(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    contract: &crate::PolicyWitnessContract,
    next_record: &LoweringBlock,
    fail: &LoweringBlock,
    done: &LoweringBlock,
) -> Result<(), CheckerError> {
    if contract.variants.is_empty() {
        return Err(policy_machine_error("policy machine contract has no declared variants"));
    }
    let mut variants = record
        .blocks
        .iter()
        .filter(|block| {
            block.owner_entry == POLICY_WRAPPER_ENTRY_ID
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lpolicy_variant_"))
        })
        .collect::<Vec<_>>();
    variants.sort_by_key(|block| block.range.start);
    if variants.len() != contract.variants.len() {
        return Err(policy_machine_error("machine variant count differs from the typed policy"));
    }
    let variant_addresses = variants.iter().map(|block| block.range.start).collect::<BTreeSet<_>>();
    let tag_set = contract.variants.iter().map(|variant| u64::from(variant.tag)).collect::<BTreeSet<_>>();
    let mut branches = Vec::new();
    for instruction in elf.instructions.iter().filter(|instruction| {
        next_record.range.start <= instruction.address && instruction.address < fail.range.start && is_beq(instruction.word, 5, 6)
    }) {
        let Some(block) = block_for_address(record, instruction.address) else { continue };
        let Some(tag) = register_constant_before(elf, block, instruction.address, 6) else { continue };
        let Some(flow) = elf.control_flow.iter().find(|flow| flow.address == instruction.address) else { continue };
        if tag_set.contains(&tag) {
            branches.push((instruction.address, tag, flow.target));
        }
    }
    branches.sort_by_key(|branch| branch.0);
    let has_common = !contract.common_checks.is_empty();
    let expected_count = contract.variants.len() * if has_common { 2 } else { 1 };
    if branches.len() != expected_count {
        return Err(policy_machine_error("declared tag comparisons are missing or duplicated in machine dispatch"));
    }
    let expected_tags = contract.variants.iter().map(|variant| u64::from(variant.tag)).collect::<Vec<_>>();
    let dispatch_offset = if has_common { contract.variants.len() } else { 0 };
    if branches[dispatch_offset..].iter().map(|branch| branch.1).collect::<Vec<_>>() != expected_tags {
        return Err(policy_machine_error("machine dispatch tag order differs from the canonical policy variants"));
    }
    for ((_, _, target), variant_block) in branches[dispatch_offset..].iter().zip(&variants) {
        if *target != variant_block.range.start {
            return Err(policy_machine_error("a policy tag branches to the wrong action adapter"));
        }
    }
    let dispatch_start = block_for_address(record, branches[dispatch_offset].0)
        .ok_or_else(|| policy_machine_error("dispatch branch is outside machine blocks"))?
        .range
        .start;
    let dispatch_prefix = instructions_from(elf, dispatch_start, 4)?;
    if !matches_large_stack_load(dispatch_prefix, 0, 5, POLICY_TAG_OFFSET) {
        return Err(policy_machine_error("action dispatch no longer reads the selected record tag"));
    }
    let last_dispatch =
        branches.last().map(|branch| branch.0).ok_or_else(|| policy_machine_error("policy machine dispatch has no tag branches"))?;
    if !is_jal_zero(instruction_at(elf, last_dispatch + 4)?.word) || !flow_targets(elf, last_dispatch + 4, fail.range.start) {
        return Err(policy_machine_error("unknown policy action tags no longer reject"));
    }

    if has_common {
        let declared = unique_policy_block(record, POLICY_WRAPPER_ENTRY_ID, ".Lpolicy_declared_tag_")?;
        if branches[..dispatch_offset].iter().map(|branch| branch.1).collect::<Vec<_>>() != expected_tags
            || branches[..dispatch_offset].iter().any(|branch| branch.2 != declared.range.start)
        {
            return Err(policy_machine_error("common checks are no longer guarded by the complete declared tag set"));
        }
        validate_only_branches_enter(
            record,
            declared,
            &branches[..dispatch_offset].iter().map(|branch| branch.0).collect::<Vec<_>>(),
        )?;
        let first_common_branch = branches[0].0;
        let common_guard_start = block_for_address(record, first_common_branch)
            .ok_or_else(|| policy_machine_error("common-check tag guard is outside machine blocks"))?
            .range
            .start;
        let guard_prefix = instructions_from(elf, common_guard_start, 4)?;
        if !matches_large_stack_load(guard_prefix, 0, 5, POLICY_TAG_OFFSET) {
            return Err(policy_machine_error("common checks no longer read the selected record tag"));
        }
        let last_guard = branches[dispatch_offset - 1].0;
        if !is_jal_zero(instruction_at(elf, last_guard + 4)?.word) || !flow_targets(elf, last_guard + 4, fail.range.start) {
            return Err(policy_machine_error("unknown tags can bypass common-check rejection"));
        }
        let expected_targets =
            contract.common_checks.iter().map(|entry_id| entry_start(record, entry_id)).collect::<Result<Vec<_>, _>>()?;
        let mut calls = elf
            .control_flow
            .iter()
            .filter(|flow| {
                declared.range.start <= flow.address && flow.address < dispatch_start && expected_targets.contains(&flow.target)
            })
            .collect::<Vec<_>>();
        calls.sort_by_key(|flow| flow.address);
        if calls.len() != expected_targets.len() || calls.iter().map(|flow| flow.target).collect::<Vec<_>>() != expected_targets {
            return Err(policy_machine_error("ordered common-check calls differ from the typed policy"));
        }
        for call in calls {
            if call.address < 4
                || !is_auipc(instruction_at(elf, call.address - 4)?.word, 1)
                || !is_jalr_call(instruction_at(elf, call.address)?.word)
                || !is_bne(instruction_at(elf, call.address + 4)?.word, 10, 0)
                || !flow_targets(elf, call.address + 4, done.range.start)
            {
                return Err(policy_machine_error("common-check failure no longer dominates action dispatch"));
            }
        }
        validate_only_fallthrough_enters(record, dispatch_start)?;
    } else if record.blocks.iter().any(|block| {
        block.owner_entry == POLICY_WRAPPER_ENTRY_ID
            && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lpolicy_declared_tag_"))
    }) {
        return Err(policy_machine_error("payload declares no common checks but the machine has a common-check dispatch block"));
    } else {
        validate_only_fallthrough_enters(record, dispatch_start)?;
    }

    let mut adapters = record
        .entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Runtime && entry.name.starts_with(".Lpolicy_action_adapter_"))
        .collect::<Vec<_>>();
    adapters.sort_by_key(|entry| entry_start(record, &entry.id).unwrap_or(u64::MAX));
    if adapters.len() != variants.len() {
        return Err(policy_machine_error("policy action adapter count differs from the declared variant count"));
    }
    for ((variant_block, adapter), variant) in variants.iter().zip(adapters).zip(&contract.variants) {
        let words = instructions_from(elf, variant_block.range.start, 11)?;
        let adapter_start = entry_start(record, &adapter.id)?;
        if !matches_large_stack_load(words, 0, 10, POLICY_ARGS_POINTER_OFFSET)
            || !matches_large_stack_load(words, 4, 11, POLICY_ARGS_LENGTH_OFFSET)
            || !is_auipc(words[8].word, 1)
            || !is_jalr_call(words[9].word)
            || !flow_targets(elf, words[9].address, adapter_start)
            || !is_jal_zero(words[10].word)
            || !flow_targets(elf, words[10].address, done.range.start)
            || !variant_addresses.contains(&variant_block.range.start)
            || entry_start(record, &variant.entry_id).is_err()
        {
            return Err(policy_machine_error(format!(
                "tag {} no longer forwards the selected args to its exact adapter",
                variant.tag
            )));
        }
        let branch_address = branches[dispatch_offset..]
            .iter()
            .find(|branch| branch.2 == variant_block.range.start)
            .map(|branch| branch.0)
            .ok_or_else(|| policy_machine_error("policy variant has no unique declared tag branch"))?;
        validate_only_branch_enters(record, variant_block, branch_address)?;
    }
    Ok(())
}

pub(super) fn validate_policy_action_adapters(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    contract: &crate::PolicyWitnessContract,
) -> Result<(), CheckerError> {
    let mut adapters = record
        .entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Runtime && entry.name.starts_with(".Lpolicy_action_adapter_"))
        .collect::<Vec<_>>();
    adapters.sort_by_key(|entry| entry_start(record, &entry.id).unwrap_or(u64::MAX));
    if adapters.len() != contract.variants.len() {
        return Err(policy_machine_error("policy adapter cardinality changed"));
    }
    let all_action_starts =
        contract.variants.iter().map(|variant| entry_start(record, &variant.entry_id)).collect::<Result<BTreeSet<_>, _>>()?;
    let mut shared_bindings: BTreeMap<String, (Value, BTreeSet<u64>)> = BTreeMap::new();
    for (adapter, variant) in adapters.into_iter().zip(&contract.variants) {
        let entry = record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.id == variant.entry_id)
            .ok_or_else(|| policy_machine_error("policy variant has no typed action entry"))?;
        let expected_outgoing = policy_outgoing_argument_bytes(entry, &record.typed_semantics)?;
        let (private_frame, private_capacity) = crate::policy::private_adapter_layout(entry, &record.typed_semantics)?;
        let expected_record_frame = private_frame
            .checked_add(expected_outgoing)
            .ok_or_else(|| policy_machine_error("policy adapter recorded frame overflows u32"))?;
        if adapter.frame_size_bytes != expected_record_frame || adapter.outgoing_argument_bytes != 0 {
            return Err(policy_machine_error(format!(
                "policy positional adapter frame contract changed: frame {}, outgoing {}, expected frame {expected_record_frame}, outgoing 0",
                adapter.frame_size_bytes, adapter.outgoing_argument_bytes
            )));
        }
        let blocks = record.blocks.iter().filter(|block| block.owner_entry == adapter.id).collect::<Vec<_>>();
        if blocks.iter().any(|block| block.frame_size_bytes != expected_record_frame || block.outgoing_argument_bytes != 0) {
            return Err(policy_machine_error("policy positional adapter blocks disagree on frame size"));
        }
        let base = entry_start(record, &adapter.id)?;
        let saved_ra = private_frame as i32 - 8;
        let save_address = base + sp_adjust_instruction_bytes(-(private_frame as i32));
        if !matches_sp_adjust_at(elf, base, -(private_frame as i32))? || !matches_stack_slot_at(elf, save_address, 1, saved_ra, false)?
        {
            return Err(policy_machine_error("policy adapter prologue no longer owns a bounded private copy frame"));
        }
        let decoder_call_start = save_address + stack_slot_instruction_bytes(saved_ra);
        let shared_decoder = elf.control_flow.iter().find(|flow| flow.address == decoder_call_start + 4).and_then(|flow| {
            record.entries.iter().find(|candidate| {
                candidate.name.starts_with(".Lpolicy_shared_decoder_") && entry_start(record, &candidate.id).ok() == Some(flow.target)
            })
        });
        let decoder_owner = if let Some(decoder) = shared_decoder {
            if !is_auipc(instruction_at(elf, decoder_call_start)?.word, 1)
                || !is_jalr_call(instruction_at(elf, decoder_call_start + 4)?.word)
                || decoder.kind != EntryKind::Runtime
                || decoder.frame_size_bytes != 0
                || decoder.outgoing_argument_bytes != 0
            {
                return Err(policy_machine_error("shared decoder call or borrowed-frame contract changed"));
            }
            let decoder_blocks: Vec<_> = record.blocks.iter().filter(|block| block.owner_entry == decoder.id).collect();
            if decoder_blocks.iter().any(|block| block.frame_size_bytes != 0 || block.outgoing_argument_bytes != 0)
                || elf
                    .stack_adjustments
                    .iter()
                    .any(|adjustment| decoder_blocks.iter().any(|block| block.range.contains(adjustment.address)))
                || elf.instructions.iter().any(|instruction| {
                    decoder_blocks.iter().any(|block| block.range.contains(instruction.address))
                        && matches!(instruction.word & 0x7f, 0x6f | 0x67)
                        && (instruction.word >> 7) & 31 != 0
                })
            {
                return Err(policy_machine_error("shared decoder may neither move sp nor call another function"));
            }
            let projections = entry
                .params
                .iter()
                .map(|param| crate::policy::builder_parameter_projection(param, entry, &record.typed_semantics))
                .collect::<Result<Vec<_>, _>>()?;
            let key =
                serde_json::json!({"params":entry.params,"projection":projections,"frame":private_frame,"capacity":private_capacity});
            let (expected, callers) = shared_bindings.entry(decoder.id.clone()).or_insert_with(|| (key.clone(), BTreeSet::new()));
            if *expected != key {
                return Err(policy_machine_error("different parameter contracts share one positional decoder"));
            }
            callers.insert(decoder_call_start + 4);
            let decoder_done = unique_policy_block(record, &decoder.id, ".Lentry_witness_done_")?;
            if instruction_at(elf, decoder_done.range.start)?.word != 0x0000_8067 {
                return Err(policy_machine_error("shared decoder must return with its borrowed frame intact"));
            }
            &decoder.id
        } else {
            &adapter.id
        };
        let fail = unique_policy_block(record, decoder_owner, ".Lentry_witness_fail_")?;
        let done = unique_policy_block(record, &adapter.id, ".Lentry_witness_done_")?;
        let has_payload =
            private_capacity > 0 && entry.params.iter().any(|param| matches!(param.source.as_str(), "default" | "witness"));
        if has_payload {
            let copy = unique_policy_block(record, decoder_owner, ".Lpolicy_args_copy_")?;
            let bound_instructions = if private_capacity <= 2047 || private_capacity == 4096 { 1 } else { 2 };
            let prefix_address = copy
                .range
                .start
                .checked_sub((5 + bound_instructions) * 4)
                .ok_or_else(|| policy_machine_error("policy adapter copy prefix underflows"))?;
            let prefix = instructions_from(elf, prefix_address, (5 + bound_instructions) as usize)?;
            let bound_matches = if private_capacity <= 2047 {
                is_addi(prefix[0].word, 5, 0, private_capacity as i32)
            } else if private_capacity == 4096 {
                is_lui(prefix[0].word, 5, 0x1000)
            } else {
                is_lui(prefix[0].word, 5, 0x1000) && is_addi(prefix[1].word, 5, 5, private_capacity as i32 - 4096)
            };
            let start = bound_instructions as usize;
            if !bound_matches
                || !is_bltu(prefix[start].word, 5, 11)
                || !flow_targets(elf, prefix[start].address, fail.range.start)
                || !is_sd(prefix[start + 1].word, 11, 2, 0)
                || !is_addi(prefix[start + 2].word, 5, 10, 0)
                || !is_addi(prefix[start + 3].word, 6, 2, 8)
                || !is_addi(prefix[start + 4].word, 7, 0, 0)
            {
                return Err(policy_machine_error("policy adapter no longer bounds and privately copies selected args"));
            }
            let copied = unique_policy_block(record, decoder_owner, ".Lpolicy_args_copied_")?;
            let loop_words = instructions_from(elf, copy.range.start, 7)?;
            if !is_bgeu(loop_words[0].word, 7, 11)
                || !flow_targets(elf, loop_words[0].address, copied.range.start)
                || !is_add(loop_words[1].word, 28, 5, 7)
                || !is_lbu(loop_words[2].word, 29, 28, 0)
                || !is_add(loop_words[3].word, 28, 6, 7)
                || !is_sb(loop_words[4].word, 29, 28, 0)
                || !is_addi(loop_words[5].word, 7, 7, 1)
                || !is_jal_zero(loop_words[6].word)
                || !flow_targets(elf, loop_words[6].address, copy.range.start)
            {
                return Err(policy_machine_error("policy adapter selected-args copy loop changed"));
            }
        } else {
            let empty_guard = if shared_decoder.is_some() { entry_start(record, decoder_owner)? } else { decoder_call_start };
            if !is_bne(instruction_at(elf, empty_guard)?.word, 11, 0)
                || !flow_targets(elf, empty_guard, fail.range.start)
                || !is_sd(instruction_at(elf, empty_guard + 4)?.word, 0, 2, 0)
            {
                return Err(policy_machine_error("payload-free policy adapter no longer requires empty args"));
            }
        }

        let target = entry_start(record, &variant.entry_id)?;
        let mut action_calls = elf
            .control_flow
            .iter()
            .filter(|flow| blocks.iter().any(|block| block.range.contains(flow.address)) && all_action_starts.contains(&flow.target))
            .collect::<Vec<_>>();
        action_calls.sort_by_key(|flow| flow.address);
        if action_calls.len() != 1 || action_calls[0].target != target {
            return Err(policy_machine_error(format!(
                "tag {} adapter no longer calls exactly action '{}'",
                variant.tag, variant.entry_id
            )));
        }
        let call = action_calls[0];
        if shared_decoder.is_some() {
            let outgoing_adjustment =
                if expected_outgoing == 0 { 0 } else { sp_adjust_instruction_bytes(-(expected_outgoing as i32)) };
            if call.address != decoder_call_start + 8 + outgoing_adjustment + 4 {
                return Err(policy_machine_error("shared decoder arguments must flow directly to the bound action call"));
            }
        }
        if call.address < 4
            || !is_auipc(instruction_at(elf, call.address - 4)?.word, 1)
            || !is_jalr_call(instruction_at(elf, call.address)?.word)
            || !machine_error_jumps_to_abort(record, elf, fail.range.start, 25)
        {
            return Err(policy_machine_error("policy adapter call/failure completion changed"));
        }
        let completion = if expected_outgoing == 0 {
            call.address + 4
        } else {
            let outgoing = i32::try_from(expected_outgoing)
                .map_err(|_| policy_machine_error("policy adapter outgoing argument frame exceeds i32"))?;
            let reserve_bytes = sp_adjust_instruction_bytes(-outgoing);
            let reserve = call
                .address
                .checked_sub(4 + reserve_bytes)
                .ok_or_else(|| policy_machine_error("policy adapter outgoing argument reservation underflows"))?;
            if !matches_sp_adjust_at(elf, reserve, -outgoing)? || reserve + reserve_bytes != call.address - 4 {
                return Err(policy_machine_error("policy adapter outgoing argument frame changed"));
            }
            let restore = call.address + 4;
            if !matches_sp_adjust_at(elf, restore, outgoing)? {
                return Err(policy_machine_error("policy adapter outgoing argument frame changed"));
            }
            restore + sp_adjust_instruction_bytes(outgoing)
        };
        if !is_jal_zero(instruction_at(elf, completion)?.word) || !flow_targets(elf, completion, done.range.start) {
            return Err(policy_machine_error("policy adapter call no longer completes through its exact done block"));
        }
        let restore = done.range.start + stack_slot_instruction_bytes(saved_ra);
        if !matches_stack_slot_at(elf, done.range.start, 1, saved_ra, true)?
            || !matches_sp_adjust_at(elf, restore, private_frame as i32)?
            || instruction_at(elf, restore + sp_adjust_instruction_bytes(private_frame as i32))?.word != 0x0000_8067
        {
            return Err(policy_machine_error("policy adapter completion no longer restores its private frame"));
        }
    }
    for decoder in record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")) {
        let (_, callers) = shared_bindings.get(&decoder.id).ok_or_else(|| policy_machine_error("unbound shared policy decoder"))?;
        let target = entry_start(record, &decoder.id)?;
        let incoming: BTreeSet<_> = elf.control_flow.iter().filter(|flow| flow.target == target).map(|flow| flow.address).collect();
        if callers.len() < 2 || &incoming != callers {
            return Err(policy_machine_error("shared decoder has an undeclared caller or no repeated layout"));
        }
    }
    Ok(())
}

pub(super) fn policy_outgoing_argument_bytes(entry: &TypedSemanticEntry, typed: &TypedSemanticRecord) -> Result<u32, CheckerError> {
    let mut argument_count = 0usize;
    for param in &entry.params {
        let projection = crate::policy::builder_parameter_projection(param, entry, typed)?;
        let schema_pointer = projection.get("schema_pointer_abi").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let fixed_pointer = projection.get("fixed_byte_pointer_abi").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let type_hash = projection.get("type_hash_pointer_abi").and_then(serde_json::Value::as_bool).unwrap_or(false);
        argument_count = argument_count.saturating_add(if schema_pointer {
            2 + usize::from(type_hash) * 2
        } else if fixed_pointer {
            2
        } else {
            1
        });
    }
    let bytes = argument_count.saturating_sub(8).saturating_mul(8);
    u32::try_from(if bytes == 0 { 0 } else { bytes.next_multiple_of(16) })
        .map_err(|_| policy_machine_error("policy adapter outgoing argument frame exceeds u32"))
}

pub(crate) fn entry_start(record: &VerifiedLoweringRecord, entry_id: &str) -> Result<u64, CheckerError> {
    let entry = record
        .entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .ok_or_else(|| policy_machine_error(format!("missing lowering entry '{entry_id}'")))?;
    record
        .blocks
        .iter()
        .find(|block| block.id == entry.entry_block)
        .map(|block| block.range.start)
        .ok_or_else(|| policy_machine_error(format!("missing entry block for '{entry_id}'")))
}

pub(super) fn validate_only_fallthrough_enters(record: &VerifiedLoweringRecord, target: u64) -> Result<(), CheckerError> {
    let block = block_for_address(record, target).ok_or_else(|| policy_machine_error("dispatch target is outside machine blocks"))?;
    let preceding = target
        .checked_sub(4)
        .and_then(|address| block_for_address(record, address))
        .ok_or_else(|| policy_machine_error("dispatch target has no adjacent predecessor block"))?;
    let incoming = record.edges.iter().filter(|edge| edge.to == block.id).collect::<Vec<_>>();
    if incoming.len() != 1 || incoming[0].from != preceding.id || incoming[0].kind != EdgeKind::ConditionalFallthrough {
        return Err(policy_machine_error("action dispatch has a machine path that bypasses a common check"));
    }
    Ok(())
}

pub(super) fn validate_only_branches_enter(
    record: &VerifiedLoweringRecord,
    target: &LoweringBlock,
    branch_addresses: &[u64],
) -> Result<(), CheckerError> {
    let sources = branch_addresses
        .iter()
        .map(|address| {
            block_for_address(record, *address)
                .map(|block| block.id.as_str())
                .ok_or_else(|| policy_machine_error("declared-tag branch is outside machine blocks"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let incoming = record.edges.iter().filter(|edge| edge.to == target.id).collect::<Vec<_>>();
    if sources.len() != branch_addresses.len()
        || incoming.len() != sources.len()
        || incoming.iter().any(|edge| edge.kind != EdgeKind::ConditionalTaken || !sources.contains(edge.from.as_str()))
    {
        return Err(policy_machine_error("common checks have an entry path outside the declared tag branches"));
    }
    Ok(())
}

pub(super) fn validate_only_branch_enters(
    record: &VerifiedLoweringRecord,
    target: &LoweringBlock,
    branch_address: u64,
) -> Result<(), CheckerError> {
    let source =
        block_for_address(record, branch_address).ok_or_else(|| policy_machine_error("variant branch is outside machine blocks"))?;
    let incoming = record.edges.iter().filter(|edge| edge.to == target.id).collect::<Vec<_>>();
    if incoming.len() != 1 || incoming[0].from != source.id || incoming[0].kind != EdgeKind::ConditionalTaken {
        return Err(policy_machine_error("a policy variant has an undeclared machine entry path"));
    }
    Ok(())
}

pub(super) fn matches_u32_le_load(
    words: &[crate::elf::DecodedInstruction],
    start: usize,
    dest: u32,
    base: u32,
    offset: i32,
    scratch: u32,
) -> bool {
    words.get(start..start + 10).is_some_and(|words| {
        is_lbu(words[0].word, dest, base, offset)
            && is_lbu(words[1].word, scratch, base, offset + 1)
            && is_slli(words[2].word, scratch, scratch, 8)
            && is_or(words[3].word, dest, dest, scratch)
            && is_lbu(words[4].word, scratch, base, offset + 2)
            && is_slli(words[5].word, scratch, scratch, 16)
            && is_or(words[6].word, dest, dest, scratch)
            && is_lbu(words[7].word, scratch, base, offset + 3)
            && is_slli(words[8].word, scratch, scratch, 24)
            && is_or(words[9].word, dest, dest, scratch)
    })
}

pub(super) fn matches_large_stack_address(words: &[crate::elf::DecodedInstruction], start: usize, dest: u32, offset: i32) -> bool {
    let Some((upper, lower)) = split_large_immediate(offset) else { return false };
    words.get(start..start + 3).is_some_and(|words| {
        is_lui(words[0].word, dest, upper) && is_addi(words[1].word, dest, dest, lower) && is_add(words[2].word, dest, 2, dest)
    })
}

pub(super) fn matches_large_stack_load(words: &[crate::elf::DecodedInstruction], start: usize, dest: u32, offset: i32) -> bool {
    matches_large_stack_address(words, start, 31, offset)
        && words.get(start + 3).is_some_and(|instruction| is_ld(instruction.word, dest, 31, 0))
}

pub(super) fn matches_large_stack_store(words: &[crate::elf::DecodedInstruction], start: usize, source: u32, offset: i32) -> bool {
    matches_large_stack_address(words, start, 31, offset)
        && words.get(start + 3).is_some_and(|instruction| is_sd(instruction.word, source, 31, 0))
}

pub(super) fn matches_large_sp_adjust(words: &[crate::elf::DecodedInstruction], start: usize, delta: i32) -> bool {
    let Some((upper, lower)) = split_large_immediate(delta) else { return false };
    words.get(start..start + 3).is_some_and(|words| {
        is_lui(words[0].word, 31, upper) && is_addi(words[1].word, 31, 31, lower) && is_add(words[2].word, 2, 2, 31)
    })
}

pub(super) fn sp_adjust_instruction_bytes(delta: i32) -> u64 {
    if (-2_048..=2_047).contains(&delta) {
        4
    } else {
        12
    }
}

pub(super) fn stack_slot_instruction_bytes(offset: i32) -> u64 {
    if (-2048..=2047).contains(&offset) {
        4
    } else {
        16
    }
}

pub(super) fn matches_stack_slot_at(
    elf: &ParsedElf,
    address: u64,
    register: u32,
    offset: i32,
    load: bool,
) -> Result<bool, CheckerError> {
    if (-2048..=2047).contains(&offset) {
        let word = instruction_at(elf, address)?.word;
        Ok(if load { is_ld(word, register, 2, offset) } else { is_sd(word, register, 2, offset) })
    } else {
        let words = instructions_from(elf, address, 4)?;
        Ok(if load {
            matches_large_stack_load(words, 0, register, offset)
        } else {
            matches_large_stack_store(words, 0, register, offset)
        })
    }
}

pub(super) fn matches_sp_adjust_at(elf: &ParsedElf, address: u64, delta: i32) -> Result<bool, CheckerError> {
    if (-2_048..=2_047).contains(&delta) {
        Ok(is_addi(instruction_at(elf, address)?.word, 2, 2, delta))
    } else {
        Ok(matches_large_sp_adjust(instructions_from(elf, address, 3)?, 0, delta))
    }
}

pub(super) fn split_large_immediate(value: i32) -> Option<(u32, i32)> {
    let upper = value.checked_add(2_048)?.div_euclid(4_096).checked_mul(4_096)?;
    let lower = value.checked_sub(upper)?;
    Some((upper as u32 & 0xffff_f000, lower))
}

pub(crate) fn register_constant_before(elf: &ParsedElf, block: &LoweringBlock, address: u64, register: usize) -> Option<u64> {
    let mut values = [None; 32];
    values[0] = Some(0);
    for instruction in
        elf.instructions.iter().filter(|instruction| block.range.start <= instruction.address && instruction.address < address)
    {
        update_constant_registers(&mut values, instruction.word);
    }
    values.get(register).copied().flatten()
}

pub(super) fn update_constant_registers(values: &mut [Option<u64>; 32], word: u32) {
    let Some(instruction) = crate::machine::decode(word) else {
        values.fill(None);
        values[0] = Some(0);
        return;
    };
    if instruction.is_call() {
        for (register, value) in values.iter_mut().enumerate() {
            if crate::machine::caller_saved(register as u32) {
                *value = None;
            }
        }
    }
    let Some(destination) = instruction.destination().filter(|register| *register != 0) else { return };
    // This context retains its existing finite arithmetic policy. Other allowed
    // instructions still invalidate their actual destination.
    use crate::machine::{Alu, Operand, Operation, Width};
    let interpreted = matches!(instruction.operation(), Operation::Lui { .. })
        || matches!(
            instruction.operation(),
            Operation::Alu { op: Alu::Add, .. }
                | Operation::Alu { op: Alu::Sub, right: Operand::Register(_), .. }
                | Operation::Alu { op: Alu::ShiftLeft | Alu::ShiftRight, width: Width::Xlen, right: Operand::Immediate(_), .. }
                | Operation::Alu { op: Alu::Or | Alu::And, width: Width::Xlen, right: Operand::Register(_), .. }
        );
    let value = interpreted.then(|| instruction.constant_result(0, |register| values[usize::from(register)])).flatten();
    values[usize::from(destination)] = value;
    values[0] = Some(0);
}
