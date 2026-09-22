//! Deterministic scalar-only slot reuse. Any unclassified instruction, pointer,
//! resource binding or external call retains the original function layout.
//!
//! Named mutable-variable slots, buffers, wide values, helper scratch and
//! outgoing arguments are allocated separately by frame.rs. No location claim
//! is exported from these offsets alone: interference is computed over the CFG.

use super::*;

type Variables = BTreeSet<usize>;
type Effect = (Variables, Variables);

pub(super) fn allocate(
    body: &IrBody,
    params: &[IrParam],
    abis: &HashMap<String, CallableAbi>,
    local_calls: &BTreeSet<String>,
) -> Option<BTreeMap<usize, usize>> {
    if !body.cell_bindings.is_empty()
        || !body.consume_set.is_empty()
        || !body.read_refs.is_empty()
        || !body.create_set.is_empty()
        || !body.mutate_set.is_empty()
        || !body.write_intents.is_empty()
        || !body.bounded_collection_ops.is_empty()
        || !body.borrow_regions.is_empty()
        || !body.trusted_external_calls.is_empty()
        || params.iter().any(|param| !scalar(&param.ty) || param.is_ref || param.is_read_ref)
    {
        return None;
    }
    let mut variables: Variables = params.iter().map(|param| param.binding.id).collect();
    let mut effects = BTreeMap::new();
    let mut successors = BTreeMap::new();
    for block in &body.blocks {
        let mut instructions = Vec::new();
        for instruction in &block.instructions {
            let (uses, defs) = instruction_effect(instruction, abis, local_calls)?;
            variables.extend(uses.iter().chain(&defs).copied());
            instructions.push((uses, defs));
        }
        let mut terminal = Variables::new();
        let next = match &block.terminator {
            IrTerminator::Return(Some(value)) => {
                operand(value, &mut terminal)?;
                vec![]
            }
            IrTerminator::Return(None) => vec![],
            IrTerminator::Jump(next) => vec![next.0],
            IrTerminator::Branch { cond, then_block, else_block } => {
                operand(cond, &mut terminal)?;
                vec![then_block.0, else_block.0]
            }
        };
        variables.extend(&terminal);
        instructions.push((terminal, Variables::new()));
        if effects.insert(block.id.0, instructions).is_some() {
            return None;
        }
        successors.insert(block.id.0, next);
    }
    if successors.values().flatten().any(|next| !effects.contains_key(next)) {
        return None;
    }
    let (live_in, live_out) = liveness(&effects, &successors);
    let mut interference: BTreeMap<_, Variables> = variables.iter().map(|var| (*var, Variables::new())).collect();
    // Every parameter is spilled on entry, including unused parameters.
    // Distinct parameter slots prevent a later dead spill overwriting a live one.
    clique(&params.iter().map(|param| param.binding.id).collect(), &mut interference);
    for (block, instructions) in &effects {
        let mut live = live_out[block].clone();
        clique(&live, &mut interference);
        for (uses, defs) in instructions.iter().rev() {
            for defined in defs {
                for other in &live {
                    connect(*defined, *other, &mut interference);
                }
            }
            live = live.difference(defs).copied().collect();
            live.extend(uses);
            clique(&live, &mut interference);
        }
        debug_assert_eq!(live, live_in[block]);
    }
    let mut slots = BTreeMap::new();
    for variable in variables {
        let occupied: BTreeSet<_> = interference[&variable].iter().filter_map(|other| slots.get(other).copied()).collect();
        let slot = (0..).map(|index| index * 8).find(|offset| !occupied.contains(offset))?;
        slots.insert(variable, slot);
    }
    Some(slots)
}

fn scalar(ty: &IrType) -> bool {
    matches!(ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64)
}

fn operand(value: &IrOperand, uses: &mut Variables) -> Option<()> {
    match value {
        IrOperand::Var(var) if scalar(&var.ty) => {
            uses.insert(var.id);
            Some(())
        }
        IrOperand::Const(IrConst::Unit | IrConst::Bool(_) | IrConst::U8(_) | IrConst::U16(_) | IrConst::U32(_) | IrConst::U64(_)) => {
            Some(())
        }
        _ => None,
    }
}

fn instruction_effect(
    instruction: &IrInstruction,
    abis: &HashMap<String, CallableAbi>,
    local_calls: &BTreeSet<String>,
) -> Option<Effect> {
    let mut uses = Variables::new();
    let dest = match instruction {
        IrInstruction::LoadConst { dest, value } => {
            operand(&IrOperand::Const(value.clone()), &mut uses)?;
            Some(dest)
        }
        IrInstruction::LoadVar { dest, .. } => Some(dest),
        IrInstruction::StoreVar { src, .. } => {
            operand(src, &mut uses)?;
            None
        }
        IrInstruction::Move { dest, src } => {
            operand(src, &mut uses)?;
            Some(dest)
        }
        IrInstruction::Unary { dest, operand: value, .. } => {
            operand(value, &mut uses)?;
            Some(dest)
        }
        IrInstruction::Binary { dest, left, right, .. } => {
            operand(left, &mut uses)?;
            operand(right, &mut uses)?;
            Some(dest)
        }
        IrInstruction::Call { dest, func, args } => {
            let abi = abis.get(func)?;
            if !local_calls.contains(func)
                || !abi.type_hash_param_indices.is_empty()
                || !abi.runtime_bound_param_indices.is_empty()
                || !abi.bounded_plan_param_indices.is_empty()
                || abi.params.iter().any(|param| !scalar(&param.ty) || param.is_ref || param.is_read_ref)
            {
                return None;
            }
            for arg in args {
                operand(arg, &mut uses)?;
            }
            dest.as_ref()
        }
        _ => return None,
    };
    let mut defs = Variables::new();
    if let Some(dest) = dest {
        if !scalar(&dest.ty) {
            return None;
        }
        defs.insert(dest.id);
    }
    Some((uses, defs))
}

fn liveness(
    effects: &BTreeMap<usize, Vec<Effect>>,
    successors: &BTreeMap<usize, Vec<usize>>,
) -> (BTreeMap<usize, Variables>, BTreeMap<usize, Variables>) {
    let mut input: BTreeMap<_, Variables> = effects.keys().map(|block| (*block, Variables::new())).collect();
    let mut output = input.clone();
    loop {
        let mut changed = false;
        for (block, instructions) in effects.iter().rev() {
            let after: Variables = successors[block].iter().flat_map(|next| input[next].iter().copied()).collect();
            let mut before = after.clone();
            for (uses, defs) in instructions.iter().rev() {
                before = before.difference(defs).copied().collect();
                before.extend(uses);
            }
            changed |= input[block] != before || output[block] != after;
            input.insert(*block, before);
            output.insert(*block, after);
        }
        if !changed {
            return (input, output);
        }
    }
}

fn connect(left: usize, right: usize, graph: &mut BTreeMap<usize, Variables>) {
    if left != right {
        graph.get_mut(&left).expect("classified variable").insert(right);
        graph.get_mut(&right).expect("classified variable").insert(left);
    }
}

fn clique(live: &Variables, graph: &mut BTreeMap<usize, Variables>) {
    for left in live {
        for right in live.range((std::ops::Bound::Excluded(left), std::ops::Bound::Unbounded)) {
            connect(*left, *right, graph);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liveness_keeps_loop_carried_and_joined_values() {
        let set = |values: &[usize]| values.iter().copied().collect::<Variables>();
        let effects = BTreeMap::from([
            (0, vec![(set(&[]), set(&[0, 1]))]),
            (1, vec![(set(&[0, 1]), set(&[2]))]),
            (2, vec![(set(&[2]), set(&[0]))]),
            (3, vec![(set(&[0, 1]), set(&[]))]),
        ]);
        let successors = BTreeMap::from([(0, vec![1]), (1, vec![2, 3]), (2, vec![1]), (3, vec![])]);
        let (input, output) = liveness(&effects, &successors);
        assert_eq!(input[&1], set(&[0, 1]));
        assert_eq!(output[&1], set(&[0, 1, 2]));
        assert_eq!(input[&2], set(&[1, 2]));
        assert_eq!(input[&3], set(&[0, 1]));
    }
}
