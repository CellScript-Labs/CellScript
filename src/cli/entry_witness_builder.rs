//! Generated client codecs use the native parameter ABI, including its
//! runtime-owned exclusions. They confer no transaction or Script authority.

use crate::{CompileMetadata, ParamMetadata};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn parameter(param: &ParamMetadata) -> Value {
    let (encoding, width) = if !crate::param_consumes_entry_witness_payload(param, &BTreeSet::new()) {
        ("runtime", 0)
    } else if param.schema_pointer_abi || param.schema_length_abi {
        ("schema", 0)
    } else if let Some(width) = param.fixed_byte_len {
        (if param.ty == "u128" && width == 16 { "u128" } else { "bytes" }, width)
    } else if let Some(width) = crate::entry_witness_scalar_param_width(&param.ty) {
        let encoding = match param.ty.as_str() {
            "()" => "unit",
            "bool" => "bool",
            "u8" | "u16" | "u32" | "u64" => "unsigned",
            "i32" => "signed",
            ty if crate::ir::is_ckb_temporal_scalar_name(ty) => "unsigned",
            ty if crate::entry_witness_type_is_small_aggregate(ty) => "bytes",
            _ => "unsupported",
        };
        (encoding, width)
    } else {
        ("unsupported", 0)
    };
    json!({"name":param.name,"type":param.ty,"encoding":encoding,"width":width})
}

pub(super) fn contracts(metadata: &CompileMetadata, actions: &[&crate::ActionMetadata]) -> Value {
    let entries = actions
        .iter()
        .map(|entry| ("action", entry.name.as_str(), entry.params.as_slice()))
        .chain(metadata.locks.iter().map(|entry| ("lock", entry.name.as_str(), entry.params.as_slice())))
        .map(|(kind, name, params)| json!({"kind":kind,"name":name,"params":params.iter().map(parameter).collect::<Vec<_>>()}))
        .collect::<Vec<_>>();
    json!({"schema":"cellscript-generated-entry-witness-contract-v1","payload_abi":crate::ENTRY_WITNESS_ABI,
        "placement_abi":crate::ENTRY_WITNESS_PLACEMENT_ABI,"placement_field":crate::ENTRY_WITNESS_PLACEMENT_FIELD,
        "magic":crate::ENTRY_WITNESS_ABI_MAGIC,"entries":entries})
}

pub(super) fn test_cases(metadata: &CompileMetadata, actions: &[&crate::ActionMetadata]) -> Vec<Value> {
    actions
        .iter()
        .map(|entry| ("action", entry.name.as_str(), entry.params.as_slice()))
        .chain(metadata.locks.iter().map(|entry| ("lock", entry.name.as_str(), entry.params.as_slice())))
        .map(|(kind, name, params)| {
            let mut inputs = Vec::new();
            let mut native = Vec::new();
            for param in params {
                let contract = parameter(param);
                let encoding = contract["encoding"].as_str().expect("parameter encoding");
                let width = contract["width"].as_u64().expect("parameter width") as usize;
                let (input, arg) = match encoding {
                    "runtime" | "unit" | "unsupported" => continue,
                    "schema" => (json!("0x0102ff"), crate::EntryWitnessArg::Bytes(vec![1, 2, 255])),
                    "bytes" => {
                        (json!(format!("0x{}", hex::encode(vec![0xa5; width]))), crate::EntryWitnessArg::Bytes(vec![0xa5; width]))
                    }
                    "u128" => (json!(u128::MAX.to_string()), crate::EntryWitnessArg::U128(u128::MAX)),
                    "bool" => (json!(true), crate::EntryWitnessArg::Bool(true)),
                    "signed" => (json!(i32::MIN.to_string()), crate::EntryWitnessArg::I32(i32::MIN)),
                    "unsigned" => match param.ty.as_str() {
                        "u8" => (json!(u8::MAX.to_string()), crate::EntryWitnessArg::U8(u8::MAX)),
                        "u16" => (json!(u16::MAX.to_string()), crate::EntryWitnessArg::U16(u16::MAX)),
                        "u32" => (json!(u32::MAX.to_string()), crate::EntryWitnessArg::U32(u32::MAX)),
                        _ => (json!(u64::MAX.to_string()), crate::EntryWitnessArg::U64(u64::MAX)),
                    },
                    _ => unreachable!("closed parameter encoding"),
                };
                inputs.push(input);
                native.push(arg);
            }
            let expected = crate::encode_entry_witness_args_for_params(params, &native).map(hex::encode).ok();
            json!({"kind":kind,"name":name,"args":inputs,"native_hex":expected})
        })
        .collect()
}
