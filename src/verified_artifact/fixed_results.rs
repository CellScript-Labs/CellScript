//! Capture feature-specific machine ranges. The standalone checker derives
//! the ABI from typed declarations and checks decoded bytes, not these labels.
use super::*;
use cellscript_artifact_checker::{FixedResultAbi, FixedResultCall, FixedResultSource, FIXED_RESULT_ABI_SCHEMA};

pub(super) fn bind(
    entries: &mut [LoweringEntry],
    blocks: &[LoweringBlock],
    metadata: &CompileMetadata,
    layout: &MachineLayoutEvidence,
) -> Result<()> {
    for (label, &start) in &layout.symbols {
        let Some(body) = label.strip_prefix(".Lresult_v1_") else { continue };
        if body.ends_with("_end") || body.ends_with("_receive") || body.ends_with("_received") {
            continue;
        }
        let (kind, numbers) = body.split_once('_').ok_or_else(|| boundary_error("invalid result ABI marker"))?;
        let numbers = numbers
            .split('_')
            .map(|part| part.parse::<u32>().map_err(|_| boundary_error("result ABI marker exceeds u32")))
            .collect::<Result<Vec<_>>>()?;
        let end = *layout.symbols.get(&format!("{label}_end")).ok_or_else(|| boundary_error("result ABI range has no end"))?;
        let owner = &blocks
            .iter()
            .find(|block| block.range.contains(start))
            .ok_or_else(|| boundary_error("unowned result ABI marker"))?
            .owner_entry;
        let entry = entries.iter_mut().find(|entry| &entry.id == owner).ok_or_else(|| boundary_error("missing result ABI entry"))?;
        let typed = metadata
            .typed_semantics
            .entries
            .iter()
            .find(|typed| &typed.id == owner)
            .ok_or_else(|| boundary_error("result ABI lacks typed entry"))?;
        match (kind, numbers.as_slice()) {
            ("save", &[index, offset, width, _]) => {
                if entry.fixed_result_abi.is_some() {
                    return Err(boundary_error("duplicate result pointer save"));
                }
                entry.fixed_result_abi = Some(FixedResultAbi {
                    schema: FIXED_RESULT_ABI_SCHEMA.into(),
                    type_name: typed.return_type.clone(),
                    width_bytes: width,
                    owned_frame_bytes: layout.entry_frame_sizes[&entry.name],
                    hidden_argument_index: index,
                    saved_pointer_offset: offset,
                    save_range: MachineRange { start, end },
                    copy_ranges: Vec::new(),
                    copy_sources: Vec::new(),
                });
            }
            // Copies precede saves in lexical symbol order, so collect below.
            ("copy", &[_, _, _]) => {}
            ("source", &[_, _, _, _]) => {}
            ("call", &[source_id, offset, width, index, outgoing, slot, _]) => {
                let operation = typed
                    .blocks
                    .iter()
                    .flat_map(|block| &block.operations)
                    .find(|operation| {
                        operation.call.is_some()
                            && operation
                                .destinations
                                .iter()
                                .any(|id| typed.locals.iter().any(|local| local.id == *id && local.source_id == u64::from(source_id)))
                    })
                    .ok_or_else(|| boundary_error("result ABI lacks its typed call"))?;
                let call = operation.call.as_ref().expect("matched typed call");
                let frame =
                    layout.fixed_result_frames.get(&entry.name).ok_or_else(|| boundary_error("result caller lacks frame evidence"))?;
                let receive_start = *layout
                    .symbols
                    .get(&format!("{label}_receive"))
                    .ok_or_else(|| boundary_error("result call has no receive marker"))?;
                let receive_end = *layout
                    .symbols
                    .get(&format!("{label}_received"))
                    .ok_or_else(|| boundary_error("result call has no receive end"))?;
                entry.fixed_result_calls.push(FixedResultCall {
                    schema: FIXED_RESULT_ABI_SCHEMA.into(),
                    callee: format!("helper:{}", call.target),
                    destination_local: operation.destinations[0],
                    buffer_offset: offset,
                    width_bytes: width,
                    owned_frame_bytes: layout.entry_frame_sizes[&entry.name],
                    hidden_argument_index: index,
                    outgoing_stack_bytes: outgoing,
                    pointer_slot_offset: slot,
                    setup_range: MachineRange { start, end },
                    receive_range: MachineRange { start: receive_start, end: receive_end },
                    scalar_region_bytes: frame.scalar_region_bytes,
                    buffer_region_end: frame.buffer_region_end,
                    buffers: frame.buffers.clone(),
                });
            }
            _ => return Err(boundary_error("invalid fixed result ABI marker shape")),
        }
    }
    for (label, &start) in &layout.symbols {
        if !(label.starts_with(".Lresult_v1_copy_") || label.starts_with(".Lresult_v1_source_")) || label.ends_with("_end") {
            continue;
        }
        let owner =
            &blocks.iter().find(|block| block.range.contains(start)).ok_or_else(|| boundary_error("unowned result copy"))?.owner_entry;
        let abi = entries
            .iter_mut()
            .find(|entry| &entry.id == owner)
            .and_then(|entry| entry.fixed_result_abi.as_mut())
            .ok_or_else(|| boundary_error("result copy has no saved pointer contract"))?;
        let end = layout.symbols[&format!("{label}_end")];
        if let Some(body) = label.strip_prefix(".Lresult_v1_source_") {
            let numbers = body
                .split('_')
                .map(str::parse::<u32>)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| boundary_error("invalid result source marker"))?;
            abi.copy_sources.push(FixedResultSource {
                source_local: u64::from(numbers[0]),
                base_local: u64::from(numbers[1]),
                field_offset: numbers[2],
                range: MachineRange { start, end },
            });
        } else {
            abi.copy_ranges.push(MachineRange { start, end });
        }
    }
    for entry in entries {
        if let Some(abi) = &mut entry.fixed_result_abi {
            abi.copy_ranges.sort_by_key(|range| range.start);
            abi.copy_sources.sort_by_key(|source| source.range.start);
        }
        entry.fixed_result_calls.sort_by_key(|call| call.setup_range.start);
    }
    Ok(())
}
