//! Strict, bounded views of the pinned CKB RawTransaction schema.
//! This validates byte structure, never live-cell/consensus authorization.
use super::invalid;
use crate::CheckerError;
use std::collections::BTreeSet;

pub(super) const MAX_CELLS: usize = 256;
const MAX_DEPS: usize = 64;

fn u32_at(bytes: &[u8], offset: usize) -> Result<usize, CheckerError> {
    let end = offset.checked_add(4).ok_or_else(|| invalid("Molecule offset overflow"))?;
    let word: [u8; 4] = bytes
        .get(offset..end)
        .ok_or_else(|| invalid("truncated Molecule integer"))?
        .try_into()
        .map_err(|_| invalid("Molecule integer width"))?;
    Ok(u32::from_le_bytes(word) as usize)
}
pub(super) fn fields(bytes: &[u8], count: usize) -> Result<Vec<&[u8]>, CheckerError> {
    let header = count.checked_add(1).and_then(|n| n.checked_mul(4)).ok_or_else(|| invalid("Molecule header overflow"))?;
    if bytes.len() < header || u32_at(bytes, 0)? != bytes.len() || u32_at(bytes, 4)? != header {
        return Err(invalid("Molecule table/dynvec total or first offset differs"));
    }
    let mut result = Vec::with_capacity(count);
    let mut previous = header;
    for index in 0..count {
        let start = u32_at(bytes, 4 + index * 4)?;
        let end = if index + 1 == count { bytes.len() } else { u32_at(bytes, 8 + index * 4)? };
        if start != previous || end < start || end > bytes.len() {
            return Err(invalid("Molecule offsets are noncanonical or escape"));
        }
        result.push(&bytes[start..end]);
        previous = end;
    }
    Ok(result)
}
pub(super) fn dynamic(bytes: &[u8], maximum: usize) -> Result<Vec<&[u8]>, CheckerError> {
    if bytes.len() == 4 && u32_at(bytes, 0)? == 4 {
        return Ok(Vec::new());
    }
    let first = u32_at(bytes, 4)?;
    if first < 8 || first % 4 != 0 {
        return Err(invalid("invalid Molecule dynvec header"));
    }
    let count = first / 4 - 1;
    if count > maximum {
        return Err(invalid("Molecule dynvec count exceeds finite profile"));
    }
    fields(bytes, count)
}
pub(super) fn fixed(bytes: &[u8], width: usize, maximum: usize) -> Result<&[u8], CheckerError> {
    let count = u32_at(bytes, 0)?;
    if count > maximum || count.checked_mul(width).and_then(|n| n.checked_add(4)) != Some(bytes.len()) {
        return Err(invalid("Molecule fixvec count/width differs or exceeds profile"));
    }
    Ok(&bytes[4..])
}
pub(super) struct Script<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) code_hash: &'a [u8],
    pub(super) hash_type: u8,
    pub(super) args: &'a [u8],
}
pub(super) fn script(bytes: &[u8]) -> Result<Script<'_>, CheckerError> {
    if bytes.len() > 4096 {
        return Err(invalid("Script exceeds 4096-byte finite profile"));
    }
    let values = fields(bytes, 3)?;
    if values[0].len() != 32 || values[1].len() != 1 || !matches!(values[1][0], 0 | 1 | 2 | 4) {
        return Err(invalid("Script code hash/hash type is invalid or unsupported"));
    }
    Ok(Script { bytes, code_hash: values[0], hash_type: values[1][0], args: fixed(values[2], 1, 4096)? })
}
pub(super) struct Cell<'a> {
    pub(super) capacity: u64,
    pub(super) lock: Script<'a>,
    pub(super) type_script: Option<Script<'a>>,
}
pub(super) fn cell(bytes: &[u8]) -> Result<Cell<'_>, CheckerError> {
    let values = fields(bytes, 3)?;
    let capacity = u64::from_le_bytes(values[0].try_into().map_err(|_| invalid("CellOutput capacity width differs"))?);
    let lock = script(values[1])?;
    let type_script = if values[2].is_empty() { None } else { Some(script(values[2])?) };
    Ok(Cell { capacity, lock, type_script })
}
pub(super) struct Transaction<'a> {
    pub(super) cells: Vec<Cell<'a>>,
    pub(super) data: Vec<&'a [u8]>,
    pub(super) inputs: &'a [u8],
    pub(super) deps: &'a [u8],
}
pub(super) fn transaction(bytes: &[u8]) -> Result<Transaction<'_>, CheckerError> {
    let values = fields(bytes, 6)?;
    if values[0] != 0u32.to_le_bytes() {
        return Err(invalid("only transaction version zero belongs to this profile"));
    }
    let deps = fixed(values[1], 37, MAX_DEPS)?;
    let mut outpoints = BTreeSet::new();
    for dep in deps.chunks_exact(37) {
        if !matches!(dep[36], 0 | 1) || !outpoints.insert(&dep[..36]) {
            return Err(invalid("duplicate or unsupported raw CellDep"));
        }
    }
    let headers = fixed(values[2], 32, MAX_DEPS)?;
    let mut hashes = BTreeSet::new();
    if headers.chunks_exact(32).any(|hash| !hashes.insert(hash)) {
        return Err(invalid("duplicate header dependency"));
    }
    let inputs = fixed(values[3], 44, MAX_CELLS)?;
    let mut previous = BTreeSet::new();
    if inputs.chunks_exact(44).any(|input| !previous.insert(&input[8..])) {
        return Err(invalid("duplicate input OutPoint"));
    }
    let cells = dynamic(values[4], MAX_CELLS)?.into_iter().map(cell).collect::<Result<Vec<_>, _>>()?;
    let data =
        dynamic(values[5], MAX_CELLS)?.into_iter().map(|bytes| fixed(bytes, 1, 4 * 1024 * 1024)).collect::<Result<Vec<_>, _>>()?;
    if cells.len() != data.len() {
        return Err(invalid("output and output-data cardinalities differ"));
    }
    Ok(Transaction { cells, data, inputs, deps })
}
