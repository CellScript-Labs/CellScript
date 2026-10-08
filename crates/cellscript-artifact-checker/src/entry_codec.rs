//! Independent fixed policy-argument decoding, distinct from full data codecs.
//! This bounded prerequisite checks the actual positional machine decoder. It
//! grants no package/interface, Cell-data, deployment or catalog admission.
use crate::checker::{
    entry_start, flow_targets, is_addi, is_auipc, is_beq, is_bne, is_jal_zero, is_jalr_call, is_lbu, is_ld, is_or, is_sd, is_slli,
    is_sltu, is_sub, unique_policy_block,
};
use crate::interface::{CheckedModuleProjection, SourceType};
use crate::{
    CheckerBudgets, CheckerError, CheckerRejectionCode, EntryDispatchContract, ParsedElf, TypedSemanticEntry, TypedSemanticRecord,
    VerifiedLoweringRecord,
};
use serde::Serialize;
use std::collections::BTreeMap;

const SCHEMA: &str = "cellscript-fixed-policy-parameter-decoder-v1";
const MAX_PAYLOAD: u32 = 1536;

#[derive(Debug)]
pub struct CheckedFixedPolicyParameterDecoders {
    module: CheckedModuleProjection,
    record: Record,
    identity: String,
}
impl CheckedFixedPolicyParameterDecoders {
    /// Check bytes against this privately verified positional decoder. The
    /// finite external profile separately restricts transported witness types.
    pub(crate) fn check_payload(&self, tag: u32, bytes: &[u8]) -> Result<(), CheckerError> {
        let variant = self.record.variants.get(&tag).ok_or_else(|| invalid("unknown checked payload tag"))?;
        let width = variant.parameters.iter().filter_map(|p| p.payload_offset.map(|offset| offset + p.width_bytes)).max().unwrap_or(0);
        if width == 0 {
            if !bytes.is_empty() {
                return Err(invalid("payload-free action requires empty args"));
            }
        } else if bytes.len() != width as usize + 8 || !bytes.starts_with(b"CSARGv1\0") {
            return Err(invalid(
                "policy args differ from checked fixed magic/length; encode the selected action's exact scalar payload",
            ));
        }
        Ok(())
    }
    pub fn module_projection(&self) -> &CheckedModuleProjection {
        &self.module
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CheckerError> {
        crate::canonical_bytes(&self.record)
    }
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    module_contract: String,
    variants: BTreeMap<u32, Variant>,
}
#[derive(Debug, Serialize)]
struct Variant {
    action: String,
    parameters: Vec<Parameter>,
}
#[derive(Debug, Serialize)]
pub(crate) struct Parameter {
    pub(crate) name: String,
    ty: String,
    source: String,
    transport: Transport,
    width_bytes: u32,
    pub(crate) payload_offset: Option<u32>,
    pub(crate) abi_index: u32,
    pub(crate) abi_arguments: u32,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Transport {
    RuntimeBoundNull,
    FixedBytePointer,
    LittleEndianScalar,
}
fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("fixed policy parameter decoder: {}", message.into()),
    )
}

/// Read actual bundles through the independent checker first. This initial
/// machine profile accepts <=1536 fixed argument bytes and <=8 ABI registers;
/// bool, enums, named/dynamic witness values, Script.args and outgoing stack
/// arguments reject. Runtime-bound null pairs are checked only as transport,
/// never as proof of their eventual transaction Cell-data decoding.
pub fn check_fixed_policy_parameter_decoders(
    artifact: &[u8],
    metadata: &[u8],
    lowering: &[u8],
    source_map: &[u8],
    budgets: &CheckerBudgets,
) -> Result<CheckedFixedPolicyParameterDecoders, CheckerError> {
    let module = crate::interface::project_bundle(artifact, metadata, lowering, source_map, budgets)?;
    let record: VerifiedLoweringRecord = serde_json::from_slice(lowering).map_err(|error| invalid(error.to_string()))?;
    let elf = crate::parse_elf(artifact, budgets.instructions).map_err(|error| invalid(error.to_string()))?;
    let variants =
        if let Some((logical, decoded)) = crate::policy_machine::normalize_relaxed_branches(&record, &elf).map_err(invalid)? {
            verify(&logical, &decoded)?
        } else {
            verify(&record, &elf)?
        };
    let record = Record { schema: SCHEMA, module_contract: module.identity().into(), variants };
    let identity = crate::canonical_hash("cellscript-fixed-policy-parameter-decoder-id-v1", &record)?;
    Ok(CheckedFixedPolicyParameterDecoders { module, record, identity })
}

fn width(ty: &SourceType) -> Result<u32, CheckerError> {
    let result = match ty {
        SourceType::Named(name, args) if args.is_empty() => match name.as_str() {
            "unit" => 0,
            "u8" => 1,
            "u16" => 2,
            "u32" | "i32" => 4,
            "u64" => 8,
            "u128" => 16,
            "Address" | "address" | "Hash" | "hash" => 32,
            _ => return Err(invalid("witness type lacks this fixed canonical codec profile")),
        },
        SourceType::Array(inner, count) => width(inner)?
            .checked_mul(u32::try_from(*count).map_err(|_| invalid("array count exceeds fixed profile"))?)
            .ok_or_else(|| invalid("array width overflow"))?,
        SourceType::Tuple(fields) => fields
            .iter()
            .try_fold(0u32, |total, field| total.checked_add(width(field)?).ok_or_else(|| invalid("tuple width overflow")))?,
        _ => return Err(invalid("witness shape lacks this fixed canonical codec profile")),
    };
    if result > MAX_PAYLOAD {
        return Err(invalid("fixed payload exceeds 1536 bytes"));
    }
    Ok(result)
}
pub(crate) fn parameters(entry: &TypedSemanticEntry, typed: &TypedSemanticRecord) -> Result<Vec<Parameter>, CheckerError> {
    let mut result = Vec::new();
    let mut cursor = 0u32;
    let mut abi = 0u32;
    for param in &entry.params {
        if param.source == "lockargs" {
            return Err(invalid("Script.args decoding requires a separate machine profile"));
        }
        let projection = crate::policy::builder_parameter_projection(param, entry, typed)?;
        let runtime = projection["cell_bound_abi"] == true || param.reference;
        let size =
            if runtime { 0 } else { width(&crate::interface::parse_source_type(&crate::checker::canonical_abi_type(&param.ty))?)? };
        let arguments = if runtime {
            2 + if projection["type_hash_pointer_abi"] == true { 2 } else { 0 }
        } else if size > 8 {
            2
        } else {
            1
        };
        result.push(Parameter {
            name: param.name.clone(),
            ty: param.ty.clone(),
            source: param.source.clone(),
            transport: if runtime {
                Transport::RuntimeBoundNull
            } else if size > 8 {
                Transport::FixedBytePointer
            } else {
                Transport::LittleEndianScalar
            },
            width_bytes: size,
            payload_offset: (!runtime).then_some(cursor),
            abi_index: abi,
            abi_arguments: arguments,
        });
        cursor = cursor.checked_add(size).ok_or_else(|| invalid("payload width overflow"))?;
        abi += arguments;
        if cursor > MAX_PAYLOAD || abi > 8 {
            return Err(invalid("fixed decoder exceeds payload/register profile"));
        }
    }
    Ok(result)
}

struct Cursor<'a> {
    elf: &'a ParsedElf,
    address: u64,
}
impl Cursor<'_> {
    fn expect(&mut self, predicate: impl FnOnce(u32) -> bool) -> Result<u64, CheckerError> {
        let address = self.address;
        // Real and relaxation-normalized text retain ascending addresses.
        let index = self
            .elf
            .instructions
            .binary_search_by_key(&address, |instruction| instruction.address)
            .map_err(|_| invalid(format!("decoder instruction is absent at {address:#x}")))?;
        let instruction = &self.elf.instructions[index];
        if !predicate(instruction.word) {
            return Err(invalid(format!("unexpected decoder instruction at {address:#x}")));
        }
        self.address += 4;
        Ok(address)
    }
    fn branch(&mut self, predicate: impl FnOnce(u32) -> bool, target: u64) -> Result<(), CheckerError> {
        let address = self.expect(predicate)?;
        if !flow_targets(self.elf, address, target) {
            return Err(invalid("decoder branch changed its bound target"));
        }
        Ok(())
    }
}
fn verify(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<BTreeMap<u32, Variant>, CheckerError> {
    let EntryDispatchContract::PolicyWitnessV1(policy) = &record.typed_semantics.foundation.entry_contract.dispatch else {
        return Err(invalid("only checked policy-witness-v1 adapters have this machine profile"));
    };
    let mut adapters = record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_action_adapter_")).collect::<Vec<_>>();
    adapters.sort_by_key(|entry| entry_start(record, &entry.id).unwrap_or(u64::MAX));
    if adapters.len() != policy.variants.len() {
        return Err(invalid("adapter cardinality differs"));
    }
    let mut result = BTreeMap::new();
    for (adapter, variant) in adapters.into_iter().zip(&policy.variants) {
        let entry = record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.id == variant.entry_id)
            .ok_or_else(|| invalid("variant has no typed entry"))?;
        let params = parameters(entry, &record.typed_semantics)?;
        let payload = params.iter().map(|param| param.width_bytes).sum::<u32>();
        let capacity = if payload == 0 { 0 } else { 8 + payload };
        let (frame, actual_capacity) = crate::policy::private_adapter_layout(entry, &record.typed_semantics)?;
        if capacity != actual_capacity || frame > 2047 || adapter.outgoing_argument_bytes != 0 {
            return Err(invalid("adapter has a noncompact or outgoing-stack layout"));
        }
        let start = entry_start(record, &adapter.id)?;
        let after_save = start + 8;
        let mut cursor = Cursor { elf, address: start };
        cursor.expect(|word| is_addi(word, 2, 2, -(frame as i32)))?;
        cursor.expect(|word| is_sd(word, 1, 2, frame as i32 - 8))?;
        let shared = elf.control_flow.iter().find(|flow| flow.address == after_save + 4).and_then(|flow| {
            record.entries.iter().find(|entry| {
                entry.name.starts_with(".Lpolicy_shared_decoder_") && entry_start(record, &entry.id).ok() == Some(flow.target)
            })
        });
        let owner = shared.unwrap_or(adapter);
        if let Some(shared) = shared {
            cursor.expect(|word| is_auipc(word, 1))?;
            cursor.expect(is_jalr_call)?;
            cursor.address = entry_start(record, &shared.id)?;
        }
        // No external branch/call may enter after the decoder's prologue.
        let owner_start = entry_start(record, &owner.id)?;
        let blocks = record.blocks.iter().filter(|block| block.owner_entry == owner.id).collect::<Vec<_>>();
        for flow in &elf.control_flow {
            if blocks.iter().any(|block| block.range.contains(flow.target))
                && !blocks.iter().any(|block| block.range.contains(flow.address))
                && flow.target != owner_start
            {
                return Err(invalid("an external control-flow edge bypasses decoder entry"));
            }
        }
        let fail = unique_policy_block(record, &owner.id, ".Lentry_witness_fail_")?.range.start;
        if capacity == 0 {
            cursor.branch(|word| is_bne(word, 11, 0), fail)?;
            cursor.expect(|word| is_sd(word, 0, 2, 0))?;
        } else {
            let copy = unique_policy_block(record, &owner.id, ".Lpolicy_args_copy_")?;
            if copy.range.start != cursor.address + 24 {
                return Err(invalid("unchecked operations precede the selected-argument copy"));
            }
            // Envelope/bounded private copy is already verified by check_bundle.
            let copied = unique_policy_block(record, &owner.id, ".Lpolicy_args_copied_")?.range.start;
            let size_ok = unique_policy_block(record, &owner.id, ".Lentry_witness_size_ok_")?.range.start;
            let args_ok = unique_policy_block(record, &owner.id, ".Lentry_witness_exact_size_ok_")?.range.start;
            let copy_branch = copy.range.start;
            for flow in &elf.control_flow {
                if (copied..args_ok).contains(&flow.target)
                    && !(copied..args_ok).contains(&flow.address)
                    && !(flow.address == copy_branch && flow.target == copied)
                {
                    return Err(invalid("an incoming edge bypasses a payload guard"));
                }
            }
            cursor.address = copied;
            cursor.expect(|word| is_ld(word, 5, 2, 0))?;
            cursor.expect(|word| is_addi(word, 6, 0, capacity as i32))?;
            cursor.expect(|word| is_sltu(word, 7, 5, 6))?;
            cursor.branch(|word| is_beq(word, 7, 0), size_ok)?;
            cursor.branch(is_jal_zero, fail)?;
            if cursor.address != size_ok {
                return Err(invalid("payload size guard has an unchecked gap"));
            }
            for (index, byte) in b"CSARGv1\0".iter().enumerate() {
                cursor.expect(|word| is_lbu(word, 5, 2, 8 + index as i32))?;
                cursor.expect(|word| is_addi(word, 6, 0, i32::from(*byte)))?;
                cursor.expect(|word| is_sub(word, 7, 5, 6))?;
                cursor.branch(|word| is_bne(word, 7, 0), fail)?;
            }
            cursor.expect(|word| is_ld(word, 5, 2, 0))?;
            cursor.expect(|word| is_addi(word, 6, 0, capacity as i32))?;
            cursor.expect(|word| is_sub(word, 7, 5, 6))?;
            cursor.branch(|word| is_beq(word, 7, 0), args_ok)?;
            cursor.branch(is_jal_zero, fail)?;
            if cursor.address != args_ok {
                return Err(invalid("exact payload guard has an unchecked gap"));
            }
        }
        let params_start = cursor.address;
        for param in &params {
            let dest = 10 + param.abi_index;
            if param.transport == Transport::RuntimeBoundNull {
                for index in 0..param.abi_arguments {
                    cursor.expect(|word| is_addi(word, dest + index, 0, 0))?;
                }
            } else {
                let offset = 16 + param.payload_offset.expect("constructed witness parameter");
                if param.width_bytes > 8 {
                    cursor.expect(|word| is_addi(word, dest, 2, offset as i32))?;
                    cursor.expect(|word| is_addi(word, dest + 1, 0, param.width_bytes as i32))?;
                } else {
                    cursor.expect(|word| is_addi(word, dest, 0, 0))?;
                    for index in 0..param.width_bytes {
                        cursor.expect(|word| is_lbu(word, 5, 2, (offset + index) as i32))?;
                        if index != 0 {
                            cursor.expect(|word| is_slli(word, 5, 5, index * 8))?;
                        }
                        cursor.expect(|word| is_or(word, dest, dest, 5))?;
                    }
                    if crate::checker::canonical_abi_type(&param.ty) == "i32" {
                        cursor.expect(|word| is_slli(word, dest, dest, 32))?;
                        cursor.expect(|word| {
                            word & 0x707f == 0x5013 && (word >> 7) & 31 == dest && (word >> 15) & 31 == dest && word >> 20 == 0x420
                        })?;
                    }
                }
            }
        }
        let end = cursor.address;
        for flow in &elf.control_flow {
            if (params_start..=end).contains(&flow.target) && !(owner_start..end).contains(&flow.address) {
                return Err(invalid("parameter decoding can be bypassed"));
            }
        }
        if shared.is_some() {
            let done = unique_policy_block(record, &owner.id, ".Lentry_witness_done_")?.range.start;
            cursor.branch(is_jal_zero, done)?;
            cursor.address = done;
            cursor.expect(|word| word == 0x00008067)?;
        } else {
            cursor.expect(|word| is_auipc(word, 1))?;
            let call = cursor.expect(is_jalr_call)?;
            if !flow_targets(elf, call, entry_start(record, &variant.entry_id)?) {
                return Err(invalid("decoded parameters do not flow to their bound action"));
            }
        }
        result.insert(variant.tag, Variant { action: entry.name.clone(), parameters: params });
    }
    Ok(result)
}
