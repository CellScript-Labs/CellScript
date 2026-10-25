//! Exact successor comparisons bound to typed create operands and loaded Cells.
//! Fixed operands bind literal bytes or typed local slots; their producer
//! equivalence and complete transaction authority are separate proofs.
use super::*;
use crate::{CellBindingRole, TypedSemanticOperationDetail};

const PREFIX: &str = ".Lnested_output_check_";
mod fixed;

/// Output root, field index, typed create block, expected value source.
pub(super) type Obligation = (u64, u32, u32, u64);

pub(super) struct Comparison {
    pub end: u64,
    pub output: u64,
    pub field: u32,
    pub expected: u64,
    pub block: u32,
    pub nested_view: bool,
}

pub(super) struct Checked {
    pub sites: BTreeMap<String, BTreeMap<u64, Comparison>>,
    pub required: BTreeMap<String, BTreeSet<Obligation>>,
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    cells: &cells::Checked,
    sizes: &BTreeMap<String, BTreeMap<u64, u32>>,
) -> Result<Checked, CheckerError> {
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect();
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for flow in &elf.control_flow {
        incoming.entry(flow.target).or_default().push(flow.address);
    }
    let mut labels = BTreeMap::new();
    let mut typed_owners = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        typed_owners.insert(block.range.start, (block.owner_entry.as_str(), current.get(block.owner_entry.as_str()).copied()));
        for label in &block.nested_output_labels {
            if labels.insert((block.owner_entry.as_str(), label.as_str()), block.range.start).is_some() {
                return Err(invalid("duplicate nested output comparison marker"));
            }
        }
    }
    // Keys bind the selected output, declared field order and exact source
    // local. A compatible type alone cannot substitute a different value.
    // Reject overlapping ranges before scanning any instructions or incoming
    // edges. Each machine range is visited at most once by this proof.
    let mut ranges = Vec::new();
    for ((owner, label), start) in &labels {
        if label.ends_with("_done") {
            continue;
        }
        let end =
            *labels.get(&(*owner, format!("{label}_done").as_str())).ok_or_else(|| invalid("missing output comparison completion"))?;
        if end <= *start {
            return Err(invalid("invalid output comparison extent"));
        }
        ranges.push((*start, end));
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(invalid("overlapping output comparisons"));
    }
    let mut expected = BTreeMap::new();
    let mut fixed_fields = BTreeMap::new();
    let mut required = BTreeMap::<String, BTreeSet<Obligation>>::new();
    for entry in &record.typed_semantics.entries {
        for block in &entry.blocks {
            for operation in &block.operations {
                let (pattern, skip) = match &operation.detail {
                    TypedSemanticOperationDetail::Create { pattern } if operation.opcode == "create" => (pattern, 0),
                    TypedSemanticOperationDetail::CreateUnique { pattern, .. } => (pattern, 0),
                    TypedSemanticOperationDetail::ReplaceUnique { pattern, .. } => (pattern, 1),
                    _ => continue,
                };
                let Some(ty) = record.typed_semantics.types.iter().find(|ty| ty.name == pattern.ty) else { continue };
                if !ty.fields.iter().any(|field| crate::checker::is_nested_source_type(&field.ty)) {
                    continue;
                }
                let binding = entry
                    .cell_bindings
                    .iter()
                    .find(|binding| {
                        binding.role == CellBindingRole::Output && binding.binding == pattern.binding && binding.ty == pattern.ty
                    })
                    .ok_or_else(|| invalid("nested create has no typed output binding"))?;
                let output = entry
                    .locals
                    .iter()
                    .find(|local| Some(local.id) == binding.local_id)
                    .ok_or_else(|| invalid("nested output has no local"))?
                    .source_id;
                for field in &ty.fields {
                    let position = pattern
                        .field_names
                        .iter()
                        .position(|name| name == &field.name)
                        .ok_or_else(|| invalid("nested create omits a declared field"))?;
                    if pattern.field_names.iter().filter(|name| *name == &field.name).count() != 1 {
                        return Err(invalid("nested create repeats a declared field"));
                    }
                    let operand = operation
                        .operands
                        .get(position + skip)
                        .ok_or_else(|| invalid("nested create has no expected field operand"))?;
                    let local = operand
                        .local
                        .map(|id| {
                            entry
                                .locals
                                .iter()
                                .find(|local| local.id == id)
                                .ok_or_else(|| invalid("output expected value has no local"))
                        })
                        .transpose()?;
                    if local.is_some_and(|local| local.ty != field.ty) || operand.ty != field.ty {
                        return Err(invalid("nested output comparison changes its declared value type"));
                    }
                    let index = field.molecule_field_index.ok_or_else(|| invalid("nested output lacks declared field order"))?;
                    let source = local.map_or(u64::MAX, |local| local.source_id);
                    let key = (entry.id.as_str(), block.id, output, index, source);
                    if !crate::checker::is_nested_source_type(&field.ty) {
                        let fixed = fixed::Expected::new(entry, operand, &field.ty)?;
                        if fixed_fields.get(&key).is_some_and(|previous| previous != &fixed) {
                            return Err(invalid("ambiguous repeated fixed output constraint"));
                        }
                        fixed_fields.insert(key, fixed);
                    } else if local.is_none() {
                        return Err(invalid("nested output has no expected view local"));
                    }
                    expected.insert(key, ty.fields.len());
                    required.entry(entry.id.clone()).or_default().insert((output, index, block.id, source));
                }
            }
        }
    }
    let mut checked = BTreeSet::new();
    let mut covered = BTreeSet::new();
    let mut sites = BTreeMap::<String, BTreeMap<u64, Comparison>>::new();
    for ((owner, label), start) in &labels {
        if label.ends_with("_done") {
            continue;
        }
        let coordinates = label
            .strip_prefix(PREFIX)
            .ok_or_else(|| invalid("invalid output comparison marker"))?
            .split('_')
            .map(|value| value.parse::<u64>().map_err(|_| invalid("invalid output coordinates")))
            .collect::<Result<Vec<_>, _>>()?;
        let [output, field, source, _ordinal] = coordinates.as_slice() else { return Err(invalid("invalid output coordinates")) };
        let field = u32::try_from(*field).map_err(|_| invalid("output field index overflows"))?;
        let (_, typed_block) = typed_owners.get(start).ok_or_else(|| invalid("output comparison has no typed owner"))?;
        let key = (*owner, typed_block.ok_or_else(|| invalid("output comparison has no typed block"))?, *output, field, *source);
        let field_count =
            *expected.get(&key).ok_or_else(|| invalid("output comparison differs from its typed create operand or block"))?;
        covered.insert(key);
        let end_name = format!("{label}_done");
        let end = *labels.get(&(*owner, end_name.as_str())).ok_or_else(|| invalid("missing output comparison completion"))?;
        checked.insert((*owner, (*label).to_string()));
        checked.insert((*owner, end_name));
        let buffer =
            cells.buffers.get(&(owner.to_string(), *output)).ok_or_else(|| invalid("output comparison has no checked Cell load"))?;
        let lowering = record.entries.iter().find(|entry| entry.id == *owner).ok_or_else(|| invalid("missing output frame"))?;
        let frame = owned_frame(record, elf, lowering)?;
        if typed_owners.range(*start..end).any(|(_, (entry, block))| entry != owner || block != typed_block)
            || incoming.range(*start + 4..end).any(|(_, sources)| sources.iter().any(|source| !(*start..end).contains(source)))
        {
            return Err(invalid("control flow enters or leaves the typed output comparison"));
        }
        let mut c = Decoder { pc: *start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
        cells::pointer(&mut c, 29, buffer.start)?;
        uses::expect_stack(&mut c, 10, buffer.size, false, frame)?;
        c.constant(6, (field_count as u64 + 1) * 4)?;
        let mut failures = vec![cells::branch(&mut c, 6, 10, 6)?];
        c.u32_le(5, 29, 0, 7)?;
        failures.push(cells::branch(&mut c, 1, 5, 10)?);
        c.u32_le(30, 29, (field as i32 + 1) * 4, 7)?;
        if field as usize + 1 < field_count {
            c.u32_le(31, 29, (field as i32 + 2) * 4, 7)?;
        } else {
            c.addi(31, 10, 0)?;
        }
        failures.extend([cells::branch(&mut c, 6, 30, 6)?, cells::branch(&mut c, 6, 31, 30)?, cells::branch(&mut c, 6, 10, 31)?]);
        c.sub(12, 31, 30)?;
        if let Some(fixed) = fixed_fields.get(&key) {
            fixed.check(&mut c, frame, end, &mut failures)?;
        } else {
            let size = sizes
                .get(*owner)
                .and_then(|sizes| sizes.get(source))
                .ok_or_else(|| invalid("output expected value has no checked size"))?;
            uses::expect_stack(&mut c, 13, *size, false, frame)?;
            failures.push(cells::branch(&mut c, 1, 12, 13)?);
            c.add(10, 29, 30)?;
            let pointer = source
                .checked_mul(8)
                .and_then(|slot| u32::try_from(slot).ok())
                .ok_or_else(|| invalid("output source slot overflows"))?;
            uses::expect_stack(&mut c, 11, pointer, false, frame)?;
            byte_loop(&mut c, end, &mut failures)?;
        }
        if failures.iter().any(|target| *target != c.pc) {
            return Err(invalid("output mismatch bypasses failure"));
        }
        cells::failure(&mut c, record, fixed_fields.get(&key).map_or(3, fixed::Expected::failure_code))?;
        if c.pc != end {
            return Err(invalid("unchecked output comparison tail"));
        }
        if sites
            .entry(owner.to_string())
            .or_default()
            .insert(
                *start,
                Comparison {
                    end,
                    output: *output,
                    field,
                    expected: *source,
                    block: key.1,
                    nested_view: !fixed_fields.contains_key(&key),
                },
            )
            .is_some()
        {
            return Err(invalid("co-located output comparisons"));
        }
    }
    if labels.keys().any(|(owner, label)| !checked.contains(&(*owner, label.to_string())))
        || expected.keys().copied().collect::<BTreeSet<_>>() != covered
    {
        return Err(invalid("missing, overlapping or untyped output comparisons"));
    }
    Ok(Checked { sites, required })
}

fn byte_loop(c: &mut Decoder<'_>, end: u64, failures: &mut Vec<u64>) -> Result<(), CheckerError> {
    let bytes = c.pc;
    if cells::branch(c, 0, 12, 0)? != end {
        return Err(invalid("output byte loop completion changed"));
    }
    c.immediate(3, 4, 5, 10, 0)?;
    c.immediate(3, 4, 6, 11, 0)?;
    failures.push(cells::branch(c, 1, 5, 6)?);
    c.addi(10, 10, 1)?;
    c.addi(11, 11, 1)?;
    c.addi(12, 12, -1)?;
    if c.jump()? != bytes {
        return Err(invalid("output byte loop back edge changed"));
    }
    Ok(())
}
