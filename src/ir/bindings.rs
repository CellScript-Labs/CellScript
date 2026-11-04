//! Resolve named Cell locations once, before storage layout and metadata.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IrCellBindingRole {
    Input,
    Output,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IrCellSource {
    Input,
    Output,
    GroupInput,
    GroupOutput,
    CellDep,
}

impl IrCellSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::GroupInput => "group-input",
            Self::GroupOutput => "group-output",
            Self::CellDep => "cell-dep",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrCellMembership {
    Unproven,
    CurrentTypeGroup,
    CurrentLockGroup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrCellBinding {
    pub binding: String,
    pub role: IrCellBindingRole,
    pub local_id: Option<usize>,
    pub ty: String,
    pub source: IrCellSource,
    pub ordinal: usize,
    pub membership: IrCellMembership,
}

impl IrBody {
    pub fn cell_binding(&self, role: IrCellBindingRole, binding: &str) -> Option<&IrCellBinding> {
        self.cell_bindings.iter().find(|entry| entry.role == role && entry.binding == binding)
    }

    pub fn cell_binding_for_local(&self, local_id: usize) -> Option<&IrCellBinding> {
        self.cell_bindings.iter().find(|entry| entry.local_id == Some(local_id))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BindingContext {
    LegacyEntry,
    LegacyLockEntry,
    NativeTypeGroup,
    NativeLockGroup,
    Helper,
}

fn schema_name(ty: &IrType) -> Option<&str> {
    match ty {
        IrType::Named(name) => Some(name),
        IrType::Ref(inner) | IrType::MutRef(inner) => schema_name(inner),
        _ => None,
    }
}

pub(super) fn resolve(
    params: &[IrParam],
    body: &IrBody,
    context: BindingContext,
    types: &HashMap<String, IrTypeKind>,
) -> Vec<IrCellBinding> {
    let mut vars = params.iter().map(|param| (param.name.clone(), param.binding.clone())).collect::<BTreeMap<_, _>>();
    for instruction in body.blocks.iter().flat_map(|block| &block.instructions) {
        let (dest, input) = match instruction {
            IrInstruction::ReadRef { dest, .. } | IrInstruction::Create { dest, .. } | IrInstruction::CreateUnique { dest, .. } => {
                (Some(dest), None)
            }
            IrInstruction::Transfer { dest, operand, .. }
            | IrInstruction::ReplaceUnique { dest, operand, .. }
            | IrInstruction::Settle { dest, operand, .. } => (Some(dest), Some(operand)),
            IrInstruction::Claim { dest, receipt } => (Some(dest), Some(receipt)),
            IrInstruction::Consume { operand } | IrInstruction::Destroy { operand, .. } => (None, Some(operand)),
            _ => (None, None),
        };
        for var in dest.into_iter().chain(input.and_then(|operand| match operand {
            IrOperand::Var(var) => Some(var),
            _ => None,
        })) {
            vars.entry(var.name.clone()).or_insert_with(|| var.clone());
        }
    }
    let native_type = context == BindingContext::NativeTypeGroup;
    let input_source = if native_type { IrCellSource::GroupInput } else { IrCellSource::Input };
    let output_source = if native_type { IrCellSource::GroupOutput } else { IrCellSource::Output };
    let membership = if native_type { IrCellMembership::CurrentTypeGroup } else { IrCellMembership::Unproven };
    let native_inputs = params
        .iter()
        .filter(|param| param.source == ParamSource::Input)
        .enumerate()
        .map(|(index, param)| (param.name.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let native_outputs = params
        .iter()
        .filter(|param| param.source == ParamSource::Output)
        .enumerate()
        .map(|(index, param)| (param.name.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut records = BTreeMap::<(IrCellBindingRole, String), IrCellBinding>::new();
    let mut insert = |role, binding: &str, ty: Option<&str>, source, ordinal, membership, has_local: bool| {
        let var = vars.get(binding);
        let Some(ty) = ty.or_else(|| var.and_then(|var| schema_name(&var.ty))) else {
            return;
        };
        records.insert(
            (role, binding.to_string()),
            IrCellBinding {
                binding: binding.to_string(),
                role,
                local_id: has_local.then(|| var.map(|var| var.id)).flatten(),
                ty: ty.to_string(),
                source,
                ordinal,
                membership,
            },
        );
    };
    for (ordinal, pattern) in body.consume_set.iter().enumerate() {
        let ordinal = if native_type { native_inputs.get(pattern.binding.as_str()).copied().unwrap_or(ordinal) } else { ordinal };
        insert(IrCellBindingRole::Input, &pattern.binding, None, input_source, ordinal, membership, true);
    }
    for pattern in &body.mutate_set {
        insert(IrCellBindingRole::Input, &pattern.binding, Some(&pattern.ty), input_source, pattern.input_index, membership, true);
        insert(IrCellBindingRole::Output, &pattern.binding, Some(&pattern.ty), output_source, pattern.output_index, membership, false);
    }
    if context != BindingContext::Helper {
        let mut read_input_ordinal = body.consume_set.len() + body.mutate_set.len();
        let mut protected_ordinal = 0;
        for param in params {
            let Some(ty) = schema_name(&param.ty) else {
                continue;
            };
            if !types.get(ty).is_some_and(|kind| matches!(kind, IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt))
                && !param.is_ref
            {
                continue;
            }
            if matches!(param.source, ParamSource::Output | ParamSource::LockArgs)
                || param.is_read_ref
                || body.consume_set.iter().any(|pattern| pattern.binding == param.name)
                || body.mutate_set.iter().any(|pattern| pattern.binding == param.name)
            {
                continue;
            }
            let (source, ordinal, membership) = if matches!(context, BindingContext::NativeLockGroup | BindingContext::LegacyLockEntry)
                && param.source == ParamSource::Protected
            {
                let ordinal = protected_ordinal;
                protected_ordinal += 1;
                (IrCellSource::GroupInput, ordinal, IrCellMembership::CurrentLockGroup)
            } else {
                let ordinal = if native_type {
                    native_inputs.get(param.name.as_str()).copied().unwrap_or(read_input_ordinal)
                } else {
                    read_input_ordinal
                };
                read_input_ordinal += 1;
                (input_source, ordinal, membership)
            };
            insert(IrCellBindingRole::ReadOnly, &param.name, Some(ty), source, ordinal, membership, true);
        }
    }
    let mut records = records.into_values().collect::<Vec<_>>();
    let mut output_occurrences = BTreeMap::<(&str, &str), usize>::new();
    for (ordinal, pattern) in body.create_set.iter().enumerate() {
        if pattern.operation == "bounded-create" {
            continue;
        }
        let occurrence = output_occurrences.entry((&pattern.operation, &pattern.binding)).or_default();
        let dest = body
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction {
                IrInstruction::Create { dest, pattern: candidate }
                | IrInstruction::CreateUnique { dest, pattern: candidate, .. }
                | IrInstruction::ReplaceUnique { dest, pattern: candidate, .. }
                    if candidate.operation == pattern.operation && candidate.binding == pattern.binding =>
                {
                    Some(dest)
                }
                IrInstruction::Transfer { dest, .. } if pattern.operation == "transfer" && dest.name == pattern.binding => Some(dest),
                IrInstruction::Claim { dest, .. } if pattern.operation == "claim" && dest.name == pattern.binding => Some(dest),
                IrInstruction::Settle { dest, .. } if pattern.operation == "settle" && dest.name == pattern.binding => Some(dest),
                _ => None,
            })
            .nth(*occurrence);
        *occurrence += 1;
        let local_id = params
            .iter()
            .find(|param| param.source == ParamSource::Output && param.name == pattern.binding)
            .map(|param| param.binding.id)
            .or_else(|| dest.map(|dest| dest.id));
        let ordinal = if native_type { native_outputs.get(pattern.binding.as_str()).copied().unwrap_or(ordinal) } else { ordinal };
        records.push(IrCellBinding {
            binding: pattern.binding.clone(),
            role: IrCellBindingRole::Output,
            local_id,
            ty: pattern.ty.clone(),
            source: output_source,
            ordinal,
            membership,
        });
    }
    let parameter_reads =
        params.iter().filter(|param| context != BindingContext::Helper && param.is_read_ref).map(|param| &param.binding);
    let expression_reads = body.blocks.iter().flat_map(|block| &block.instructions).filter_map(|instruction| match instruction {
        IrInstruction::ReadRef { dest, .. } => Some(dest),
        _ => None,
    });
    for (ordinal, var) in parameter_reads.chain(expression_reads).enumerate() {
        let Some(ty) = schema_name(&var.ty) else {
            continue;
        };
        records.push(IrCellBinding {
            binding: var.name.clone(),
            role: IrCellBindingRole::ReadOnly,
            local_id: Some(var.id),
            ty: ty.to_string(),
            source: IrCellSource::CellDep,
            ordinal,
            membership: IrCellMembership::Unproven,
        });
    }
    records.sort_by(|left, right| {
        (left.role, &left.binding, left.local_id, left.source, left.ordinal, &left.ty).cmp(&(
            right.role,
            &right.binding,
            right.local_id,
            right.source,
            right.ordinal,
            &right.ty,
        ))
    });
    // A native named successor can occur both in the signature and in an
    // explicit create_unique relation. Those are aliases of one physical Cell;
    // anonymous creates retain distinct local IDs and ordinals even when their
    // generated binding names coincide.
    records.dedup();
    records
}
