use crate::error::Result;
use crate::{CompileMetadata, EntryWitnessArg, ParamMetadata};

pub(super) struct SelectedEntryWitnessMetadata<'a> {
    pub(super) kind: &'static str,
    pub(super) name: &'a str,
    pub(super) params: &'a [ParamMetadata],
    pub(super) runtime_bound_param_names: std::collections::BTreeSet<String>,
}

pub(super) fn select_entry_witness_metadata<'a>(
    metadata: &'a CompileMetadata,
    action: Option<&str>,
    lock: Option<&str>,
) -> Result<SelectedEntryWitnessMetadata<'a>> {
    if let Some(name) = action {
        let action = metadata
            .actions
            .iter()
            .find(|candidate| candidate.name == name)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("action '{}' was not found in metadata", name)))?;
        return Ok(SelectedEntryWitnessMetadata {
            kind: "action",
            name: action.name.as_str(),
            params: &action.params,
            runtime_bound_param_names: crate::runtime_bound_param_names(&action.params),
        });
    }
    if let Some(name) = lock {
        let lock = metadata
            .locks
            .iter()
            .find(|candidate| candidate.name == name)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("lock '{}' was not found in metadata", name)))?;
        return Ok(SelectedEntryWitnessMetadata {
            kind: "lock",
            name: lock.name.as_str(),
            params: &lock.params,
            runtime_bound_param_names: crate::runtime_bound_param_names(&lock.params),
        });
    }

    let mut entries = metadata
        .actions
        .iter()
        .filter(|action| !action.params.is_empty())
        .map(|action| SelectedEntryWitnessMetadata {
            kind: "action",
            name: action.name.as_str(),
            params: action.params.as_slice(),
            runtime_bound_param_names: crate::runtime_bound_param_names(&action.params),
        })
        .chain(metadata.locks.iter().filter(|lock| !lock.params.is_empty()).map(|lock| SelectedEntryWitnessMetadata {
            kind: "lock",
            name: lock.name.as_str(),
            params: lock.params.as_slice(),
            runtime_bound_param_names: crate::runtime_bound_param_names(&lock.params),
        }))
        .collect::<Vec<_>>();

    match entries.len() {
        1 => Ok(entries.remove(0)),
        0 => Err(crate::error::CompileError::without_span(
            "no parameterized action or lock found; specify --action or --lock for explicit selection",
        )),
        _ => Err(crate::error::CompileError::without_span(
            "multiple parameterized actions/locks found; specify --action NAME or --lock NAME",
        )),
    }
}

pub(super) fn parse_entry_witness_arg(param: &ParamMetadata, value: &str) -> Result<EntryWitnessArg> {
    if param.schema_pointer_abi || param.schema_length_abi {
        return decode_hex_arg(&param.name, value, None).map(EntryWitnessArg::Bytes);
    }

    if let Some(width) = param.fixed_byte_len {
        return parse_entry_witness_fixed_arg(param, value, width);
    }

    match param.ty.as_str() {
        "bool" => parse_bool_arg(&param.name, value).map(EntryWitnessArg::Bool),
        "u8" => parse_integer_arg(&param.name, value, u8::MAX as u128).map(|value| EntryWitnessArg::U8(value as u8)),
        "u16" => parse_integer_arg(&param.name, value, u16::MAX as u128).map(|value| EntryWitnessArg::U16(value as u16)),
        "u32" => parse_integer_arg(&param.name, value, u32::MAX as u128).map(|value| EntryWitnessArg::U32(value as u32)),
        "u64" => parse_integer_arg(&param.name, value, u64::MAX as u128).map(|value| EntryWitnessArg::U64(value as u64)),
        temporal if crate::ir::is_ckb_temporal_scalar_name(temporal) => {
            parse_integer_arg(&param.name, value, u64::MAX as u128).map(|value| EntryWitnessArg::U64(value as u64))
        }
        "()" => Ok(EntryWitnessArg::Unit),
        other => {
            let Some(width) = crate::entry_witness_static_type_len(other).filter(|width| (1..=8).contains(width)) else {
                return Err(crate::error::CompileError::without_span(format!(
                    "parameter '{}' has unsupported entry witness CLI type '{}'",
                    param.name, param.ty
                )));
            };
            decode_hex_arg(&param.name, value, Some(width)).map(EntryWitnessArg::Bytes)
        }
    }
}

fn parse_entry_witness_fixed_arg(param: &ParamMetadata, value: &str, width: usize) -> Result<EntryWitnessArg> {
    match param.ty.as_str() {
        "u128" if width == 16 => parse_integer_arg(&param.name, value, u128::MAX).map(EntryWitnessArg::U128),
        "Address" if width == 32 => {
            let bytes = decode_hex_arg(&param.name, value, Some(32))?;
            let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
                crate::error::CompileError::without_span(format!("parameter '{}' expects exactly 32 hex bytes", param.name))
            })?;
            Ok(EntryWitnessArg::Address(bytes))
        }
        "Hash" if width == 32 => {
            let bytes = decode_hex_arg(&param.name, value, Some(32))?;
            let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
                crate::error::CompileError::without_span(format!("parameter '{}' expects exactly 32 hex bytes", param.name))
            })?;
            Ok(EntryWitnessArg::Hash(bytes))
        }
        _ => decode_hex_arg(&param.name, value, Some(width)).map(EntryWitnessArg::Bytes),
    }
}

fn parse_bool_arg(name: &str, value: &str) -> Result<bool> {
    match value.trim() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        other => Err(crate::error::CompileError::without_span(format!(
            "parameter '{}' expects bool value true/false/1/0, got '{}'",
            name, other
        ))),
    }
}

fn parse_integer_arg(name: &str, value: &str, max: u128) -> Result<u128> {
    let trimmed = value.trim();
    let parsed = if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        u128::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<u128>()
    }
    .map_err(|error| crate::error::CompileError::without_span(format!("parameter '{}' expects integer: {}", name, error)))?;
    if parsed > max {
        return Err(crate::error::CompileError::without_span(format!(
            "parameter '{}' integer value {} is out of range",
            name, parsed
        )));
    }
    Ok(parsed)
}

pub(super) fn decode_hex_arg(name: &str, value: &str, expected_len: Option<usize>) -> Result<Vec<u8>> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("hex:")
        .or_else(|| trimmed.strip_prefix("HEX:"))
        .or_else(|| trimmed.strip_prefix("0x"))
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if !hex.len().is_multiple_of(2) {
        return Err(crate::error::CompileError::without_span(format!("parameter '{}' hex value must contain full bytes", name)));
    }
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .enumerate()
        .map(|(pair_index, pair)| {
            let offset = pair_index * 2;
            let high = hex_nibble(pair[0]).ok_or_else(|| invalid_hex_arg_error(name, offset))?;
            let low = hex_nibble(pair[1]).ok_or_else(|| invalid_hex_arg_error(name, offset))?;
            Ok((high << 4) | low)
        })
        .collect::<Result<Vec<_>>>()?;
    if let Some(expected_len) = expected_len
        && bytes.len() != expected_len
    {
        return Err(crate::error::CompileError::without_span(format!(
            "parameter '{}' expects {} byte(s), got {}",
            name,
            expected_len,
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn invalid_hex_arg_error(name: &str, offset: usize) -> crate::error::CompileError {
    crate::error::CompileError::without_span(format!(
        "parameter '{}' has invalid hex byte at offset {}: invalid digit found in string",
        name, offset
    ))
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
