//! Helper availability is a property of actual accepting paths, not the union
//! of calls in the module. Arguments are absent from this conservation helper;
//! owner-or-conservation and delta helpers establish different propositions.
use super::{auto_group_conservation_helper, XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER};
use crate::ir::{self, IrBody, IrConst, IrInstruction, IrItem, IrModule, IrOperand, IrTerminator};
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) fn body_checks_conservation(body: &IrBody, checked: &HashSet<&str>) -> bool {
    body_checks_helper(body, XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER, checked)
}

pub(crate) fn body_checks_fungible_profile(body: &IrBody) -> bool {
    body_checks_helper(body, super::FUNGIBLE_TYPE_GROUP_V1_CODEGEN_HELPER, &HashSet::new())
}

fn body_checks_helper(body: &IrBody, required_helper: &str, checked: &HashSet<&str>) -> bool {
    let Some(entry) = body.blocks.first() else { return false };
    let blocks = body.blocks.iter().map(|block| (block.id, block)).collect::<HashMap<_, _>>();
    let mut pending = VecDeque::from([(entry.id, false)]);
    let mut visited = HashSet::new();
    let mut checked_return = false;
    while let Some((id, mut executed)) = pending.pop_front() {
        if !visited.insert((id, executed)) {
            continue;
        }
        let Some(block) = blocks.get(&id) else { return false };
        if block.runtime_error.is_some() {
            continue;
        }
        for instruction in &block.instructions {
            if let IrInstruction::Call { func, .. } = instruction {
                executed |= match func {
                    ir::IrCallTarget::Runtime(helper) => helper.symbol() == required_helper,
                    ir::IrCallTarget::Function(name) => checked.contains(name.as_str()),
                    ir::IrCallTarget::InvalidRuntime(_) => false,
                };
            }
        }
        match &block.terminator {
            IrTerminator::Return(_) => {
                if !executed {
                    return false;
                }
                checked_return = true;
            }
            IrTerminator::Jump(target) => pending.push_back((*target, executed)),
            IrTerminator::Branch { cond: IrOperand::Const(IrConst::Bool(value)), then_block, else_block } => {
                pending.push_back((if *value { *then_block } else { *else_block }, executed));
            }
            IrTerminator::Branch { then_block, else_block, .. } => {
                pending.push_back((*then_block, executed));
                pending.push_back((*else_block, executed));
            }
        }
    }
    checked_return
}

pub(crate) fn conservation_checked_for_selected_entries(module: &IrModule) -> bool {
    let callables = module
        .items
        .iter()
        .filter_map(|item| match item {
            IrItem::Action(action) => {
                Some((action.name.as_str(), &action.body, auto_group_conservation_helper(module, action).is_some()))
            }
            IrItem::Function(function) => Some((function.name.as_str(), &function.body, false)),
            IrItem::Lock(lock) => Some((lock.name.as_str(), &lock.body, false)),
            IrItem::TypeDef(_) | IrItem::Invariant(_) => None,
        })
        .collect::<Vec<_>>();
    let mut checked = callables.iter().filter_map(|(name, _, prelude)| prelude.then_some(*name)).collect::<HashSet<_>>();
    loop {
        let before = checked.len();
        for (name, body, _) in &callables {
            if !checked.contains(name) && body_checks_conservation(body, &checked) {
                checked.insert(*name);
            }
        }
        if before == checked.len() {
            break;
        }
    }
    match &module.entry_selection {
        ir::IrEntrySelection::Artifact(declaration) => {
            if !matches!(declaration.context, crate::artifact::ArtifactContext::TypeGroup {}) {
                return false;
            }
            if declaration.common_checks.iter().any(|name| checked.contains(name.as_str())) {
                return true;
            }
            if let Some(name) = &declaration.entry {
                return checked.contains(name.as_str());
            }
            !declaration.actions.is_empty() && declaration.actions.iter().all(|entry| checked.contains(entry.action.as_str()))
        }
        _ => matches!(module.resolved_entry(), Some(ir::IrEntryPoint::Action(action)) if checked.contains(action.name.as_str())),
    }
}
