//! Test-only reader of the two declared Molecule fixture shapes. It uses literal
//! field widths supplied by the fixture, never compiler schemas or codec helpers.
//! Wire rules: https://github.com/nervosnetwork/molecule/blob/master/docs/encoding_spec.md

fn word(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(bytes.get(at..at.checked_add(4)?)?.try_into().ok()?) as usize)
}

fn fields(bytes: &[u8]) -> Option<Vec<&[u8]>> {
    if word(bytes, 0)? != bytes.len() {
        return None;
    }
    if bytes.len() == 4 {
        return Some(vec![]);
    }
    let header = word(bytes, 4)?;
    if header < 8 || header > bytes.len() || header % 4 != 0 {
        return None;
    }
    let count = header / 4 - 1;
    let mut parts = Vec::with_capacity(count);
    for index in 0..count {
        let start = word(bytes, 4 * (index + 1))?;
        let end = if index + 1 == count { bytes.len() } else { word(bytes, 4 * (index + 2))? };
        parts.push(bytes.get(start..end)?);
    }
    Some(parts)
}

/// Decode the fixed fixture schema with independent numeric bounds. Returning
/// owned values lets tests compare every field/payload byte, not just validity.
pub fn decode(bytes: &[u8], widths: &[usize], max_items: usize, max_payload: usize, max_bytes: usize) -> Option<super::OwnedItems> {
    if bytes.len() > max_bytes {
        return None;
    }
    let items = fields(bytes)?;
    if items.len() > max_items {
        return None;
    }
    items
        .into_iter()
        .map(|item| {
            let (fixed, payload) = if widths.is_empty() {
                (vec![], item)
            } else {
                let record = fields(item)?;
                if record.len() != widths.len() + 1 {
                    return None;
                }
                let fixed = record[..widths.len()]
                    .iter()
                    .zip(widths)
                    .map(|(field, width)| (field.len() == *width).then(|| field.to_vec()))
                    .collect::<Option<Vec<_>>>()?;
                (fixed, record[widths.len()])
            };
            let size = word(payload, 0)?;
            let value = payload.get(4..)?;
            if size != value.len() || size > max_payload {
                return None;
            }
            Some((fixed, value.to_vec()))
        })
        .collect()
}
