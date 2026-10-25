//! Bounded host reconstruction of the source-declared selection on concrete
//! transaction Cells. This is builder evidence, not live-state or VM execution.
//! Layouts come from the same checked typed record as the machine contract;
//! canonical nested bytes use the existing independent Molecule validator.
use super::{layout, Contract, Correspondence};
use crate::nested_machine::cells::Field;
use crate::TypedSemanticType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

mod contract;
pub use contract::{project, Catalog, Site};

#[derive(Clone, Copy, Debug)]
pub struct CurrentScript {
    pub hash: [u8; 32],
    pub is_type: bool,
}

/// Hashes must be computed from the complete packed Scripts by the caller.
/// Input data must come from resolved inputs, never a witness index proposal.
#[derive(Clone, Copy, Debug)]
pub struct Cell<'a> {
    pub type_hash: Option<[u8; 32]>,
    pub lock_hash: [u8; 32],
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Input,
    Output,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedCell {
    pub side: Side,
    pub index: u32,
    pub group_index: Option<u32>,
    pub role: u32,
    pub key: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub role: u32,
    pub inputs: Vec<u32>,
    pub outputs: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub cells: Vec<SelectedCell>,
    pub pairs: Vec<Pair>,
    pub aggregate_bytes: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Contract,
    Authority,
    ScanBound,
    Custody,
    Layout,
    Codec,
    SelectedBound,
    ByteBound,
    DuplicateIdentity,
    Cardinality,
    Correspondence,
    Conditional,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Contract => "invalid or unsupported typed role contract",
            Self::Authority => "local role differs from the current Type Script",
            Self::ScanBound => "transaction exceeds the declared complete scan bound",
            Self::Custody => "protected Cell has the wrong Lock identity",
            Self::Layout => "protected Cell differs from its complete layout",
            Self::Codec => "protected Cell contains noncanonical or out-of-bound nested data",
            Self::SelectedBound => "selected Cells exceed the per-side bound",
            Self::ByteBound => "selected data exceeds the aggregate byte bound",
            Self::DuplicateIdentity => "duplicate protected identity within a role and side",
            Self::Cardinality => "protected role count is outside its declared interval",
            Self::Correspondence => "protected inputs and outputs violate correspondence",
            Self::Conditional => "conditional selection requires an execution-path binding before builder admission",
        })
    }
}
impl std::error::Error for Error {}

/// Reconstruct all membership and pairs without accepting caller-proposed
/// indexes. At most 128 Cells are scanned; each decoded Cell is at most 32768
/// bytes and accepted protected data is charged against the aggregate budget.
pub fn select(
    contract: &Contract,
    types: &[TypedSemanticType],
    current: CurrentScript,
    inputs: &[Cell<'_>],
    outputs: &[Cell<'_>],
) -> Result<Selection, Error> {
    // Bound descriptor work before collecting or deriving schema maps.
    if types.len() > 512 || types.iter().any(|ty| ty.fields.len() > 256) {
        return Err(Error::Contract);
    }
    let schemas = types.iter().map(|ty| (ty.name.as_str(), ty)).collect::<BTreeMap<_, _>>();
    if schemas.len() != types.len() {
        return Err(Error::Contract);
    }
    let layouts = layout::derive(contract, &schemas).map_err(|_| Error::Contract)?;
    if contract.roles.iter().any(|role| role.local_type && (!current.is_type || role.type_hash != current.hash)) {
        return Err(Error::Authority);
    }
    if inputs.len() > contract.input_scan as usize || outputs.len() > contract.output_scan as usize {
        return Err(Error::ScanBound);
    }
    let mut result = Selection { cells: Vec::new(), pairs: Vec::new(), aggregate_bytes: 0 };
    for (side, cells) in [(Side::Input, inputs), (Side::Output, outputs)] {
        let mut group = 0;
        let mut count = 0;
        for (index, cell) in cells.iter().enumerate() {
            let in_group = if current.is_type {
                cell.type_hash == Some(current.hash)
            } else {
                side == Side::Input && cell.lock_hash == current.hash
            };
            let group_index = in_group.then_some(group);
            group += u32::from(in_group);
            let Some(role_index) = contract.roles.iter().position(|role| Some(role.type_hash) == cell.type_hash) else { continue };
            let role = &contract.roles[role_index];
            if cell.lock_hash != role.lock_hash {
                return Err(Error::Custody);
            }
            let key = identity(&layouts[role_index], cell.data)?;
            count += 1;
            if count > contract.selected_per_side {
                return Err(Error::SelectedBound);
            }
            let bytes = u32::try_from(cell.data.len()).map_err(|_| Error::ByteBound)?;
            result.aggregate_bytes = result.aggregate_bytes.checked_add(bytes).ok_or(Error::ByteBound)?;
            if result.aggregate_bytes > contract.aggregate_bytes {
                return Err(Error::ByteBound);
            }
            if result.cells.iter().any(|other| other.side == side && other.role == role_index as u32 && other.key == key) {
                return Err(Error::DuplicateIdentity);
            }
            result.cells.push(SelectedCell { side, index: index as u32, group_index, role: role_index as u32, key: key.to_vec() });
        }
    }
    for (index, role) in contract.roles.iter().enumerate() {
        let inputs = result.cells.iter().filter(|cell| cell.role == index as u32 && cell.side == Side::Input).collect::<Vec<_>>();
        let outputs = result.cells.iter().filter(|cell| cell.role == index as u32 && cell.side == Side::Output).collect::<Vec<_>>();
        if [(role.inputs, inputs.len()), (role.outputs, outputs.len())]
            .iter()
            .any(|((min, max), count)| *count < *min as usize || *count > *max as usize)
        {
            return Err(Error::Cardinality);
        }
        let pair = |inputs, outputs| Pair { role: index as u32, inputs, outputs };
        match &role.correspondence {
            Correspondence::Preserve if inputs.len() == outputs.len() => {
                for input in inputs {
                    let output = outputs.iter().find(|output| output.key == input.key).ok_or(Error::Correspondence)?;
                    result.pairs.push(pair(vec![input.index], vec![output.index]));
                }
            }
            Correspondence::Create if inputs.is_empty() && !outputs.is_empty() => {
                result.pairs.extend(outputs.iter().map(|cell| pair(vec![], vec![cell.index])));
            }
            Correspondence::Burn if !inputs.is_empty() && outputs.is_empty() => {
                result.pairs.extend(inputs.iter().map(|cell| pair(vec![cell.index], vec![])));
            }
            Correspondence::Merge(target) if inputs.len() >= 2 && outputs.len() == 1 && outputs[0].key == *target => {
                result.pairs.push(pair(inputs.iter().map(|cell| cell.index).collect(), vec![outputs[0].index]));
            }
            _ => return Err(Error::Correspondence),
        }
    }
    Ok(result)
}

fn identity<'a>(layout: &layout::Layout, data: &'a [u8]) -> Result<&'a [u8], Error> {
    if let Some(table) = &layout.nested {
        if data.len() > layout.width as usize {
            return Err(Error::Layout);
        }
        let fields = crate::code_origin::molecule::fields(data, table.fields.len()).map_err(|_| Error::Codec)?;
        for (field, bytes) in table.fields.iter().zip(&fields) {
            match field {
                Field::Fixed(width) if bytes.len() != *width as usize => return Err(Error::Layout),
                Field::Values(contract) => {
                    contract.check(bytes).map_err(|_| Error::Codec)?;
                }
                _ => {}
            }
        }
        return fields.get(table.key_field as usize).copied().ok_or(Error::Contract);
    }
    if data.len() != layout.width as usize {
        return Err(Error::Layout);
    }
    data.get(layout.key_offset as usize..(layout.key_offset + layout.key_width) as usize).ok_or(Error::Contract)
}
