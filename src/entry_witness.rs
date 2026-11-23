use super::error::{CompileError, Result};
use super::{ir, script_handle_contract, zk_contract, ParamMetadata, ENTRY_WITNESS_ABI, ENTRY_WITNESS_ABI_MAGIC};
use std::collections::BTreeSet;

/// Decode a hex-encoded scheduler witness from compile metadata.
pub fn decode_scheduler_witness_hex(hex: &str) -> Result<Vec<u8>> {
    let hex_bytes = hex.as_bytes();
    if !hex_bytes.len().is_multiple_of(2) {
        return Err(CompileError::without_span("scheduler witness hex string must contain full bytes"));
    }
    let mut bytes = Vec::with_capacity(hex_bytes.len() / 2);
    for (pair_index, pair) in hex_bytes.chunks_exact(2).enumerate() {
        let offset = pair_index * 2;
        let high = hex_nibble(pair[0]).ok_or_else(|| {
            CompileError::without_span(format!("invalid scheduler witness hex byte at offset {offset}: invalid digit found in string"))
        })?;
        let low = hex_nibble(pair[1]).ok_or_else(|| {
            CompileError::without_span(format!("invalid scheduler witness hex byte at offset {offset}: invalid digit found in string"))
        })?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

pub(super) fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryWitnessArg {
    Unit,
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    I32(i32),
    U64(u64),
    U128(u128),
    Address([u8; 32]),
    Hash([u8; 32]),
    Bytes(Vec<u8>),
}

/// Encode the inner `bounded-output-plan-v1` payload carried as one
/// `BoundedList<Plan, N>` entry-witness argument. The body is a version marker
/// followed by the canonical Molecule FixVec count and fixed-width elements.
pub fn encode_bounded_output_plan_v1(elements: &[Vec<u8>], element_width: usize, max_elements: usize) -> Result<Vec<u8>> {
    if element_width == 0 || element_width > 512 {
        return Err(CompileError::without_span("bounded output plan element width must be in 1..=512 bytes"));
    }
    if max_elements == 0 || max_elements > 1024 || elements.len() > max_elements {
        return Err(CompileError::without_span(format!(
            "bounded output plan has {} elements but the declared maximum is {}",
            elements.len(),
            max_elements
        )));
    }
    if let Some((index, element)) = elements.iter().enumerate().find(|(_, element)| element.len() != element_width) {
        return Err(CompileError::without_span(format!(
            "bounded output plan element {} has {} bytes; expected exactly {}",
            index,
            element.len(),
            element_width
        )));
    }
    let payload_len = elements
        .len()
        .checked_mul(element_width)
        .and_then(|bytes| bytes.checked_add(12))
        .ok_or_else(|| CompileError::without_span("bounded output plan length exceeds addressable memory"))?;
    if payload_len > ir::BOUNDED_OUTPUT_PLAN_MAX_BYTES {
        return Err(CompileError::without_span(format!(
            "bounded output plan needs {} bytes but the entry ABI permits at most {}",
            payload_len,
            ir::BOUNDED_OUTPUT_PLAN_MAX_BYTES
        )));
    }
    let count = u32::try_from(elements.len())
        .map_err(|_| CompileError::without_span("bounded output plan element count exceeds Molecule FixVec u32"))?;
    let mut payload = Vec::with_capacity(payload_len);
    payload.extend_from_slice(ir::BOUNDED_OUTPUT_PLAN_MAGIC_V1);
    payload.extend_from_slice(&count.to_le_bytes());
    for element in elements {
        payload.extend_from_slice(element);
    }
    Ok(payload)
}

/// Encode positional witness bytes for CellScript's generated entry wrapper.
///
/// The result is suitable for transaction witnesses consumed by `_cellscript_entry`.
/// It mirrors the codegen wrapper ABI: named schema parameters are encoded as
/// `<u32:len><bytes>`, fixed-byte parameters are appended verbatim, and scalar
/// parameters are little-endian encoded.
pub fn encode_entry_witness_args_for_params(params: &[ParamMetadata], args: &[EntryWitnessArg]) -> Result<Vec<u8>> {
    encode_entry_witness_args_for_params_with_runtime_bound(params, args, &BTreeSet::new())
}

/// Encode using the authoritative ABI source fields in `params`.
///
/// `runtime_bound_param_names` is retained for API compatibility; a name alone
/// cannot suppress an ordinary witness value or override its parameter ABI.
pub fn encode_entry_witness_args_for_params_with_runtime_bound(
    params: &[ParamMetadata],
    args: &[EntryWitnessArg],
    runtime_bound_param_names: &BTreeSet<String>,
) -> Result<Vec<u8>> {
    if !params.iter().any(|param| param_consumes_entry_witness_payload(param, runtime_bound_param_names)) {
        if args.is_empty() {
            return Ok(Vec::new());
        }
        return Err(CompileError::without_span(format!(
            "entry witness received {} payload args but this entry ABI consumes none",
            args.len()
        )));
    }
    let payload_len = entry_witness_metadata_payload_len_with_runtime_bound(params, runtime_bound_param_names)?;
    let capacity = ENTRY_WITNESS_ABI_MAGIC.len().checked_add(payload_len).ok_or_else(|| {
        CompileError::without_span(format!("entry witness payload length exceeds addressable memory for {}", ENTRY_WITNESS_ABI))
    })?;
    let mut witness = Vec::with_capacity(capacity);
    witness.extend_from_slice(ENTRY_WITNESS_ABI_MAGIC);

    let mut arg_index = 0usize;
    for param in params {
        if !param_consumes_entry_witness_payload(param, runtime_bound_param_names) {
            continue;
        }

        if param.schema_pointer_abi || param.schema_length_abi {
            let arg = args.get(arg_index).ok_or_else(|| entry_witness_missing_arg_error(param, arg_index))?;
            entry_witness_append_schema_arg(&mut witness, param, arg)?;
            arg_index += 1;
            continue;
        }

        if let Some(width) = param.fixed_byte_len {
            let arg = args.get(arg_index).ok_or_else(|| entry_witness_missing_arg_error(param, arg_index))?;
            witness.extend_from_slice(&entry_witness_fixed_arg_bytes(param, arg, width)?);
            arg_index += 1;
            continue;
        }

        let Some(width) = entry_witness_scalar_param_width(&param.ty) else {
            return Err(CompileError::without_span(format!(
                "entry witness parameter '{}' has unsupported type '{}'",
                param.name, param.ty
            )));
        };
        if width == 0 {
            if matches!(args.get(arg_index), Some(EntryWitnessArg::Unit)) {
                arg_index += 1;
            }
            continue;
        }
        let arg = args.get(arg_index).ok_or_else(|| entry_witness_missing_arg_error(param, arg_index))?;
        entry_witness_append_scalar_arg(&mut witness, param, arg, width)?;
        arg_index += 1;
    }

    if arg_index != args.len() {
        return Err(CompileError::without_span(format!(
            "entry witness received {} payload args but consumed {} for {}",
            args.len(),
            arg_index,
            ENTRY_WITNESS_ABI
        )));
    }

    Ok(witness)
}

fn entry_witness_metadata_payload_len_with_runtime_bound(
    params: &[ParamMetadata],
    runtime_bound_param_names: &BTreeSet<String>,
) -> Result<usize> {
    params.iter().try_fold(0usize, |acc, param| {
        if !param_consumes_entry_witness_payload(param, runtime_bound_param_names) {
            Ok(acc)
        } else if param.schema_pointer_abi || param.schema_length_abi {
            entry_witness_checked_payload_len_add(acc, 4, param)
        } else if let Some(width) = param.fixed_byte_len {
            entry_witness_checked_payload_len_add(acc, width, param)
        } else if let Some(width) = entry_witness_scalar_param_width(&param.ty) {
            entry_witness_checked_payload_len_add(acc, width, param)
        } else {
            Err(CompileError::without_span(format!("entry witness parameter '{}' has unsupported type '{}'", param.name, param.ty)))
        }
    })
}

fn entry_witness_checked_payload_len_add(acc: usize, width: usize, param: &ParamMetadata) -> Result<usize> {
    acc.checked_add(width).ok_or_else(|| {
        CompileError::without_span(format!(
            "entry witness parameter '{}' makes payload length exceed addressable memory for {}",
            param.name, ENTRY_WITNESS_ABI
        ))
    })
}

fn entry_witness_append_schema_arg(witness: &mut Vec<u8>, param: &ParamMetadata, arg: &EntryWitnessArg) -> Result<()> {
    let bytes = match arg {
        EntryWitnessArg::Bytes(bytes) => bytes,
        _ => {
            return Err(CompileError::without_span(format!(
                "entry witness parameter '{}' expects schema bytes for type '{}'",
                param.name, param.ty
            )));
        }
    };
    let len = u32::try_from(bytes.len()).map_err(|_| {
        CompileError::without_span(format!(
            "entry witness parameter '{}' exceeds the 4-byte schema payload limit for {}",
            param.name, ENTRY_WITNESS_ABI
        ))
    })?;
    witness.extend_from_slice(&len.to_le_bytes());
    witness.extend_from_slice(bytes);
    Ok(())
}

fn entry_witness_fixed_arg_bytes(param: &ParamMetadata, arg: &EntryWitnessArg, width: usize) -> Result<Vec<u8>> {
    let bytes = match (param.ty.as_str(), arg) {
        ("u128", EntryWitnessArg::U128(value)) if width == 16 => value.to_le_bytes().to_vec(),
        ("Address", EntryWitnessArg::Address(bytes)) if width == 32 => bytes.to_vec(),
        ("Hash", EntryWitnessArg::Hash(bytes)) if width == 32 => bytes.to_vec(),
        (ty, EntryWitnessArg::Hash(bytes)) if width == 32 && ir::is_ckb_fixed_hash_domain_name(ty) => bytes.to_vec(),
        (_, EntryWitnessArg::Bytes(bytes)) if bytes.len() == width => bytes.clone(),
        _ => {
            return Err(CompileError::without_span(format!(
                "entry witness parameter '{}' expects {} fixed bytes for type '{}'",
                param.name, width, param.ty
            )));
        }
    };
    Ok(bytes)
}

fn entry_witness_append_scalar_arg(witness: &mut Vec<u8>, param: &ParamMetadata, arg: &EntryWitnessArg, width: usize) -> Result<()> {
    match (param.ty.as_str(), arg) {
        ("bool", EntryWitnessArg::Bool(value)) if width == 1 => witness.push(u8::from(*value)),
        ("u8", EntryWitnessArg::U8(value)) if width == 1 => witness.push(*value),
        ("u16", EntryWitnessArg::U16(value)) if width == 2 => witness.extend_from_slice(&value.to_le_bytes()),
        ("u32", EntryWitnessArg::U32(value)) if width == 4 => witness.extend_from_slice(&value.to_le_bytes()),
        ("i32", EntryWitnessArg::I32(value)) if width == 4 => witness.extend_from_slice(&value.to_le_bytes()),
        ("u64", EntryWitnessArg::U64(value)) if width == 8 => witness.extend_from_slice(&value.to_le_bytes()),
        (_, EntryWitnessArg::U64(value)) if width == 8 && ir::is_ckb_temporal_scalar_name(&param.ty) => {
            witness.extend_from_slice(&value.to_le_bytes());
        }
        (_, EntryWitnessArg::Bytes(bytes)) if bytes.len() == width && entry_witness_type_is_small_aggregate(&param.ty) => {
            witness.extend_from_slice(bytes);
        }
        _ => {
            return Err(CompileError::without_span(format!(
                "entry witness parameter '{}' expects scalar payload for type '{}'",
                param.name, param.ty
            )));
        }
    }
    Ok(())
}

fn entry_witness_missing_arg_error(param: &ParamMetadata, index: usize) -> CompileError {
    CompileError::without_span(format!(
        "entry witness missing payload arg {} for parameter '{}' of type '{}'",
        index, param.name, param.ty
    ))
}

pub(super) fn runtime_bound_param_names(params: &[ParamMetadata]) -> BTreeSet<String> {
    params
        .iter()
        .filter(|param| !param_consumes_entry_witness_payload(param, &BTreeSet::new()))
        .map(|param| param.name.clone())
        .collect()
}

pub(super) fn param_consumes_entry_witness_payload(param: &ParamMetadata, _runtime_bound_param_names: &BTreeSet<String>) -> bool {
    // The legacy names argument cannot override the parameter's ABI source.
    // Generated temporaries may share an ordinary witness parameter's name.
    !param.lock_args_data_source
        && !param.cell_bound_abi
        && !param.is_ref
        && !param.ty.starts_with('&')
        && param.bounded_runtime_contract.as_deref() != Some(ir::BOUNDED_TYPE_GROUP_INPUTS_V1)
}

pub(super) fn entry_witness_scalar_param_width(ty: &str) -> Option<usize> {
    match ty.trim() {
        "()" => Some(0),
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" => Some(4),
        "i32" => Some(4),
        "u64" => Some(8),
        other => entry_witness_static_type_len(other).filter(|width| (1..=8).contains(width)),
    }
}

pub(super) fn entry_witness_type_is_small_aggregate(ty: &str) -> bool {
    let ty = ty.trim();
    (ty.starts_with('[') || ty.starts_with('(')) && entry_witness_static_type_len(ty).is_some_and(|width| width <= 8)
}

pub(crate) fn entry_witness_static_type_len(ty: &str) -> Option<usize> {
    let ty = ty.trim();
    match ty {
        "()" => return Some(0),
        "bool" | "u8" => return Some(1),
        "u16" => return Some(2),
        "u32" => return Some(4),
        "i32" => return Some(4),
        "u64" => return Some(8),
        "u128" => return Some(16),
        "Address" | "Hash" => return Some(32),
        other if ir::is_ckb_fixed_hash_domain_name(other) => return Some(32),
        other if zk_contract::fixed_width(other).is_some() => return zk_contract::fixed_width(other),
        other if other == script_handle_contract::EXACT_SCRIPT_HANDLE_TYPE => {
            return Some(script_handle_contract::EXACT_SCRIPT_HANDLE_BYTES)
        }
        other if other.starts_with("InstanceScriptHandle<") => return Some(script_handle_contract::INSTANCE_SCRIPT_HANDLE_BYTES),
        // The compatible-open handle runtime value is the frozen 656-byte
        // selection witness; its width never depends on the designation.
        other
            if matches!(
                other.split('<').next().unwrap_or(other),
                script_handle_contract::OPEN_SCRIPT_HANDLE_TYPE | script_handle_contract::OPEN_VERIFIER_HANDLE_TYPE
            ) =>
        {
            return Some(656)
        }
        other if other == script_handle_contract::DEPLOYMENT_LINE_HANDLE_TYPE => {
            return Some(script_handle_contract::DEPLOYMENT_LINE_HANDLE_BYTES)
        }
        other if ir::is_ckb_temporal_scalar_name(other) => return Some(8),
        _ => {}
    }

    if let Some(inner) = ty.strip_prefix('&') {
        return entry_witness_static_type_len(inner.trim_start_matches("mut ").trim());
    }

    if let Some(body) = ty.strip_prefix('[').and_then(|value| value.strip_suffix(']')) {
        let (inner, len) = split_top_level_once(body, ';')?;
        let len = len.trim().parse::<usize>().ok()?;
        return entry_witness_static_type_len(inner).and_then(|inner_len| inner_len.checked_mul(len));
    }

    if let Some(body) = ty.strip_prefix('(').and_then(|value| value.strip_suffix(')')) {
        if body.trim().is_empty() {
            return Some(0);
        }
        return split_top_level_commas(body)
            .iter()
            .try_fold(0usize, |acc, item| entry_witness_static_type_len(item).and_then(|len| acc.checked_add(len)));
    }

    None
}

fn split_top_level_once(input: &str, separator: char) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            _ if ch == separator && depth == 0 => return Some((&input[..index], &input[index + ch.len_utf8()..])),
            _ => {}
        }
    }
    None
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                items.push(input[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    items.push(input[start..].trim());
    items
}
