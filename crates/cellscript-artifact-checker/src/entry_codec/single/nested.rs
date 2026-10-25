//! Experimental dynamic entry transport. Reuses independently checked single
//! and policy envelopes; no interface admission is granted here.
use super::*;
mod script_args;

enum Shape {
    Runtime,
    Dynamic,
    Fixed { width: u32, pointer: bool },
}

struct Argument {
    shape: Shape,
    abi: u32,
    words: u32,
    signed: bool,
    script_args: bool,
}

pub(crate) fn verify(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    let contract = &record.typed_semantics.foundation.entry_contract;
    match &contract.dispatch {
        EntryDispatchContract::SingleEntry => {
            let entry = record
                .typed_semantics
                .entries
                .iter()
                .find(|entry| entry.id == contract.exact_entry)
                .ok_or_else(|| invalid("single entry has no typed owner"))?;
            if !entry.params.iter().any(|param| param.ty.contains("NestedValues<")) {
                return Ok(());
            }
            let wrapper = record
                .entries
                .iter()
                .find(|entry| entry.name == "_cellscript_entry")
                .ok_or_else(|| invalid("missing nested single-entry wrapper"))?;
            verify_transport(record, elf, entry, wrapper, false)
        }
        EntryDispatchContract::PolicyWitnessV1(policy) => {
            // The ordinary policy checker has already bound the outer envelope,
            // selector and actual selected record to this ordered adapter set.
            let mut adapters = record
                .entries
                .iter()
                .filter(|entry| entry.kind == crate::EntryKind::Runtime && entry.name.starts_with(".Lpolicy_action_adapter_"))
                .collect::<Vec<_>>();
            adapters.sort_by_key(|entry| entry_start(record, &entry.id).unwrap_or(u64::MAX));
            if adapters.len() != policy.variants.len() {
                return Err(invalid("nested policy adapter cardinality differs"));
            }
            for (adapter, variant) in adapters.into_iter().zip(&policy.variants) {
                let entry = record
                    .typed_semantics
                    .entries
                    .iter()
                    .find(|entry| entry.id == variant.entry_id)
                    .ok_or_else(|| invalid("nested policy variant has no typed owner"))?;
                verify_transport(record, elf, entry, adapter, true)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn verify_transport(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &TypedSemanticEntry,
    wrapper: &crate::LoweringEntry,
    policy: bool,
) -> Result<(), CheckerError> {
    let contract = &record.typed_semantics.foundation.entry_contract;
    if !entry.params.iter().any(|param| param.ty.contains("NestedValues<")) {
        return Ok(());
    }
    let has_script_args = entry.params.iter().any(|param| param.source == "lockargs");
    let mut arguments = Vec::new();
    let mut abi = 0u32;
    let mut minimum = 8u32;
    for param in &entry.params {
        let projection = crate::policy::builder_parameter_projection(param, entry, &record.typed_semantics)?;
        let ty = crate::checker::canonical_abi_type(&param.ty);
        let words = crate::fixed_results::parameter_word_width(record, entry, param);
        let shape = if projection["cell_bound_abi"] == true || param.reference {
            Shape::Runtime
        } else if projection["fixed_byte_pointer_abi"] == true {
            let width = projection["fixed_byte_len"]
                .as_u64()
                .and_then(|width| u32::try_from(width).ok())
                .ok_or_else(|| invalid("missing fixed pointer width"))?;
            Shape::Fixed { width, pointer: true }
        } else if projection["schema_pointer_abi"] == true {
            // Temporal nominal primitives use one scalar ABI word; ordinary
            // nominal values use the separate pointer/length representation.
            if words == 1 {
                Shape::Fixed { width: 8, pointer: false }
            } else {
                Shape::Dynamic
            }
        } else {
            let width = (ty == "unit")
                .then_some(0)
                .or_else(|| crate::checker::committed_state_type_width(&ty, &record.typed_semantics.types))
                .filter(|width| *width <= 8)
                .and_then(|width| u32::try_from(width).ok())
                .ok_or_else(|| invalid("unknown scalar transport width"))?;
            Shape::Fixed { width, pointer: false }
        };
        let width = match shape {
            Shape::Runtime => 0,
            Shape::Dynamic => 4,
            Shape::Fixed { width, .. } => width,
        };
        let script_args = param.source == "lockargs";
        if !script_args {
            minimum = minimum.checked_add(width).ok_or_else(|| invalid("witness minimum overflows"))?;
        }
        arguments.push(Argument { shape, abi, words, signed: ty == "i32", script_args });
        abi = abi.checked_add(words).ok_or_else(|| invalid("argument count overflows"))?;
    }
    let outgoing = abi
        .saturating_sub(8)
        .checked_mul(8)
        .and_then(|bytes| bytes.checked_add(15))
        .map(|bytes| bytes & !15)
        .ok_or_else(|| invalid("outgoing frame overflows"))?;
    if outgoing > 2047 || minimum > 4096 {
        return Err(invalid("nested transport exceeds the bounded witness/frame profile"));
    }
    if wrapper.frame_size_bytes != 5376 + outgoing || wrapper.outgoing_argument_bytes != 0 {
        return Err(invalid("nested wrapper frame does not bind its outgoing arguments"));
    }
    if !policy
        && (contract.entry_payload_abi != "cellscript-entry-witness-v1"
            || contract.witness_placement_abi != "cellscript-witnessargs-input-type-v2"
            || contract.witness_placement_field != "input_type"
            || contract.witness_placement_source != "group-input-0-then-group-output-0")
    {
        return Err(invalid("nested witness placement differs"));
    }
    let start = entry_start(record, &wrapper.id)?;
    let owned = record.blocks.iter().filter(|block| block.owner_entry == wrapper.id).collect::<Vec<_>>();
    if elf.control_flow.iter().any(|flow| {
        owned.iter().any(|block| block.range.contains(flow.target))
            && !owned.iter().any(|block| block.range.contains(flow.address))
            && flow.target != start
    }) {
        return Err(invalid("incoming edge bypasses nested witness decoding"));
    }
    let label = |name: &str| unique_policy_block(record, &wrapper.id, name).map(|block| block.range.start);
    let fail = label(".Lentry_witness_fail_")?;
    let mut c = Cursor { elf, address: start };
    c.adjust(-5376)?;
    c.ra(false)?;
    if policy {
        // Own a disjoint bounded copy of precisely the selected record before
        // decoding any ABI words. Check every loop instruction and both edges.
        let copy = label(".Lpolicy_args_copy_")?;
        let copied = label(".Lpolicy_args_copied_")?;
        c.li(5, 4096)?;
        c.b(5, 11, 6, fail)?;
        c.store(11, 2, 0, false)?;
        c.i(5, 10, 0)?;
        c.i(6, 2, 8)?;
        c.li(7, 0)?;
        c.at(copy)?;
        c.b(7, 11, 7, copied)?;
        c.r(28, 5, 7, 0)?;
        c.load(29, 28, 0, true)?;
        c.r(28, 6, 7, 0)?;
        c.store(29, 28, 0, true)?;
        c.i(7, 7, 1)?;
        c.jump(copy)?;
        c.at(copied)?;
    }
    if has_script_args {
        script_args::load(&mut c, fail)?;
    }
    if !policy {
        load_witness(&mut c, fail, &label)?;
    }
    c.expect(|word| is_ld(word, 5, 2, 0) || is_addi(word, 5, 6, 0))?;
    c.li(6, u64::from(minimum))?;
    c.r(7, 5, 6, 0x3000)?;
    guard(&mut c, fail)?;
    for (index, byte) in b"CSARGv1\0".iter().enumerate() {
        c.load(5, 2, 8 + index as i32, true)?;
        c.li(6, u64::from(*byte))?;
        c.r(7, 5, 6, 0x40000000)?;
        c.b(7, 0, 1, fail)?;
    }
    c.load(30, 2, 0, false)?;
    c.li(31, 8)?;
    for argument in arguments {
        if argument.script_args {
            script_args::argument(&mut c, &argument, outgoing, fail)?;
            continue;
        }
        match argument.shape {
            Shape::Runtime => {
                for index in 0..argument.words {
                    immediate_arg(&mut c, argument.abi + index, 0, outgoing)?;
                }
            }
            Shape::Dynamic => {
                c.i(6, 31, 4)?;
                c.r(7, 30, 6, 0x3000)?;
                guard(&mut c, fail)?;
                c.r(5, 2, 31, 0)?;
                c.i(5, 5, 8)?;
                c.li(29, 0)?;
                for index in 0..4 {
                    c.load(6, 5, index, true)?;
                    if index != 0 {
                        c.shift(6, 6, index as u32 * 8, false)?;
                    }
                    c.r(29, 29, 6, 0x6000)?;
                }
                c.i(6, 31, 4)?;
                c.r(6, 6, 29, 0)?;
                c.r(7, 30, 6, 0x3000)?;
                guard(&mut c, fail)?;
                c.r(5, 2, 31, 0)?;
                c.i(5, 5, 12)?;
                register_arg(&mut c, argument.abi, 5, outgoing)?;
                register_arg(&mut c, argument.abi + 1, 29, outgoing)?;
                c.i(31, 31, 4)?;
                c.r(31, 31, 29, 0)?;
                for index in 2..argument.words {
                    immediate_arg(&mut c, argument.abi + index, 0, outgoing)?;
                }
            }
            Shape::Fixed { width, pointer } => {
                let width = i32::try_from(width).map_err(|_| invalid("fixed parameter width exceeds i32"))?;
                c.i(6, 31, width)?;
                c.r(7, 30, 6, 0x3000)?;
                guard(&mut c, fail)?;
                c.r(5, 2, 31, 0)?;
                c.i(5, 5, 8)?;
                if pointer {
                    register_arg(&mut c, argument.abi, 5, outgoing)?;
                    immediate_arg(&mut c, argument.abi + 1, width as u64, outgoing)?;
                } else {
                    let dest = if argument.abi < 8 { 10 + argument.abi } else { 28 };
                    c.li(dest, 0)?;
                    for index in 0..width {
                        c.load(6, 5, index, true)?;
                        if index != 0 {
                            c.shift(6, 6, index as u32 * 8, false)?;
                        }
                        c.r(dest, dest, 6, 0x6000)?;
                    }
                    if argument.signed {
                        c.shift(dest, dest, 32, false)?;
                        c.expect(|word| word == (0x42005013 | dest << 15 | dest << 7))?;
                    }
                    if argument.abi >= 8 {
                        register_arg(&mut c, argument.abi, dest, outgoing)?;
                    }
                }
                c.i(31, 31, width)?;
            }
        }
    }
    c.load(30, 2, 0, false)?;
    c.r(7, 30, 31, 0x40000000)?;
    guard(&mut c, fail)?;
    if has_script_args {
        script_args::finish(&mut c, fail)?;
    }
    if outgoing != 0 {
        c.i(2, 2, -(outgoing as i32))?;
    }
    c.expect(|word| is_auipc(word, 1))?;
    let call = c.address;
    let target = entry_start(record, &entry.id)?;
    c.branch(is_jalr_call, target)?;
    // Every invocation of this entry must pass this verified transport.
    if elf.control_flow.iter().any(|flow| flow.target == target && flow.address != call) {
        return Err(invalid("nested entry has an unverified caller"));
    }
    if outgoing != 0 {
        c.i(2, 2, outgoing as i32)?;
    }
    let done = label(".Lentry_witness_done_")?;
    c.jump(done)?;
    c.at(fail)?;
    c.li(10, 25)?;
    let abort = record.entries.iter().find(|entry| entry.name == "__cellscript_abort").ok_or_else(|| invalid("missing abort"))?;
    c.jump(entry_start(record, &abort.id)?)?;
    c.at(done)?;
    c.ra(true)?;
    c.adjust(5376)?;
    c.expect(|word| word == 0x00008067)?;
    if owned.iter().any(|block| block.range.start < start || block.range.end > c.address) {
        return Err(invalid("unchecked code in nested entry wrapper"));
    }
    Ok(())
}

fn guard(c: &mut Cursor<'_>, fail: u64) -> Result<(), CheckerError> {
    c.b(7, 0, 0, c.address + 8)?;
    c.jump(fail)
}

fn register_arg(c: &mut Cursor<'_>, index: u32, source: u32, outgoing: u32) -> Result<(), CheckerError> {
    if index < 8 {
        c.i(10 + index, source, 0)
    } else {
        c.store(source, 2, ((index - 8) * 8) as i32 - outgoing as i32, false)
    }
}

fn immediate_arg(c: &mut Cursor<'_>, index: u32, value: u64, outgoing: u32) -> Result<(), CheckerError> {
    if index < 8 {
        c.li(10 + index, value)
    } else {
        c.li(5, value)?;
        register_arg(c, index, 5, outgoing)
    }
}
