//! Independent complete-snapshot derivation for the bounded counter-set profile.
//! It proves byte consistency, not input resolution, setup trust or ZK validity.
use super::{molecule, SuppliedInputCell};
use crate::{ckb_blake2b256 as hash, zk::Statement, CheckerError, CheckerRejectionCode};

pub const COUNTER_SET_PROFILE: &str = "cellscript-zk-counter-set-v1";
const MAGIC: &[u8; 8] = b"CSZKSET1";
const LEAF_BYTES: usize = 236;

fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, format!("ZK counter set: {message}"))
}
fn require(condition: bool, message: &str) -> Result<(), CheckerError> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterSetLeaf {
    pub input_index: u32,
    pub output_index: u32,
    pub statement: Statement,
}

/// Molecule table (magic, version, family, count, raw-tx hash, fixed leaf vector).
/// Each leaf is two u32 global indices followed by the unchanged 228-byte exact
/// single-state statement. There is no caller-authorized subset or proof cache.
pub fn encode_counter_set(family: [u8; 32], leaves: &[CounterSetLeaf]) -> Result<Vec<u8>, CheckerError> {
    require((1..=4).contains(&leaves.len()), "update requires 1..4 leaves")?;
    let tx_hash = leaves[0].statement.transaction_hash;
    let mut bytes = Vec::with_capacity(112 + LEAF_BYTES * leaves.len());
    for value in [112 + LEAF_BYTES * leaves.len(), 28, 36, 40, 72, 76, 108] {
        bytes.extend((value as u32).to_le_bytes());
    }
    bytes.extend(MAGIC);
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(family);
    bytes.extend((leaves.len() as u32).to_le_bytes());
    bytes.extend(tx_hash);
    bytes.extend((leaves.len() as u32).to_le_bytes());
    let mut previous = None;
    for leaf in leaves {
        require(leaf.input_index < 64 && leaf.output_index < 64, "global index bound")?;
        require(leaf.statement.transaction_hash == tx_hash, "leaf raw transaction identity differs")?;
        if let Some((input, output)) = previous {
            require(leaf.input_index > input && leaf.output_index > output, "duplicate or reordered global index")?;
        }
        previous = Some((leaf.input_index, leaf.output_index));
        bytes.extend(leaf.input_index.to_le_bytes());
        bytes.extend(leaf.output_index.to_le_bytes());
        bytes.extend(leaf.statement.encode());
    }
    Ok(bytes)
}

pub fn decode_counter_set(bytes: &[u8]) -> Result<([u8; 32], Vec<CounterSetLeaf>), CheckerError> {
    require(bytes.len() <= 1056, "set statement byte budget")?;
    let fields = molecule::fields(bytes, 6)?;
    require(fields[0] == MAGIC && fields[1] == 1u32.to_le_bytes(), "set statement version or magic")?;
    let family: [u8; 32] = fields[2].try_into().map_err(|_| invalid("family width"))?;
    let count = u32::from_le_bytes(fields[3].try_into().map_err(|_| invalid("count width"))?) as usize;
    require((1..=4).contains(&count), "update count must be 1..4")?;
    require(fields[4].len() == 32, "raw transaction hash width")?;
    let encoded = molecule::fixed(fields[5], LEAF_BYTES, 4)?;
    require(encoded.len() == count * LEAF_BYTES, "count/vector mismatch")?;
    let leaves = encoded
        .chunks_exact(LEAF_BYTES)
        .map(|leaf| {
            Ok(CounterSetLeaf {
                input_index: u32::from_le_bytes(leaf[..4].try_into().unwrap()),
                output_index: u32::from_le_bytes(leaf[4..8].try_into().unwrap()),
                statement: Statement::decode(&leaf[8..]).map_err(|_| invalid("leaf statement codec"))?,
            })
        })
        .collect::<Result<Vec<_>, CheckerError>>()?;
    require(encode_counter_set(family, &leaves)? == bytes, "noncanonical or inconsistent set statement")?;
    Ok((family, leaves))
}

#[derive(Debug)]
pub struct CheckedCounterSet {
    selected_script: Vec<u8>,
    encoded: Vec<u8>,
    leaves: Vec<CounterSetLeaf>,
    snapshot_hash: [u8; 32],
}
impl CheckedCounterSet {
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    pub fn leaves(&self) -> &[CounterSetLeaf] {
        &self.leaves
    }
    pub fn check_statement(&self, supplied: &[u8]) -> Result<(), CheckerError> {
        decode_counter_set(supplied)?;
        require(supplied == self.encoded, "selection/count/correspondence/public-input substitution")
    }
    pub fn check_unchanged(&self, full: &[u8], inputs: &[SuppliedInputCell<'_>]) -> Result<(), CheckerError> {
        let current = check_counter_set(full, inputs, &self.selected_script)?;
        require(current.snapshot_hash == self.snapshot_hash, "transaction or input snapshot changed")
    }
}

fn state(data: &[u8]) -> Result<(), CheckerError> {
    require(data.len() == 48 && &data[..8] == b"CSZKCNT1", "noncanonical counter state")
}

pub fn check_counter_set(
    full: &[u8],
    inputs: &[SuppliedInputCell<'_>],
    selected_script: &[u8],
) -> Result<CheckedCounterSet, CheckerError> {
    require(full.len() <= 16_384 && inputs.len() <= 64 && selected_script.len() == 126, "preparse byte/count budget")?;
    let mut snapshot = full.to_vec();
    for input in inputs {
        require(input.output.len() <= 4096 && input.data.len() <= 4096, "input snapshot byte budget")?;
        snapshot.extend(input.out_point);
        snapshot.extend(hash(input.output));
        snapshot.extend(hash(input.data));
    }
    let current = molecule::script(selected_script)?;
    require(
        current.hash_type == 4
            && current.args.len() == 73
            && &current.args[..8] == MAGIC
            && current.args[40] < 4
            && current.args[41..] != [0; 32],
        "selected family Script",
    )?;
    let family: [u8; 32] = current.args[8..40].try_into().unwrap();
    let fields = molecule::fields(full, 2)?;
    let raw = molecule::transaction(fields[0])?;
    require(raw.inputs.len() / 44 == inputs.len() && raw.cells.len() <= 64, "complete input/output count")?;
    require(raw.deps.chunks_exact(37).all(|dep| dep[36] == 0), "set profile uses direct code dependencies")?;
    for witness in molecule::dynamic(fields[1], 4096)? {
        molecule::fixed(witness, 1, 16_384)?;
    }
    let member = |script: Option<&molecule::Script<'_>>| -> Result<Option<u8>, CheckerError> {
        let Some(script) = script else { return Ok(None) };
        if script.code_hash != current.code_hash {
            return Ok(None);
        }
        require(script.args.len() >= 40 && &script.args[..8] == MAGIC, "malformed same-code family")?;
        if script.args[8..40] != family {
            return Ok(None);
        }
        require(
            script.hash_type == 4 && script.args.len() == 73 && script.args[40] < 4 && script.args[41..] == current.args[41..],
            "same-family args/hash type/parent drift",
        )?;
        Ok(Some(script.args[40]))
    };
    let mut selected = Vec::new();
    for (index, (input, raw_input)) in inputs.iter().zip(raw.inputs.chunks_exact(44)).enumerate() {
        require(input.out_point == raw_input[8..], "resolved input OutPoint mismatch")?;
        let cell = molecule::cell(input.output)?;
        if let Some(lane) = member(cell.type_script.as_ref())? {
            require(
                selected.len() < 4 && selected.last().is_none_or(|(last, _, _)| lane > *last),
                "input set cardinality/order/duplicate",
            )?;
            selected.push((lane, index, cell));
        }
    }
    require(!selected.is_empty(), "set transition requires protected inputs; creation is a separate path")?;
    require(
        selected.iter().any(|(_, _, cell)| cell.type_script.as_ref().unwrap().bytes == selected_script),
        "selected Script missing from complete set",
    )?;
    let mut successors = Vec::new();
    for (index, cell) in raw.cells.iter().enumerate() {
        if let Some(lane) = member(cell.type_script.as_ref())? {
            require(
                successors.len() < 4 && successors.last().is_none_or(|(last, _)| lane > *last),
                "output set cardinality/order/duplicate",
            )?;
            successors.push((lane, index));
        }
    }
    require(selected.len() == successors.len(), "unmatched successor count")?;
    let tx_hash = hash(fields[0]);
    let mut leaves = Vec::new();
    for ((lane, input_index, old), (next_lane, output_index)) in selected.iter().zip(successors) {
        let next = &raw.cells[output_index];
        let script = old.type_script.as_ref().unwrap();
        require(
            *lane == next_lane && next.type_script.as_ref().is_some_and(|s| s.bytes == script.bytes),
            "lane/complete-Script correspondence",
        )?;
        let old_data = inputs[*input_index].data;
        let new_data = raw.data[output_index];
        state(old_data)?;
        state(new_data)?;
        require(
            old_data[8..40] == new_data[8..40] && old.capacity == next.capacity && old.lock.bytes == next.lock.bytes,
            "owner/custody changed",
        )?;
        let old_count = u64::from_le_bytes(old_data[40..].try_into().unwrap());
        let new_count = u64::from_le_bytes(new_data[40..].try_into().unwrap());
        require(old_count.checked_add(1) == Some(new_count), "counter increment/overflow")?;
        let point = inputs[*input_index].out_point;
        leaves.push(CounterSetLeaf {
            input_index: *input_index as u32,
            output_index: output_index as u32,
            statement: Statement {
                domain: hash(b"cellscript-private-counter-v1/domain"),
                action: hash(b"cellscript-private-counter-v1/increment"),
                script_hash: hash(script.bytes),
                old_data_hash: hash(old_data),
                new_data_hash: hash(new_data),
                input_transaction_hash: point[..32].try_into().unwrap(),
                input_output_index: u32::from_le_bytes(point[32..].try_into().unwrap()),
                transaction_hash: tx_hash,
            },
        });
    }
    let encoded = encode_counter_set(family, &leaves)?;
    Ok(CheckedCounterSet { selected_script: selected_script.into(), encoded, leaves, snapshot_hash: hash(&snapshot) })
}
