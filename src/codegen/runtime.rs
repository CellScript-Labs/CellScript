use super::*;

mod cell_data;
mod cell_membership;
mod commitment_arithmetic;
mod dao;
mod exact_cell_reads;
mod fungible;
mod headers;
mod memory;
mod metapoints;
mod order_master;
mod out_points;
mod script_fields;
mod sighash;
mod temporal;
mod transaction_reads;
mod witness_reads;

mod dispatch;
mod handles;
mod hashes;

#[cfg(test)]
mod stack_audit;

/// Raw byte sources for the exact-width read ABI. This does not parse a
/// protocol envelope or change the legacy fixed-buffer field getters.
#[derive(Clone, Copy)]
enum RuntimeByteSource {
    CellData,
    Witness,
    CellLock,
    CellType,
}

/// Keep the legacy in-memory artifact unchanged; both transaction byte
/// sources share one streaming compressor with a compiler-selected syscall.
#[derive(Clone, Copy)]
enum RuntimeHashInput {
    Memory,
    Transaction,
    PrefixedTransaction,
    Segments,
}

#[derive(Clone, Copy)]
struct OrderMasterDataOffsets {
    source: usize,
    cell_index: usize,
    action_offset: usize,
    tx_hash_offset: usize,
    index_offset: usize,
    tx_dest: usize,
    index_dest: usize,
    data_buffer: usize,
    size: usize,
}

#[derive(Clone, Copy)]
struct OrderMasterFailureLabels<'a> {
    invalid_action: &'a str,
    malformed: &'a str,
    out_point_failed: &'a str,
}

impl CodeGenerator {
    pub(super) fn generate_runtime_support(&mut self, ir: &IrModule) {
        self.emit_section(".text");
        self.emit_runtime_memcmp_fixed();
        self.emit_runtime_memzero_fixed();
        self.emit_runtime_memcpy_fixed();
        self.emit_runtime_size_guards();
        if ir.items.iter().any(|item| {
            let body = match item {
                IrItem::Action(entry) => &entry.body,
                IrItem::Lock(entry) => &entry.body,
                IrItem::Function(entry) => &entry.body,
                _ => return false,
            };
            body.cell_bindings.iter().any(|binding| binding.membership != IrCellMembership::Unproven)
        }) {
            self.emit_runtime_cell_membership();
        }
        // These scalar getters have no runtime-to-runtime callers. Emit their
        // complete checked implementation only when a retained IR call needs
        // it, rather than adding unused failure paths to every artifact.
        let scalar_helpers = ir
            .items
            .iter()
            .filter_map(|item| match item {
                IrItem::Action(entry) => Some(&entry.body),
                IrItem::Lock(entry) => Some(&entry.body),
                IrItem::Function(entry) => Some(&entry.body),
                _ => None,
            })
            .flat_map(|body| &body.blocks)
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction {
                IrInstruction::Call { func, .. } if func.runtime_helper().is_some_and(is_runtime_header_u64_call) => {
                    Some(func.as_str())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        // CKB exposes epoch-number based timepoints here, not Unix timestamps.
        for (symbol, field_name, field_id, enabled, disabled_reason) in [
            (
                "__env_current_timepoint",
                "ckb_epoch_number",
                CKB_HEADER_FIELD_EPOCH_NUMBER,
                true,
                "env::current_timepoint is required for CKB profile",
            ),
            (
                "__ckb_header_epoch_number",
                "ckb_epoch_number",
                CKB_HEADER_FIELD_EPOCH_NUMBER,
                self.options.target_profile.is_ckb(),
                "ckb::header_epoch_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_epoch_start_block_number",
                "ckb_epoch_start_block_number",
                CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER,
                self.options.target_profile.is_ckb(),
                "ckb::header_epoch_start_block_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_epoch_length",
                "ckb_epoch_length",
                CKB_HEADER_FIELD_EPOCH_LENGTH,
                self.options.target_profile.is_ckb(),
                "ckb::header_epoch_length is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_dep_epoch_number",
                "ckb_epoch_number",
                CKB_HEADER_FIELD_EPOCH_NUMBER,
                self.options.target_profile.is_ckb(),
                "HeaderDepView.epoch_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_dep_epoch_start_block_number",
                "ckb_epoch_start_block_number",
                CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER,
                self.options.target_profile.is_ckb(),
                "HeaderDepView.epoch_start_block_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_dep_epoch_length",
                "ckb_epoch_length",
                CKB_HEADER_FIELD_EPOCH_LENGTH,
                self.options.target_profile.is_ckb(),
                "HeaderDepView.epoch_length is rejected outside the ckb target profile",
            ),
        ] {
            if scalar_helpers.contains(symbol) {
                if symbol.starts_with("__ckb_header_dep_") {
                    self.emit_runtime_header_dep_field_u64(symbol, field_name, field_id, enabled, disabled_reason);
                } else {
                    self.emit_runtime_header_field_u64(symbol, field_name, field_id, enabled, disabled_reason);
                }
            }
        }
        for (symbol, field_name, field_offset, disabled_reason) in [
            (
                "__ckb_header_dep_block_number",
                "number",
                ckb_abi::header::NUMBER_OFFSET,
                "HeaderDepView.block_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_dep_timestamp_millis",
                "timestamp",
                ckb_abi::header::TIMESTAMP_OFFSET,
                "HeaderDepView.timestamp is rejected outside the ckb target profile",
            ),
        ] {
            if scalar_helpers.contains(symbol) {
                self.emit_runtime_header_dep_full_u64(
                    symbol,
                    field_name,
                    field_offset,
                    self.options.target_profile.is_ckb(),
                    disabled_reason,
                );
            }
        }
        if scalar_helpers.contains("__ckb_input_since") {
            self.emit_runtime_input_field_u64(
                "__ckb_input_since",
                "ckb_input_since",
                CKB_INPUT_FIELD_SINCE,
                self.options.target_profile.is_ckb(),
                "ckb::input_since is rejected outside the ckb target profile",
            );
        }
        let mut runtime_helpers = referenced_runtime_helpers(ir);
        if runtime_helpers.contains("__ckb_require_instance_script_participant_from_args") {
            runtime_helpers.insert("__ckb_require_cell_dep_open_script_handle".into());
        }
        self.emit_runtime_helpers(&runtime_helpers);
    }
}

pub fn generate(ir: &IrModule, options: &CodegenOptions, format: ArtifactFormat) -> Result<Vec<u8>> {
    let generator = CodeGenerator::new(options.clone());
    generator.generate(ir, format)
}

pub fn generate_with_evidence(ir: &IrModule, options: &CodegenOptions, format: ArtifactFormat) -> Result<GeneratedArtifact> {
    let generator = CodeGenerator::new(options.clone());
    generator.generate_with_evidence(ir, format)
}

#[derive(Debug, Clone)]
pub struct GeneratedArtifact {
    pub bytes: Vec<u8>,
    pub machine_layout: Option<MachineLayoutEvidence>,
}

#[derive(Debug, Clone)]
pub struct MachineLayoutEvidence {
    pub text_start: u64,
    pub text_end: u64,
    pub entry_label: String,
    pub blocks: Vec<MachineBlockEvidence>,
    pub edges: Vec<MachineEdgeEvidence>,
    pub symbols: BTreeMap<String, u64>,
    /// Text labels in emission order, including labels sharing an address.
    /// A preceding empty IR block must not own the next block's instructions.
    pub symbol_order: Vec<String>,
    pub globals: BTreeSet<String>,
    pub entry_frame_sizes: BTreeMap<String, u32>,
    pub fixed_result_frames: BTreeMap<String, FixedResultFrameEvidence>,
}

#[derive(Debug, Clone)]
pub struct FixedResultFrameEvidence {
    pub scalar_region_bytes: u32,
    pub buffer_region_end: u32,
    pub buffers: Vec<cellscript_artifact_checker::FixedResultBuffer>,
}

#[derive(Debug, Clone)]
pub struct MachineBlockEvidence {
    pub index: usize,
    pub label: Option<String>,
    pub start: u64,
    pub end: u64,
    pub terminator: MachineTerminatorEvidence,
    pub runtime_error_codes: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineTerminatorEvidence {
    Fallthrough,
    Jump,
    ConditionalBranch,
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineEdgeEvidence {
    pub from: usize,
    pub to: usize,
    pub kind: MachineEdgeKindEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineEdgeKindEvidence {
    Fallthrough,
    Jump,
    ConditionalTaken,
    ConditionalFallthrough,
    Call,
}
