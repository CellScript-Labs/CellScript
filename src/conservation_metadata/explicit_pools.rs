//! Producer-side proof of explicit pool accounting from executable IR.
//! The standalone checker reconstructs its own proof from typed records.

use crate::ast::BinaryOp;
use crate::ir::{
    self, CreatePattern, IrBlock, IrBody, IrConst, IrInstruction, IrLifecycleExecution, IrOperand, IrPoolAccounting,
    IrSourceDisposition, IrSourceDispositionKind, IrTerminator, IrType, IrVar,
};
use crate::runtime_errors::CellScriptRuntimeError;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(super) fn checked_detail(
    body: &IrBody,
    sources: &[IrSourceDisposition],
    schema: &str,
    consumed: &[IrVar],
    created: &[CreatePattern],
) -> Option<String> {
    if sources.is_empty() {
        return None;
    }
    let input_ids = consumed.iter().map(|value| value.id).collect::<BTreeSet<_>>();
    let output_names = created.iter().map(|value| value.binding.as_str()).collect::<BTreeSet<_>>();
    let reachable = ir::effective_reachable_blocks(body);
    let mut required = BTreeSet::new();
    let mut actual_inputs = BTreeSet::new();
    let mut actual_outputs = BTreeSet::new();
    let mut definitions = HashMap::new();
    for block in body.blocks.iter().filter(|block| reachable.contains(&block.id)) {
        for instruction in &block.instructions {
            match instruction {
                IrInstruction::Consume { operand: IrOperand::Var(value) } if input_ids.contains(&value.id) => {
                    required.insert((block.id.0, value.id, true));
                    actual_inputs.insert(value.id);
                }
                IrInstruction::Create { dest, pattern } if pattern.ty == schema && output_names.contains(pattern.binding.as_str()) => {
                    required.insert((block.id.0, dest.id, false));
                    actual_outputs.insert(pattern.binding.as_str());
                }
                _ => {}
            }
            let dest = match instruction {
                IrInstruction::Binary { dest, .. } | IrInstruction::Move { dest, .. } | IrInstruction::FieldAccess { dest, .. } => {
                    Some(dest)
                }
                _ => None,
            };
            if let Some(dest) = dest {
                definitions.entry(dest.id).and_modify(|value| *value = None).or_insert(Some((block.id.0, instruction)));
            }
        }
    }
    if required.is_empty() || actual_inputs != input_ids || actual_outputs != output_names {
        return None;
    }
    let mut covered = BTreeSet::new();
    let mut pools = BTreeMap::<(&str, usize), (&IrPoolAccounting, Vec<&IrLifecycleExecution>)>::new();
    for source in sources {
        let pool_id = match &source.kind {
            IrSourceDispositionKind::PooledInput { pool_id, .. } | IrSourceDispositionKind::PoolResult { pool_id, .. } => pool_id,
            _ => continue,
        };
        let execution = source.execution.as_ref()?;
        let event = (execution.operation_block.0, execution.local_id, source.input_binding.is_some());
        if !required.contains(&event) {
            continue;
        }
        if !covered.insert(event) {
            return None;
        }
        let pool = execution.pool.as_ref()?;
        let group = pools.entry((pool_id, execution.occurrence)).or_insert_with(|| (pool, Vec::new()));
        if group.0 != pool {
            return None;
        }
        group.1.push(execution);
    }
    if covered != required || pools.is_empty() {
        return None;
    }
    let blocks = body.blocks.iter().map(|block| (block.id.0, block)).collect::<HashMap<_, _>>();
    for (pool, events) in pools.values() {
        if pool.inputs.is_empty()
            || pool.outputs.is_empty()
            || pool.conserved_fields.is_empty()
            || events.len() != pool.inputs.len() + pool.outputs.len()
            || pool.inputs.iter().collect::<BTreeSet<_>>().len() != pool.inputs.len()
            || pool.outputs.iter().collect::<BTreeSet<_>>().len() != pool.outputs.len()
        {
            return None;
        }
        for field in &pool.conserved_fields {
            let checked = body.blocks.iter().any(|guard| {
                let IrTerminator::Branch { cond: IrOperand::Var(condition), then_block, else_block } = &guard.terminator else {
                    return false;
                };
                if !blocks
                    .get(&else_block.0)
                    .is_some_and(|failure| failure.runtime_error == Some(CellScriptRuntimeError::AssertionFailed))
                    || !events.iter().all(|event| dominates(body, &blocks, then_block.0, event.operation_block.0))
                {
                    return false;
                }
                let Some((defined_at, IrInstruction::Binary { op: BinaryOp::Eq, left, right, .. })) =
                    definitions.get(&condition.id).and_then(Option::as_ref)
                else {
                    return false;
                };
                if !dominates(body, &blocks, *defined_at, guard.id.0)
                    || operand_type(left) != Some(&IrType::U128)
                    || operand_type(right) != Some(&IrType::U128)
                {
                    return false;
                }
                let terms = |value| sum_terms(value, field, guard.id.0, body, &blocks, &definitions);
                let expected = |members: &[usize]| members.iter().map(|member| (*member, 1)).collect::<BTreeMap<_, _>>();
                let left = terms(left);
                let right = terms(right);
                left == Some(expected(&pool.inputs)) && right == Some(expected(&pool.outputs))
                    || right == Some(expected(&pool.inputs)) && left == Some(expected(&pool.outputs))
            });
            if !checked {
                return None;
            }
        }
    }
    Some(format!("Explicit pool conservation is checked by executable u128 field-sum guards across {} input/output role occurrences; all resource operations have bound pool evidence", covered.len()))
}

fn operand_type(value: &IrOperand) -> Option<&IrType> {
    match value {
        IrOperand::Var(value) => Some(&value.ty),
        _ => None,
    }
}

fn successors(block: &IrBlock) -> Vec<usize> {
    if block.runtime_error.is_some() {
        return Vec::new();
    }
    match &block.terminator {
        IrTerminator::Return(_) => Vec::new(),
        IrTerminator::Jump(next) => vec![next.0],
        IrTerminator::Branch { cond: IrOperand::Const(IrConst::Bool(value)), then_block, else_block } => {
            vec![if *value { then_block.0 } else { else_block.0 }]
        }
        IrTerminator::Branch { then_block, else_block, .. } => vec![then_block.0, else_block.0],
    }
}

fn dominates(body: &IrBody, blocks: &HashMap<usize, &IrBlock>, parent: usize, child: usize) -> bool {
    if !blocks.contains_key(&parent) || !blocks.contains_key(&child) {
        return false;
    }
    let Some(entry) = body.blocks.first() else { return false };
    let mut pending = vec![entry.id.0];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if id == parent || !seen.insert(id) {
            continue;
        }
        if id == child {
            return false;
        }
        let Some(block) = blocks.get(&id) else { return false };
        pending.extend(successors(block));
    }
    ir::effective_reachable_blocks(body).iter().any(|block| block.0 == child)
}

fn sum_terms(
    value: &IrOperand,
    field: &str,
    at: usize,
    body: &IrBody,
    blocks: &HashMap<usize, &IrBlock>,
    definitions: &HashMap<usize, Option<(usize, &IrInstruction)>>,
) -> Option<BTreeMap<usize, usize>> {
    let mut pending = vec![value];
    let mut seen = BTreeSet::new();
    let mut result = BTreeMap::new();
    while let Some(value) = pending.pop() {
        let IrOperand::Var(value) = value else { return None };
        if !seen.insert(value.id) {
            return None;
        }
        let (defined_at, instruction) = definitions.get(&value.id)?.as_ref()?;
        if !dominates(body, blocks, *defined_at, at) {
            return None;
        }
        match instruction {
            IrInstruction::FieldAccess { obj: IrOperand::Var(owner), field: actual, .. }
                if actual == field && matches!(value.ty, IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64 | IrType::U128) =>
            {
                *result.entry(owner.id).or_default() += 1;
            }
            IrInstruction::Move { src, .. }
                if operand_type(src)
                    .and_then(unsigned_width)
                    .zip(unsigned_width(&value.ty))
                    .is_some_and(|(source, target)| source <= target) =>
            {
                pending.push(src)
            }
            IrInstruction::Binary { op: BinaryOp::Add, left, right, .. }
                if value.ty == IrType::U128
                    && operand_type(left) == Some(&IrType::U128)
                    && operand_type(right) == Some(&IrType::U128) =>
            {
                pending.extend([left, right]);
            }
            _ => return None,
        }
    }
    Some(result)
}

fn unsigned_width(ty: &IrType) -> Option<u32> {
    match ty {
        IrType::U8 => Some(8),
        IrType::U16 => Some(16),
        IrType::U32 => Some(32),
        IrType::U64 => Some(64),
        IrType::U128 => Some(128),
        _ => None,
    }
}
