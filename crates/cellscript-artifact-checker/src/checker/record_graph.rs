//! Lowering schemas, bounded record graphs, entry ABI and call reachability.
use super::*;

pub(super) fn validate_record_schema(record: &VerifiedLoweringRecord) -> Result<(), CheckerError> {
    if record.schema != LOWERING_RECORD_SCHEMA || record.version != LOWERING_RECORD_VERSION {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2403UnsupportedSchema,
            format!("unsupported lowering record '{}'/{}", record.schema, record.version),
        ));
    }
    if record.claim.lowering_record != "binding-verified"
        || record.claim.machine_code != "structurally-verified"
        || record.claim.semantic_equivalence
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2403UnsupportedSchema,
            "lowering record overclaims or mislabels the v1 verification boundary",
        ));
    }
    if record.artifact_format != "RISC-V ELF" || ckb_deployment_hash_type(&record.target_profile).is_none() {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2403UnsupportedSchema,
            "v1 checker accepts only the ckb or ckb-type-hash RISC-V ELF profiles",
        ));
    }
    if record.compatibility_profile.target_profile != record.target_profile
        || record.compatibility_profile.edition != record.edition
        || record.compatibility_profile.raw_entry_witness_payload_compatible
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            "record compatibility profile disagrees with edition/target or accepts raw entry witnesses",
        ));
    }
    let source_semantics = match record.edition.as_str() {
        "2027" => Some(crate::SOURCE_SEMANTICS_2027),
        _ => None,
    };
    if source_semantics != Some(record.compatibility_profile.source_semantics.as_str()) {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2403UnsupportedSchema,
            "unsupported source edition or semantics; use Edition 2027 and regenerate the complete artifact bundle",
        ));
    }
    let profile_hash = canonical_hash("cellscript-compatibility-profile-identity-v1", &record.compatibility_profile)?;
    if profile_hash != record.compatibility_profile_hash {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            "record compatibility profile hash does not match its canonical identity",
        ));
    }
    let typed_hash = canonical_hash(TYPED_SEMANTICS_SCHEMA, &record.typed_semantics)?;
    if typed_hash != record.typed_semantics_hash {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2419TypedSemanticsInvalid,
            "typed semantic record hash does not match its canonical contents",
        ));
    }
    Ok(())
}

pub(super) fn validate_declared_limits(declared: &DeclaredLimits, budgets: &CheckerBudgets) -> Result<(), CheckerError> {
    let checks = [
        ("artifact_bytes", declared.artifact_bytes, budgets.artifact_bytes),
        ("record_bytes", declared.record_bytes, budgets.record_bytes),
        ("source_map_bytes", declared.source_map_bytes, budgets.source_map_bytes),
        ("entries", u64::from(declared.entries), u64::from(budgets.entries)),
        ("blocks", u64::from(declared.blocks), u64::from(budgets.blocks)),
        ("edges", u64::from(declared.edges), u64::from(budgets.edges)),
        ("instructions", declared.instructions, budgets.instructions),
        ("call_depth", u64::from(declared.call_depth), u64::from(budgets.call_depth)),
        ("stack_frame_bytes", u64::from(declared.stack_frame_bytes), u64::from(budgets.stack_frame_bytes)),
        ("proof_records", u64::from(declared.proof_records), u64::from(budgets.proof_records)),
        ("source_map_intervals", u64::from(declared.source_map_intervals), u64::from(budgets.source_map_intervals)),
        ("diagnostic_bytes", u64::from(declared.diagnostic_bytes), u64::from(budgets.diagnostic_bytes)),
    ];
    for (name, value, limit) in checks {
        if value > limit {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2400BudgetExceeded,
                format!("record-declared {name} limit {value} exceeds checker policy {limit}"),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_counts(
    record: &VerifiedLoweringRecord,
    source_map: &SourceArtifactMap,
    budgets: &CheckerBudgets,
) -> Result<(), CheckerError> {
    ensure_count("entries", record.entries.len(), budgets.entries)?;
    ensure_count("blocks", record.blocks.len(), budgets.blocks)?;
    ensure_count("edges", record.edges.len(), budgets.edges)?;
    ensure_count("proof records", record.proof_records.len(), budgets.proof_records)?;
    ensure_count("verifier failure exits", record.verifier_failure_exits.len(), budgets.blocks)?;
    ensure_count("source-map intervals", source_map.intervals.len(), budgets.source_map_intervals)?;
    if artifact_declared_too_large(record.artifact_size_bytes, budgets.artifact_bytes) {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2400BudgetExceeded,
            "record-declared artifact size exceeds checker policy",
        ));
    }
    Ok(())
}

pub(super) fn validate_record_graph(record: &VerifiedLoweringRecord, budgets: &CheckerBudgets) -> Result<(), CheckerError> {
    if record.entries.is_empty() || record.blocks.is_empty() || record.text_range.is_empty() {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2405ReferentialIntegrity,
            "lowering record requires at least one entry, one block, and a non-empty text range",
        ));
    }
    ensure_sorted_unique(&record.entries, |entry| entry.id.as_str(), "entry")?;
    ensure_sorted_unique(&record.blocks, |block| block.id.as_str(), "block")?;
    ensure_sorted_unique(&record.proof_records, |proof| proof.id.as_str(), "proof")?;
    if !record.edges.windows(2).all(|pair| (&pair[0].from, &pair[0].kind, &pair[0].to) < (&pair[1].from, &pair[1].kind, &pair[1].to)) {
        return Err(CheckerError::new(CheckerRejectionCode::V2404CanonicalOrder, "lowering edges are not strictly sorted and unique"));
    }

    let entries = record.entries.iter().map(|entry| (entry.id.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let blocks = record.blocks.iter().map(|block| (block.id.as_str(), block)).collect::<BTreeMap<_, _>>();
    let proofs = record.proof_records.iter().map(|proof| (proof.id.as_str(), proof)).collect::<BTreeMap<_, _>>();
    for entry in &record.entries {
        let Some(block) = blocks.get(entry.entry_block.as_str()) else {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2405ReferentialIntegrity,
                format!("entry '{}' references missing block '{}'", entry.id, entry.entry_block),
            ));
        };
        if block.owner_entry != entry.id {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2405ReferentialIntegrity,
                format!("entry '{}' begins in block owned by '{}'", entry.id, block.owner_entry),
            ));
        }
        validate_entry_abi(entry, budgets)?;
        if !strictly_sorted(&entry.capabilities)
            || entry.capabilities.iter().any(String::is_empty)
            || !strictly_sorted(&entry.proof_ids)
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2404CanonicalOrder,
                format!("entry '{}' has non-canonical capabilities or ProofPlan links", entry.id),
            ));
        }
        for proof_id in &entry.proof_ids {
            let Some(proof) = proofs.get(proof_id.as_str()) else {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2408ProofCoverageInvalid,
                    format!("entry '{}' references missing proof '{}'", entry.id, proof_id),
                ));
            };
            if proof.entry_id != entry.id {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2408ProofCoverageInvalid,
                    format!("proof '{}' is not owned by entry '{}'", proof_id, entry.id),
                ));
            }
        }
    }
    for proof in &record.proof_records {
        if !entries.contains_key(proof.entry_id.as_str())
            || proof.obligation.is_empty()
            || !matches!(
                proof.evidence_tier.as_str(),
                "checked-static"
                    | "checked-runtime"
                    | "trusted-external"
                    | "runtime-helper-required"
                    | "builder-evidence-required"
                    | "metadata-only"
                    | "chain-evidence-required"
            )
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2408ProofCoverageInvalid,
                format!("proof '{}' has an invalid owner or empty enforcement fields", proof.id),
            ));
        }
    }

    if !record
        .runtime_error_exits
        .windows(2)
        .all(|pair| (&pair[0].block_id, pair[0].code, pair[0].address) < (&pair[1].block_id, pair[1].code, pair[1].address))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2404CanonicalOrder,
            "runtime-error exits are not strictly sorted and unique",
        ));
    }
    for exit in &record.runtime_error_exits {
        if exit.code <= 0
            || exit.code > 255
            || exit.name.is_empty()
            || blocks.get(exit.block_id.as_str()).is_none_or(|block| !block.range.contains(exit.address))
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2406CfgInvalid,
                format!("runtime-error exit {} ({}) is outside its declared block", exit.code, exit.name),
            ));
        }
    }

    let mut expected_start = record.text_range.start;
    for block in &record.blocks {
        if !entries.contains_key(block.owner_entry.as_str()) {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2405ReferentialIntegrity,
                format!("block '{}' has missing owner '{}'", block.id, block.owner_entry),
            ));
        }
        if block.range.start != expected_start || block.range.is_empty() || block.range.start % 4 != 0 || block.range.end % 4 != 0 {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2406CfgInvalid,
                format!("block '{}' does not form aligned contiguous text coverage", block.id),
            ));
        }
        expected_start = block.range.end;
        validate_block_abi(block, entries[block.owner_entry.as_str()], &proofs, budgets)?;
    }
    if expected_start != record.text_range.end {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2406CfgInvalid,
            "lowering blocks do not cover the declared text range exactly",
        ));
    }

    let mut outgoing = BTreeMap::<&str, Vec<&LoweringEdge>>::new();
    for edge in &record.edges {
        if !blocks.contains_key(edge.from.as_str()) || !blocks.contains_key(edge.to.as_str()) {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2405ReferentialIntegrity,
                format!("edge '{} -> {}' references a missing block", edge.from, edge.to),
            ));
        }
        outgoing.entry(edge.from.as_str()).or_default().push(edge);
    }
    for block in &record.blocks {
        validate_terminator_edges(block, outgoing.get(block.id.as_str()).map(Vec::as_slice).unwrap_or(&[]))?;
    }
    validate_reachability(record, &outgoing)?;
    validate_call_graph(record, &entries, &blocks, budgets.call_depth)?;
    Ok(())
}

pub(super) fn validate_entry_abi(entry: &LoweringEntry, budgets: &CheckerBudgets) -> Result<(), CheckerError> {
    if entry.name.is_empty()
        || entry.return_type.is_empty()
        || entry.frame_size_bytes > budgets.stack_frame_bytes
        || entry.outgoing_argument_bytes > entry.frame_size_bytes
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2407AbiOrStackInvalid,
            format!("entry '{}' has an invalid name/frame/outgoing-argument area", entry.id),
        ));
    }
    let mut expected_index = 0u32;
    for param in &entry.params {
        if param.index != expected_index
            || param.name.is_empty()
            || param.ty.is_empty()
            || param.width_bytes == 0
            || !valid_alignment(param.alignment_bytes)
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2407AbiOrStackInvalid,
                format!("entry '{}' has an invalid typed parameter at index {}", entry.id, param.index),
            ));
        }
        expected_index = expected_index.saturating_add(1);
    }
    Ok(())
}

pub(super) fn validate_block_abi(
    block: &LoweringBlock,
    entry: &LoweringEntry,
    proofs: &BTreeMap<&str, &ProofRecord>,
    budgets: &CheckerBudgets,
) -> Result<(), CheckerError> {
    if block.allocated_init_labels.len() > 16
        || !strictly_sorted(&block.allocated_init_labels)
        || block.allocated_init_labels.iter().any(|label| label.len() > 128 || !label.starts_with(".Lallocated_init_"))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "invalid or excessive allocated initialization labels",
        ));
    }
    if block.allocated_allocation_labels.len() > 16
        || !strictly_sorted(&block.allocated_allocation_labels)
        || block.allocated_allocation_labels.iter().any(|label| label.len() > 128 || !label.starts_with(".Lallocated_allocation_"))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "invalid or excessive allocated allocation labels",
        ));
    }
    if block.allocated_lifecycle_labels.len() > 16
        || !strictly_sorted(&block.allocated_lifecycle_labels)
        || block.allocated_lifecycle_labels.iter().any(|label| {
            label.len() > 128
                || ![
                    ".Lallocated_resize_",
                    ".Lallocated_release_",
                    ".Lallocated_reserve_",
                    ".Lallocated_drop_",
                    ".Lallocated_push_",
                    ".Lallocated_insert_",
                    ".Lallocated_set_",
                    ".Lallocated_remove_",
                    ".Lallocated_find_",
                    ".Lallocated_put_",
                    ".Lallocated_erase_",
                    ".Lallocated_read_",
                    ".Lallocated_lookup_",
                    ".Lallocated_take_",
                    ".Lallocated_pop_",
                    ".Lallocated_required_",
                    ".Lallocated_sorted_",
                    ".Lallocated_len_",
                    ".Lallocated_capacity_",
                    ".Lallocated_empty_",
                    ".Lallocated_borrowed_",
                    ".Lallocated_args_",
                    ".Lallocated_scalar_",
                    ".Lallocated_branch_",
                    ".Lallocated_return_",
                    ".Lallocated_call_",
                    ".Lallocated_index_",
                    ".Lallocated_result_",
                ]
                .iter()
                .any(|prefix| label.starts_with(prefix))
        })
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "invalid or excessive allocated lifecycle labels",
        ));
    }
    if block.cell_view_value_labels.len() > 16
        || !strictly_sorted(&block.cell_view_value_labels)
        || block.cell_view_value_labels.iter().any(|label| label.len() > 128 || !label.starts_with(".Lcell_view_value_"))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "invalid or excessive Cell view value labels",
        ));
    }
    if block.exact_role_labels.len() > 16
        || !strictly_sorted(&block.exact_role_labels)
        || block.exact_role_labels.iter().any(|label| {
            label.len() > 128
                || !(label.starts_with(".Lexact_role_check_")
                    || label.starts_with(".Lexact_role_use_")
                    || label.starts_with(".Lexact_role_data_"))
        })
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "invalid or excessive exact-role verification labels",
        ));
    }
    if block.frame_size_bytes != entry.frame_size_bytes
        || block.outgoing_argument_bytes != entry.outgoing_argument_bytes
        || block.capabilities != entry.capabilities
        || block.frame_size_bytes > budgets.stack_frame_bytes
        || block.outgoing_argument_bytes > block.frame_size_bytes
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2407AbiOrStackInvalid,
            format!("block '{}' frame contract disagrees with owner entry", block.id),
        ));
    }
    if block.nested_read_labels.len() > 16
        || !strictly_sorted(&block.nested_read_labels)
        || block.nested_read_labels.iter().any(|label| label.len() > 128 || !label.starts_with(".Lnested_use_"))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "invalid or excessive nested read boundary labels",
        ));
    }
    if block.nested_cell_labels.len() > 16
        || !strictly_sorted(&block.nested_cell_labels)
        || block.nested_cell_labels.iter().any(|label| label.len() > 128 || !label.starts_with(".Lnested_cell_codec_"))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "invalid or excessive nested Cell decoder labels",
        ));
    }
    if block.nested_output_labels.len() > 16
        || !strictly_sorted(&block.nested_output_labels)
        || block.nested_output_labels.iter().any(|label| label.len() > 128 || !label.starts_with(".Lnested_output_check_"))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "invalid or excessive nested output comparison labels",
        ));
    }
    let valid_registers = [
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "s2", "s3",
        "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
    ];
    if !strictly_sorted(&block.scratch_register_avoid)
        || block.scratch_register_avoid.iter().any(|register| !valid_registers.contains(&register.as_str()))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2407AbiOrStackInvalid,
            format!("block '{}' has invalid scratch-register declarations", block.id),
        ));
    }
    let mut last_end = block.outgoing_argument_bytes;
    for slot in &block.stack_slots {
        if slot.name.is_empty()
            || slot.width_bytes == 0
            || !valid_alignment(slot.alignment_bytes)
            || slot.offset % slot.alignment_bytes != 0
            || slot.offset < last_end
            || slot.offset.saturating_add(slot.width_bytes) > block.frame_size_bytes
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2407AbiOrStackInvalid,
                format!("block '{}' has overlapping, misaligned, or out-of-frame stack slot '{}'", block.id, slot.name),
            ));
        }
        last_end = slot.offset.saturating_add(slot.width_bytes);
    }
    if !strictly_sorted(&block.proof_ids)
        || block.proof_ids.iter().any(|proof_id| proofs.get(proof_id.as_str()).is_none_or(|proof| proof.entry_id != block.owner_entry))
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2408ProofCoverageInvalid,
            format!("block '{}' has invalid ProofPlan links", block.id),
        ));
    }
    Ok(())
}

pub(super) fn validate_reachability(
    record: &VerifiedLoweringRecord,
    outgoing: &BTreeMap<&str, Vec<&LoweringEdge>>,
) -> Result<(), CheckerError> {
    let mut reachable = BTreeSet::new();
    let mut pending = record.entries.iter().map(|entry| entry.entry_block.as_str()).collect::<Vec<_>>();
    while let Some(block_id) = pending.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        if let Some(edges) = outgoing.get(block_id) {
            pending.extend(edges.iter().map(|edge| edge.to.as_str()));
        }
    }
    if let Some(block) = record.blocks.iter().find(|block| block.reachable != reachable.contains(block.id.as_str())) {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2406CfgInvalid,
            format!(
                "block '{}' declared reachable={} but CFG reachability is {}",
                block.id,
                block.reachable,
                reachable.contains(block.id.as_str())
            ),
        ));
    }
    Ok(())
}

pub(super) fn validate_terminator_edges(block: &LoweringBlock, edges: &[&LoweringEdge]) -> Result<(), CheckerError> {
    let non_call = edges.iter().filter(|edge| edge.kind != EdgeKind::Call).map(|edge| edge.kind).collect::<Vec<_>>();
    let valid = match block.terminator {
        MachineTerminator::Fallthrough => non_call == [EdgeKind::Fallthrough],
        MachineTerminator::Jump => non_call == [EdgeKind::Jump],
        MachineTerminator::ConditionalBranch => {
            non_call == [EdgeKind::ConditionalTaken, EdgeKind::ConditionalFallthrough]
                || non_call == [EdgeKind::ConditionalFallthrough, EdgeKind::ConditionalTaken]
        }
        MachineTerminator::Return => non_call.is_empty(),
    };
    if !valid {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2406CfgInvalid,
            format!("block '{}' terminator does not match its CFG edges", block.id),
        ));
    }
    Ok(())
}

pub(super) fn validate_call_graph(
    record: &VerifiedLoweringRecord,
    entries: &BTreeMap<&str, &LoweringEntry>,
    blocks: &BTreeMap<&str, &LoweringBlock>,
    max_depth: u32,
) -> Result<(), CheckerError> {
    let mut graph = BTreeMap::<&str, BTreeSet<&str>>::new();
    for edge in record.edges.iter().filter(|edge| edge.kind == EdgeKind::Call) {
        let from = blocks[edge.from.as_str()].owner_entry.as_str();
        let to = blocks[edge.to.as_str()].owner_entry.as_str();
        if from != to {
            graph.entry(from).or_default().insert(to);
        }
    }
    let mut depth_memo = BTreeMap::new();
    for root in entries.keys() {
        let mut active = BTreeSet::new();
        let depth = deepest_call_depth(root, &graph, &mut active, &mut depth_memo)?;
        if depth.saturating_add(1) > max_depth {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2400BudgetExceeded,
                format!("static call depth exceeds checker budget {max_depth}"),
            ));
        }
    }
    Ok(())
}

/// Longest call-chain depth below `current`, memoised per entry. Path
/// enumeration over compact DAGs is exponential; the memo keeps validation
/// linear in edges while `active` still detects recursion cycles.
pub(super) fn deepest_call_depth<'a>(
    current: &'a str,
    graph: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    active: &mut BTreeSet<&'a str>,
    depth_memo: &mut BTreeMap<&'a str, u32>,
) -> Result<u32, CheckerError> {
    if let Some(depth) = depth_memo.get(current) {
        return Ok(*depth);
    }
    if !active.insert(current) {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2418RecursionPolicyInvalid,
            format!("recursive call cycle reaches entry '{current}'"),
        ));
    }
    let mut deepest = 0u32;
    if let Some(children) = graph.get(current) {
        for child in children {
            let child_depth = deepest_call_depth(child, graph, active, depth_memo)?;
            deepest = deepest.max(child_depth.saturating_add(1));
        }
    }
    active.remove(current);
    depth_memo.insert(current, deepest);
    Ok(deepest)
}
