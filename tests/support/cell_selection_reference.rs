//! Independent host oracle for the proposed exact-role Cell-set contract.
//! No compiler imports, transaction serializer, or claim of VM enforcement.

pub type Hash = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptRole {
    Lock,
    Type,
}

#[derive(Clone, Copy, Debug)]
pub struct CurrentScript {
    pub role: ScriptRole,
    pub hash: Hash,
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub type_hash: Option<Hash>,
    pub lock_hash: Hash,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Observe,
    LocalType,
}

#[derive(Clone, Debug)]
pub enum Correspondence {
    Preserve,
    Create,
    Burn,
    Merge { target_key: Vec<u8> },
}

#[derive(Clone, Copy, Debug)]
pub struct Count {
    pub min: usize,
    pub max: usize,
}

#[derive(Clone, Debug)]
pub struct Role {
    pub type_hash: Hash,
    pub lock_hash: Hash,
    // Supplied by the admitted typed contract, never read from untrusted Cell data.
    pub schema: Hash,
    pub width: usize,
    pub key_offset: usize,
    pub key_width: usize,
    pub inputs: Count,
    pub outputs: Count,
    pub access: Access,
    pub correspondence: Correspondence,
}

#[derive(Clone, Debug)]
pub struct Contract {
    pub roles: Vec<Role>,
    pub input_scan: usize,
    pub output_scan: usize,
    pub selected_per_side: usize,
    pub aggregate_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Contract,
    AmbiguousRole,
    Authority,
    ScanBound,
    LockIdentity,
    Layout,
    SelectedBound,
    ByteBound,
    DuplicateKey,
    Cardinality,
    Correspondence,
    ProposalBound,
    ProposalIndex,
    DuplicateIndex,
    ProposalMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selected {
    pub side: Side,
    pub index: usize,
    pub group_index: Option<usize>,
    pub role: usize,
    pub schema: Hash,
    pub access: Access,
    pub key: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub role: usize,
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub cells: Vec<Selected>,
    pub pairs: Vec<Pair>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proposed {
    pub side: Side,
    pub index: usize,
    pub role: usize,
}

impl Contract {
    fn validate(&self, current: CurrentScript) -> Result<(), Error> {
        if !(1..=8).contains(&self.roles.len())
            || !(1..=64).contains(&self.input_scan)
            || !(1..=64).contains(&self.output_scan)
            || !(1..=32).contains(&self.selected_per_side)
            || !(1..=32768).contains(&self.aggregate_bytes)
        {
            return Err(Error::Contract);
        }
        for (i, role) in self.roles.iter().enumerate() {
            if !(1..=512).contains(&role.width)
                || !(1..=32).contains(&role.key_width)
                || role.key_offset.checked_add(role.key_width).is_none_or(|end| end > role.width)
                || [role.inputs, role.outputs].iter().any(|count| count.min > count.max || count.max > 32)
            {
                return Err(Error::Contract);
            }
            if self.roles[..i].iter().any(|other| other.type_hash == role.type_hash) {
                return Err(Error::AmbiguousRole);
            }
            if role.access == Access::LocalType && (current.role != ScriptRole::Type || current.hash != role.type_hash) {
                return Err(Error::Authority);
            }
            if let Correspondence::Merge { target_key } = &role.correspondence
                && target_key.len() != role.key_width
            {
                return Err(Error::Contract);
            }
        }
        Ok(())
    }
}

pub fn select(contract: &Contract, current: CurrentScript, inputs: &[Cell], outputs: &[Cell]) -> Result<Selection, Error> {
    contract.validate(current)?;
    // A VM implementation must establish these lengths using an end probe, not a witness count.
    if inputs.len() > contract.input_scan || outputs.len() > contract.output_scan {
        return Err(Error::ScanBound);
    }
    let mut result = Selection { cells: Vec::new(), pairs: Vec::new() };
    let mut bytes = 0usize;
    for (side, cells) in [(Side::Input, inputs), (Side::Output, outputs)] {
        let mut group_count = 0;
        let mut selected_count = 0;
        for (index, cell) in cells.iter().enumerate() {
            let in_group = match current.role {
                ScriptRole::Lock => side == Side::Input && cell.lock_hash == current.hash,
                ScriptRole::Type => cell.type_hash == Some(current.hash),
            };
            let group_index = in_group.then_some(group_count);
            group_count += usize::from(in_group);
            let Some(role_index) = contract.roles.iter().position(|role| Some(role.type_hash) == cell.type_hash) else {
                continue;
            };
            let role = &contract.roles[role_index];
            // Wrong custody is a protected-cell failure, never an exclusion predicate.
            if cell.lock_hash != role.lock_hash {
                return Err(Error::LockIdentity);
            }
            if cell.data.len() != role.width {
                return Err(Error::Layout);
            }
            selected_count += 1;
            if selected_count > contract.selected_per_side {
                return Err(Error::SelectedBound);
            }
            bytes = bytes.checked_add(cell.data.len()).ok_or(Error::ByteBound)?;
            if bytes > contract.aggregate_bytes {
                return Err(Error::ByteBound);
            }
            let key = cell.data[role.key_offset..role.key_offset + role.key_width].to_vec();
            if result.cells.iter().any(|other| other.side == side && other.role == role_index && other.key == key) {
                return Err(Error::DuplicateKey);
            }
            result.cells.push(Selected { side, index, group_index, role: role_index, schema: role.schema, access: role.access, key });
        }
    }
    for (index, role) in contract.roles.iter().enumerate() {
        let inputs = result.cells.iter().filter(|cell| cell.role == index && cell.side == Side::Input).collect::<Vec<_>>();
        let outputs = result.cells.iter().filter(|cell| cell.role == index && cell.side == Side::Output).collect::<Vec<_>>();
        if [(role.inputs, inputs.len()), (role.outputs, outputs.len())].iter().any(|(bound, len)| *len < bound.min || *len > bound.max)
        {
            return Err(Error::Cardinality);
        }
        match &role.correspondence {
            Correspondence::Preserve => {
                if inputs.len() != outputs.len() {
                    return Err(Error::Correspondence);
                }
                for input in inputs {
                    let output = outputs.iter().find(|output| output.key == input.key).ok_or(Error::Correspondence)?;
                    result.pairs.push(Pair { role: index, inputs: vec![input.index], outputs: vec![output.index] });
                }
            }
            Correspondence::Create if inputs.is_empty() && !outputs.is_empty() => {
                for output in outputs {
                    result.pairs.push(Pair { role: index, inputs: vec![], outputs: vec![output.index] });
                }
            }
            Correspondence::Burn if !inputs.is_empty() && outputs.is_empty() => {
                for input in inputs {
                    result.pairs.push(Pair { role: index, inputs: vec![input.index], outputs: vec![] });
                }
            }
            Correspondence::Merge { target_key } if inputs.len() >= 2 && outputs.len() == 1 && outputs[0].key == *target_key => {
                result.pairs.push(Pair {
                    role: index,
                    inputs: inputs.iter().map(|cell| cell.index).collect(),
                    outputs: vec![outputs[0].index],
                });
            }
            _ => return Err(Error::Correspondence),
        }
    }
    Ok(result)
}

// Proposal order is Input global-index order followed by Output global-index order.
// This checks an optional proposal against an already complete selection, not the reverse.
pub fn check_proposal(selection: &Selection, inputs: &[Cell], outputs: &[Cell], proposal: &[Proposed]) -> Result<(), Error> {
    if proposal.len() > 64 {
        return Err(Error::ProposalBound);
    }
    for (i, item) in proposal.iter().enumerate() {
        let cells = if item.side == Side::Input { inputs } else { outputs };
        if item.index >= cells.len() {
            return Err(Error::ProposalIndex);
        }
        if proposal[..i].iter().any(|other| other.side == item.side && other.index == item.index) {
            return Err(Error::DuplicateIndex);
        }
    }
    if proposal.len() != selection.cells.len()
        || proposal.iter().zip(&selection.cells).any(|(a, b)| a.side != b.side || a.index != b.index || a.role != b.role)
    {
        return Err(Error::ProposalMismatch);
    }
    Ok(())
}
