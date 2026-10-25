//! Typed call coordinates for borrowed nested Cells, values and derived views.
//! Actual pointer/length provenance is checked by the memory-effect interpreter
//! at each call; matching the codec alone cannot authorize another valid root.
use super::*;
use crate::checker::canonical_abi_type;
use crate::{TypedSemanticEntry, TypedSemanticOperation};

pub(super) struct Parameter {
    pub source: u64,
    pub word: u32,
    pub ty: String,
    pub nested: bool,
}

pub(super) struct Argument {
    pub root: u64,
    pub word: u32,
    pub nested: bool,
}

pub(super) struct Call {
    pub owner: String,
    pub callee: String,
    pub arguments: Vec<Argument>,
    pub outgoing_stack_bytes: u32,
}

pub(super) struct Checked {
    pub parameters: BTreeMap<String, Vec<Parameter>>,
    pub calls: BTreeMap<u64, Call>,
}

fn nominal(ty: &str) -> String {
    let ty = canonical_abi_type(ty);
    ty.strip_prefix("&mut").or_else(|| ty.strip_prefix('&')).unwrap_or(&ty).to_string()
}

pub(super) fn root(entry: &TypedSemanticEntry, mut source: u64) -> Result<u64, CheckerError> {
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(source) {
            return Err(invalid("cyclic borrowed view aliases"));
        }
        let local = entry
            .locals
            .iter()
            .find(|local| local.source_id == source)
            .ok_or_else(|| invalid("borrowed Cell source is not a typed local"))?;
        let mut definitions = entry.blocks.iter().flat_map(|block| &block.operations).filter(|op| {
            (op.opcode == "move"
                || (op.opcode == "unary"
                    && matches!(&op.detail,
                crate::TypedSemanticOperationDetail::UnaryOperator { operator } if operator == "ref")))
                && op.destinations.contains(&local.id)
        });
        let Some(definition) = definitions.next() else { return Ok(source) };
        if definitions.next().is_some() || definition.operands.len() != 1 {
            return Err(invalid("borrowed Cell alias has no unique source"));
        }
        let operand = &definition.operands[0];
        if nominal(&operand.ty) != nominal(&local.ty) {
            return Err(invalid("borrowed Cell alias changes its nominal schema"));
        }
        source = entry
            .locals
            .iter()
            .find(|local| Some(local.id) == operand.local)
            .ok_or_else(|| invalid("borrowed Cell alias has no local root"))?
            .source_id;
    }
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<Checked, CheckerError> {
    let mut parameters = BTreeMap::new();
    for entry in &record.typed_semantics.entries {
        if entry.kind != "helper" {
            continue;
        }
        let mut params = Vec::new();
        let mut word = 0u32;
        for param in &entry.params {
            let ty = canonical_abi_type(&param.ty);
            let nested = crate::checker::is_nested_source_type(&ty);
            if nested
                && (param.mutable
                    || param.reference
                    || param.source != "default"
                    || !matches!(crate::checker::normalize_effect(&entry.effect).as_str(), "pure" | "readonly"))
            {
                return Err(invalid("nested helper parameters require immutable default bindings and a read-only effect"));
            }
            if nested
                || (ty.starts_with('&')
                    && record.typed_semantics.types.iter().any(|layout| {
                        nominal(&layout.name) == nominal(&ty)
                            && matches!(layout.kind.as_str(), "resource" | "shared" | "receipt")
                            && layout.fields.iter().any(|field| crate::checker::is_nested_source_type(&field.ty))
                    }))
            {
                if ty.starts_with("&mut") {
                    return Err(invalid("mutable borrowed nested Cells are not admitted"));
                }
                let local = entry
                    .locals
                    .iter()
                    .find(|local| local.id == param.binding_id)
                    .ok_or_else(|| invalid("borrowed Cell parameter has no local"))?;
                if entry.locals.iter().filter(|candidate| candidate.source_id == local.source_id).count() != 1 {
                    return Err(invalid("borrowed Cell parameter aliases another typed local"));
                }
                params.push(Parameter { source: local.source_id, word, ty: nominal(&ty), nested });
            }
            word = word
                .checked_add(crate::fixed_results::parameter_word_width(record, entry, param))
                .ok_or_else(|| invalid("borrowed Cell argument expansion overflows"))?;
        }
        if !params.is_empty() {
            parameters.insert(entry.id.clone(), params);
        }
    }
    let entries = record.typed_semantics.entries.iter().map(|entry| (entry.id.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let names = record
        .typed_semantics
        .entries
        .iter()
        .filter(|entry| parameters.contains_key(&entry.id))
        .map(|entry| (entry.name.as_str(), entry.id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let starts = record
        .entries
        .iter()
        .filter(|entry| parameters.contains_key(&entry.id))
        .map(|entry| {
            let start = record
                .blocks
                .iter()
                .find(|block| block.id == entry.entry_block)
                .ok_or_else(|| invalid("borrowed helper has no entry block"))?
                .range
                .start;
            Ok((start, entry.id.as_str()))
        })
        .collect::<Result<BTreeMap<_, _>, CheckerError>>()?;
    let blocks = record.blocks.iter().map(|block| (block.range.start, block)).collect::<BTreeMap<_, _>>();
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect::<BTreeMap<_, _>>();
    let mut typed_at = BTreeMap::new();
    let mut current = BTreeMap::new();
    for (address, block) in &blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        typed_at.insert(*address, (block.owner_entry.as_str(), current.get(block.owner_entry.as_str()).copied()));
    }
    let mut expected = BTreeMap::<(&str, u32), Vec<&TypedSemanticOperation>>::new();
    for entry in &record.typed_semantics.entries {
        for block in &entry.blocks {
            for operation in &block.operations {
                if operation.call.as_ref().is_some_and(|call| names.contains_key(call.target.as_str())) {
                    expected.entry((entry.id.as_str(), block.id)).or_default().push(operation);
                }
            }
        }
    }
    let mut observed = BTreeMap::<(&str, u32), usize>::new();
    let mut calls = BTreeMap::new();
    // Machine order within each typed block binds repeated calls independently.
    let mut flows = elf.control_flow.iter().collect::<Vec<_>>();
    flows.sort_by_key(|flow| flow.address);
    for flow in flows {
        let Some(callee) = starts.get(&flow.target).copied() else { continue };
        let word = *words.get(&flow.address).ok_or_else(|| invalid("missing borrowed call instruction"))?;
        if !matches!(word & 0x7f, 0x67 | 0x6f) || word >> 7 & 31 != 1 {
            return Err(invalid("borrowed helper must be entered by a call"));
        }
        let (_, &(owner, Some(block))) =
            typed_at.range(..=flow.address).next_back().ok_or_else(|| invalid("borrowed call has no typed owner"))?
        else {
            return Err(invalid("borrowed call has no typed block"));
        };
        let index = observed.entry((owner, block)).or_default();
        let operation = expected
            .get(&(owner, block))
            .and_then(|ops| ops.get(*index))
            .ok_or_else(|| invalid("machine borrowed call has no typed operation"))?;
        *index += 1;
        let declaration = operation.call.as_ref().expect("selected call");
        if names.get(declaration.target.as_str()) != Some(&callee) {
            return Err(invalid("borrowed call target/order differs from its typed operation"));
        }
        let caller = entries[owner];
        let callee_entry = entries[callee];
        let mut arguments = Vec::new();
        for parameter in &parameters[callee] {
            let index = callee_entry
                .params
                .iter()
                .position(|param| {
                    callee_entry.locals.iter().any(|local| local.id == param.binding_id && local.source_id == parameter.source)
                })
                .ok_or_else(|| invalid("borrowed parameter position missing"))?;
            let operand = operation.operands.get(index).ok_or_else(|| invalid("borrowed call argument missing"))?;
            if nominal(&operand.ty) != parameter.ty {
                return Err(invalid("borrowed argument changes its declared view type or bounds"));
            }
            let source = caller
                .locals
                .iter()
                .find(|local| Some(local.id) == operand.local)
                .ok_or_else(|| invalid("borrowed call argument is not a local"))?
                .source_id;
            arguments.push(Argument { root: root(caller, source)?, word: parameter.word, nested: parameter.nested });
        }
        let mut words = callee_entry.params.iter().try_fold(0u32, |count, param| {
            count
                .checked_add(crate::fixed_results::parameter_word_width(record, callee_entry, param))
                .ok_or_else(|| invalid("borrowed call ABI overflows"))
        })?;
        if record.entries.iter().any(|entry| entry.id == callee && entry.fixed_result_abi.is_some()) {
            words = words.checked_add(1).ok_or_else(|| invalid("borrowed call result ABI overflows"))?;
        }
        let outgoing_stack_bytes = words
            .saturating_sub(8)
            .checked_mul(8)
            .and_then(|bytes| bytes.checked_next_multiple_of(16))
            .ok_or_else(|| invalid("borrowed call stack size overflows"))?;
        calls.insert(flow.address, Call { owner: owner.to_string(), callee: callee.to_string(), arguments, outgoing_stack_bytes });
    }
    if expected.iter().any(|(key, operations)| observed.get(key).copied().unwrap_or(0) != operations.len()) {
        return Err(invalid("typed borrowed call lacks a machine call"));
    }
    Ok(Checked { parameters, calls })
}
