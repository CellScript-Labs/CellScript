//! Bind arena helper calls to source owner identities and expanded ABI words.
//! `lifetime` separately requires the actual pointer/extent facts on every
//! reaching path. Complete collection value and machine retirement proofs remain
//! separate from constructor/parameter provenance and argument transport.
use super::*;
use crate::allocated_values::machine::Checked as Allocated;

#[derive(Clone, Copy)]
pub(super) enum Root {
    Owned { source: u64, extent: u32 },
    Borrowed { source: u64 },
}

pub(super) struct Argument {
    pub word: u32,
    pub root: Root,
}

pub(super) struct Call {
    pub owner: String,
    pub callee: String,
    pub outgoing: u32,
    pub arguments: Vec<Argument>,
}

#[derive(Default)]
pub(super) struct Checked {
    pub helpers: BTreeSet<String>,
    pub calls: BTreeMap<u64, Call>,
}

struct Parameter {
    position: usize,
    word: u32,
    owner_type: String,
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    seeds: &BTreeMap<String, lifetime::Seed>,
    allocated: &Allocated,
) -> Result<Checked, CheckerError> {
    let owner_types = crate::allocated_values::owner_types(&record.typed_semantics);
    let mut result = Checked::default();
    let mut parameters = BTreeMap::new();
    let mut roots = BTreeMap::<String, BTreeMap<u32, (Root, String)>>::new();
    for entry in &record.typed_semantics.entries {
        let owner_roots = roots.entry(entry.id.clone()).or_default();
        for constructor in allocated.constructors.get(&entry.id).into_iter().flat_map(|sites| sites.values()) {
            owner_roots.insert(
                constructor.local,
                (Root::Owned { source: constructor.source, extent: constructor.extent }, constructor.owner_type.clone()),
            );
        }
        if entry.kind != "helper" {
            continue;
        }
        let mut word = 0u32;
        let mut params = Vec::new();
        for (position, param) in entry.params.iter().enumerate() {
            let width = crate::fixed_results::parameter_word_width(record, entry, param);
            if let Some(ty) = param.ty.strip_prefix('&').filter(|ty| owner_types.contains(*ty)) {
                let source = entry
                    .locals
                    .iter()
                    .find(|local| local.id == param.binding_id)
                    .ok_or_else(|| invalid("arena argument has no parameter local"))?
                    .source_id;
                if width != 2 || seeds.get(&entry.id).and_then(|seed| seed.allocated_spans.get(&word)) != Some(&source) {
                    return Err(invalid("arena call parameter lacks independently checked incoming words"));
                }
                params.push(Parameter { position, word, owner_type: ty.to_string() });
                owner_roots.insert(param.binding_id, (Root::Borrowed { source }, ty.to_string()));
            }
            word = word.checked_add(width).ok_or_else(|| invalid("arena call ABI expansion overflows"))?;
        }
        if !params.is_empty() {
            result.helpers.insert(entry.id.clone());
            parameters.insert(entry.id.as_str(), params);
        }
    }
    if parameters.is_empty() {
        return Ok(result);
    }
    let entries = record.typed_semantics.entries.iter().map(|entry| (entry.id.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let names = entries
        .values()
        .filter(|entry| parameters.contains_key(entry.id.as_str()))
        .map(|entry| (entry.name.as_str(), entry.id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut starts = BTreeMap::new();
    let mut outgoing = BTreeMap::new();
    for id in parameters.keys() {
        let entry =
            record.entries.iter().find(|entry| entry.id == *id).ok_or_else(|| invalid("arena helper has no lowering entry"))?;
        let start = record
            .blocks
            .iter()
            .find(|block| block.id == entry.entry_block)
            .ok_or_else(|| invalid("arena helper has no entry block"))?
            .range
            .start;
        if starts.insert(start, *id).is_some() {
            return Err(invalid("arena helpers share a machine entry"));
        }
        outgoing.insert(*id, view_calls::outgoing_bytes(record, entries[id])?);
    }
    let mut typed_at = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        typed_at.insert(block.range.start, (block.owner_entry.as_str(), current.get(block.owner_entry.as_str()).copied()));
    }
    let mut expected = BTreeMap::<(&str, u32), Vec<&crate::TypedSemanticOperation>>::new();
    let mut aliases = BTreeMap::new();
    for entry in entries.values() {
        aliases.insert(entry.id.as_str(), crate::allocated_values::machine::owner_aliases(entry, &owner_types)?);
        for block in &entry.blocks {
            for op in &block.operations {
                if op.call.as_ref().is_some_and(|call| names.contains_key(call.target.as_str())) {
                    expected.entry((entry.id.as_str(), block.id)).or_default().push(op);
                }
            }
        }
    }
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    let mut observed = BTreeMap::<(&str, u32), usize>::new();
    let mut flows = elf.control_flow.iter().collect::<Vec<_>>();
    flows.sort_by_key(|flow| flow.address);
    let mut steps = 0usize;
    for flow in flows {
        let Some(callee) = starts.get(&flow.target).copied() else { continue };
        let word = *words.get(&flow.address).ok_or_else(|| invalid("arena call instruction missing"))?;
        if !matches!(word & 0x7f, 0x67 | 0x6f) || word >> 7 & 31 != 1 {
            return Err(invalid("arena helper must be entered through a call"));
        }
        let &(owner, Some(block)) = typed_at.range(..=flow.address).next_back().ok_or_else(|| invalid("arena call owner missing"))?.1
        else {
            return Err(invalid("arena call has no typed block"));
        };
        let ordinal = observed.entry((owner, block)).or_default();
        let op = expected
            .get(&(owner, block))
            .and_then(|ops| ops.get(*ordinal))
            .ok_or_else(|| invalid("machine arena call has no typed operation"))?;
        *ordinal += 1;
        if op.call.as_ref().and_then(|call| names.get(call.target.as_str())).copied() != Some(callee) {
            return Err(invalid("arena call target or order differs from typed source"));
        }
        let mut arguments = Vec::new();
        for param in &parameters[callee] {
            let arg = op.operands.get(param.position).ok_or_else(|| invalid("arena call operand missing"))?;
            if arg.ty.trim_start_matches('&') != param.owner_type {
                return Err(invalid("arena call changes the nominal owner type"));
            }
            let mut local = arg.local.ok_or_else(|| invalid("arena call argument is not a local"))?;
            let mut visited = BTreeSet::new();
            while !roots[owner].contains_key(&local) {
                steps += 1;
                if steps > 4_000_000 || !visited.insert(local) {
                    return Err(invalid("cyclic or excessive arena call alias chain"));
                }
                let source = aliases[owner]
                    .get(&local)
                    .filter(|sources| sources.len() == 1)
                    .and_then(|sources| sources.first())
                    .ok_or_else(|| invalid("arena call has an ambiguous source owner"))?;
                local = *source;
            }
            let (root, ty) = &roots[owner][&local];
            if *ty != param.owner_type {
                return Err(invalid("arena call root type differs from its parameter"));
            }
            arguments.push(Argument { word: param.word, root: *root });
        }
        result.calls.insert(
            flow.address,
            Call { owner: owner.to_string(), callee: callee.to_string(), outgoing: outgoing[callee], arguments },
        );
    }
    if expected.iter().any(|(key, ops)| observed.get(key).copied().unwrap_or(0) != ops.len()) {
        return Err(invalid("typed arena call lacks machine coverage"));
    }
    Ok(result)
}
