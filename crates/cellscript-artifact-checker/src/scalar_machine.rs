//! Bind scalar definitions in bounded collection, Cell and nested-view bodies.
//! Successful typed-block exits must execute scalar definitions and checked
//! collection/Cell operations in source order. This
//! does not prove parameter decoding, aggregate producers or value preservation
//! across arbitrary intervening writes.
use crate::cell_selection::machine::{cursor::Cursor, flow};
use crate::{
    CheckerError, CheckerRejectionCode, ParsedElf, TypedSemanticConstant, TypedSemanticEntry, TypedSemanticLocal,
    TypedSemanticOperand, TypedSemanticOperation, TypedSemanticOperationDetail, VerifiedLoweringRecord,
};
use std::collections::BTreeMap;

mod branches;
mod comparisons;
mod properties;
mod returns;

pub(crate) use properties::call_sources as property_call_sources;
pub(crate) use properties::CallSource as PropertyCallSource;

pub(crate) const PREFIX: &str = ".Lallocated_scalar_";

struct Machine<'a> {
    words: &'a BTreeMap<u64, u32>,
    targets: &'a BTreeMap<u64, u64>,
    abort: u64,
    properties: BTreeMap<properties::Property, u64>,
}

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("scalar machine contract: {}", message.into()))
}

pub(crate) fn covered_entry(entry: &TypedSemanticEntry) -> bool {
    entry.locals.iter().any(|local| {
        let ty = local.ty.strip_prefix('&').unwrap_or(&local.ty);
        crate::checker::is_nested_source_type(ty)
            || ["InputView<", "OutputView<", "CellDataView<", "ExactCellSelection<"].iter().any(|prefix| ty.starts_with(prefix))
    }) || entry.blocks.iter().flat_map(|block| &block.operations).any(|op| {
        matches!(
            op.detail,
            TypedSemanticOperationDetail::AllocatedValue { .. } | TypedSemanticOperationDetail::ExactRoleSelection { .. }
        )
    })
}

fn scalar(ty: &str) -> bool {
    matches!(ty, "bool" | "u8" | "u16" | "u32" | "i32" | "u64")
}

fn literal(value: &TypedSemanticConstant) -> Result<u64, CheckerError> {
    match value {
        TypedSemanticConstant::Bool(value) => Ok(u64::from(*value)),
        TypedSemanticConstant::U8(value)
        | TypedSemanticConstant::U16(value)
        | TypedSemanticConstant::U32(value)
        | TypedSemanticConstant::U64(value) => value.parse().map_err(|_| invalid("invalid scalar definition constant")),
        _ => Err(invalid("non-scalar definition constant")),
    }
}

fn covered(op: &TypedSemanticOperation, locals: &BTreeMap<u32, &TypedSemanticLocal>) -> bool {
    if properties::property(op).is_some() {
        return true;
    }
    if op.destinations.len() != 1
        || !locals.get(&op.destinations[0]).is_some_and(|local| scalar(&local.ty))
        || op.operands.iter().any(|operand| !scalar(&operand.ty))
    {
        return false;
    }
    match (&*op.opcode, &op.detail) {
        ("load-const", TypedSemanticOperationDetail::Constant { value }) => literal(value).is_ok(),
        ("move", _) => true,
        ("unary", TypedSemanticOperationDetail::UnaryOperator { operator }) => matches!(&**operator, "neg" | "not"),
        ("binary", TypedSemanticOperationDetail::BinaryOperator { operator }) => matches!(
            &**operator,
            "add"
                | "sub"
                | "mul"
                | "div"
                | "mod"
                | "and"
                | "or"
                | "bit-and"
                | "bit-or"
                | "bit-xor"
                | "shl"
                | "shr"
                | "eq"
                | "ne"
                | "lt"
                | "le"
                | "gt"
                | "ge"
        ),
        _ => false,
    }
}

fn slot(local: &TypedSemanticLocal, frame: u32) -> Result<u32, CheckerError> {
    let offset = local
        .source_id
        .checked_mul(8)
        .and_then(|offset| u32::try_from(offset).ok())
        .ok_or_else(|| invalid("scalar definition slot overflow"))?;
    if offset.checked_add(8).is_none_or(|end| end > frame.saturating_sub(16)) {
        return Err(invalid("scalar definition leaves its private frame"));
    }
    Ok(offset)
}

fn operand(
    c: &mut Cursor<'_>,
    value: &TypedSemanticOperand,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
    frame: u32,
    register: u32,
) -> Result<(), CheckerError> {
    if let Some(id) = value.local {
        c.stack_load(register, slot(locals.get(&id).ok_or_else(|| invalid("scalar definition operand missing"))?, frame)?)
    } else {
        c.constant(register, literal(value.constant.as_ref().ok_or_else(|| invalid("scalar definition has no source"))?)?)
    }
}

fn arithmetic(c: &mut Cursor<'_>, machine: &Machine<'_>, operator: &str, ty: &str, signed: bool) -> Result<(), CheckerError> {
    // t0/t1 are the typed operands. Guards must terminate the process with the
    // exact numeric error; returning an error as a value is not sufficient.
    if matches!(operator, "div" | "mod" | "shl" | "shr") {
        let ok = c.label();
        let code = if matches!(operator, "div" | "mod") {
            c.branch(1, 6, 0, ok)?;
            20
        } else {
            let width = match ty {
                "u8" => 8,
                "u16" => 16,
                "u32" | "i32" => 32,
                _ => 64,
            };
            c.constant(7, width)?;
            c.branch(6, 6, 7, ok)?;
            65
        };
        let failure = c.jump_target()?;
        let mut exit = Cursor::new(failure, failure + 8, machine.words, machine.targets);
        exit.constant(10, code)?;
        if exit.jump_target()? != machine.abort {
            return Err(invalid("scalar guard does not terminate the process"));
        }
        exit.finish()?;
        c.bind(ok)?;
    }
    let (funct7, funct3) = match operator {
        "add" => (0, 0),
        "sub" => (32, 0),
        "mul" => (1, 0),
        "div" => (1, if signed { 4 } else { 5 }),
        "mod" => (1, if signed { 6 } else { 7 }),
        "and" | "bit-and" => (0, 7),
        "or" | "bit-or" => (0, 6),
        "bit-xor" => (0, 4),
        "shl" => (0, 1),
        "shr" if ty == "i32" => {
            c.immediate(0x13, 1, 5, 5, 32)?;
            c.immediate(0x13, 5, 5, 5, 0x420)?;
            (32, 5)
        }
        "shr" => (0, 5),
        _ => return Err(invalid("unknown scalar operation")),
    };
    c.exact((funct7 << 25) | (6 << 20) | (5 << 15) | (funct3 << 12) | (5 << 7) | 0x33)?;
    let shift = match ty {
        "u8" => 56,
        "u16" => 48,
        "u32" | "i32" => 32,
        _ => 0,
    };
    if shift != 0 {
        c.immediate(0x13, 1, 5, 5, shift)?;
        c.immediate(0x13, 5, 5, 5, shift | if ty == "i32" { 0x400 } else { 0 })?;
    }
    Ok(())
}

fn definition(
    c: &mut Cursor<'_>,
    machine: &Machine<'_>,
    op: &TypedSemanticOperation,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
    frame: u32,
    fused_successors: Option<[u64; 2]>,
) -> Result<Option<u64>, CheckerError> {
    if let Some(property) = properties::property(op) {
        properties::call(c, machine, op, locals, frame, property)?;
        return Ok(None);
    }
    let dest = locals[&op.destinations[0]];
    match (&*op.opcode, &op.detail, op.operands.as_slice()) {
        ("load-const", TypedSemanticOperationDetail::Constant { value }, []) => c.constant(5, literal(value)?)?,
        ("move", _, [value]) => operand(c, value, locals, frame, 5)?,
        ("unary", TypedSemanticOperationDetail::UnaryOperator { operator }, [value]) if operator == "not" => {
            operand(c, value, locals, frame, 5)?;
            c.immediate(0x13, 4, 5, 5, 1)?;
        }
        ("unary", TypedSemanticOperationDetail::UnaryOperator { operator }, [value]) if operator == "neg" => {
            c.constant(5, 0)?;
            operand(c, value, locals, frame, 6)?;
            arithmetic(c, machine, "sub", &dest.ty, false)?;
        }
        ("binary", TypedSemanticOperationDetail::BinaryOperator { operator }, [left, right]) => {
            operand(c, left, locals, frame, 5)?;
            operand(c, right, locals, frame, 6)?;
            let signed = left.ty == "i32" && right.ty == "i32";
            if comparisons::is_comparison(operator) {
                if dest.ty != "bool" {
                    return Err(invalid("scalar comparison has a non-boolean result"));
                }
                if machine.words.get(&c.pc).is_some_and(|word| word & 0x7f == 0x63) {
                    let branch = c.pc;
                    comparisons::fused(
                        c,
                        operator,
                        signed,
                        fused_successors.ok_or_else(|| invalid("fused comparison result has another use"))?,
                    )?;
                    return Ok(Some(branch));
                }
                comparisons::materialized(c, operator, signed)?;
            } else {
                arithmetic(c, machine, operator, &dest.ty, signed)?;
            }
        }
        _ => return Err(invalid("invalid scalar definition shape")),
    }
    c.stack_store(5, slot(dest, frame)?)?;
    c.finish()?;
    Ok(None)
}

pub(crate) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    terminal_sink: Option<&str>,
    selections: &crate::cell_selection::machine::Checked,
    bodies: &BTreeMap<u64, u64>,
    allocated: &crate::allocated_values::machine::Checked,
) -> Result<(), CheckerError> {
    if !record.typed_semantics.entries.iter().any(covered_entry)
        && !record.blocks.iter().any(|block| block.allocated_lifecycle_labels.iter().any(|label| label.starts_with(PREFIX)))
    {
        return Ok(());
    }
    let normalized = crate::policy_machine::normalize_relaxed_branches(record, elf).map_err(invalid)?;
    let (record, elf) = normalized.as_ref().map_or((record, elf), |(record, elf)| (record, elf));
    let abort = record
        .blocks
        .iter()
        .find(|block| Some(block.id.as_str()) == terminal_sink)
        .ok_or_else(|| invalid("scalar definitions have no checked fatal sink"))?
        .range
        .start;
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let properties = properties::validate(record, elf, &words, &targets, abort)?;
    let machine = &Machine { words: &words, targets: &targets, abort, properties };
    let fused = comparisons::fused_sites(record)?;
    let mut expected = BTreeMap::new();
    let mut entries = BTreeMap::new();
    for entry in &record.typed_semantics.entries {
        if !covered_entry(entry) {
            continue;
        }
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect::<BTreeMap<_, _>>();
        for block in &entry.blocks {
            for op in &block.operations {
                if covered(op, &locals) {
                    expected.insert((entry.id.as_str(), block.id, op.index), op);
                }
            }
        }
        let frame = record
            .entries
            .iter()
            .find(|machine| machine.id == entry.id)
            .ok_or_else(|| invalid("scalar owner has no machine entry"))?
            .frame_size_bytes;
        entries.insert(entry.id.as_str(), (locals, frame));
    }
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    let mut current = BTreeMap::new();
    let mut boundaries = BTreeMap::new();
    for block in &blocks {
        let owner = block.owner_entry.as_str();
        if let Some(id) = block.lowering_block_id {
            current.insert(owner, id);
        }
        for label in &block.allocated_lifecycle_labels {
            let Some(suffix) = label.strip_prefix(PREFIX) else { continue };
            let parts = suffix.split('_').collect::<Vec<_>>();
            if parts.len() != 4 || !matches!(parts[3], "begin" | "end") {
                return Err(invalid("invalid scalar definition marker"));
            }
            let numeric = parts[..3]
                .iter()
                .map(|part| part.parse::<u32>().map_err(|_| invalid("invalid scalar definition coordinate")))
                .collect::<Result<Vec<_>, _>>()?;
            // An end marker can share the next fallthrough block's address.
            // The half-open instruction range below still belongs to its
            // declared source block; only its start needs that current ID.
            if (parts[3] == "begin" && current.get(owner) != Some(&numeric[0]))
                || boundaries.insert((owner, numeric[0], numeric[1], numeric[2], parts[3]), block.range.start).is_some()
            {
                return Err(invalid("duplicate or detached scalar definition boundary"));
            }
        }
    }
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for edge in &elf.control_flow {
        incoming.entry(edge.target).or_default().push(edge.address);
    }
    let mut last = BTreeMap::new();
    let mut required = flow::Markers::new();
    let mut fused_blocks = std::collections::BTreeSet::new();
    let mut work = 0usize;
    let count = boundaries.len();
    let mut consumed = 0;
    for (&(owner, block, index, nonce, edge), &start) in &boundaries {
        if edge != "begin" {
            continue;
        }
        let &end = boundaries.get(&(owner, block, index, nonce, "end")).ok_or_else(|| invalid("scalar definition end missing"))?;
        if start >= end || end - start > 4096 || last.insert((owner, block), end).is_some_and(|previous| previous > start) {
            return Err(invalid("overlapping, reordered or excessive scalar definition range"));
        }
        let op = expected.remove(&(owner, block, index)).ok_or_else(|| invalid("extra or unbound scalar definition"))?;
        let (locals, frame) = &entries[owner];
        let first = blocks.partition_point(|candidate| candidate.range.end <= start);
        let stop = blocks.partition_point(|candidate| candidate.range.start < end);
        for candidate in &blocks[first..stop] {
            work += 1;
            if work > 4_000_000 || candidate.owner_entry != owner || candidate.lowering_block_id.is_some_and(|id| id != block) {
                return Err(invalid("scalar definition leaves its typed block or proof budget"));
            }
        }
        for (_, sources) in incoming.range((std::ops::Bound::Excluded(start), std::ops::Bound::Excluded(end))) {
            work += sources.len();
            if work > 4_000_000 || sources.iter().any(|source| !(start..end).contains(source)) {
                return Err(invalid("control flow bypasses part of a scalar definition"));
            }
        }
        let branch = definition(
            &mut Cursor::new(start, end, machine.words, machine.targets),
            machine,
            op,
            locals,
            *frame,
            fused.get(&(owner, block, index)).copied(),
        )
        .map_err(|error| invalid(format!("definition {owner}/{block}/{index} at {start:#x}..{end:#x}: {}", error.message)))?;
        if branch.is_some() {
            fused_blocks.insert((owner, block));
        }
        // The decoded arithmetic/guard body is atomic on successful paths.
        // A fused comparison is different: keep its branch visible to the CFG
        // walker so both successors are checked rather than assuming fallthrough.
        required.insert((owner, block, flow::operation_position(index, 0)?), (start, branch.unwrap_or(end), 0));
        consumed += 2;
    }
    if !expected.is_empty() || consumed != count {
        return Err(invalid("missing typed scalar definition or orphan boundary"));
    }
    // Compose complete, independently recognized Cell reads/selections. Their
    // inline returning syscalls and failure branches are already checked; raw
    // syscall metadata alone cannot authorize a path past scalar definitions.
    for ((owner, block, op), range) in &selections.flow_sites {
        if required.insert((owner.as_str(), *block, flow::operation_position(*op, 0)?), *range).is_some() {
            return Err(invalid("scalar definition overlaps a checked Cell operation"));
        }
    }
    for ((owner, block, position), range) in &allocated.flow_sites {
        if required.insert((owner.as_str(), *block, *position), *range).is_some() {
            return Err(invalid("collection operation overlaps another required source operation"));
        }
    }
    let fallthroughs = branches::validate(record, machine, &fused_blocks, &mut required)?;
    returns::validate(record, elf, machine, &mut required)?;
    flow::validate_with_fallthroughs(record, elf, &required, machine.abort, bodies, &fallthroughs)
        .map_err(|error| invalid(format!("bounded operation flow: {}", error.message)))
}
