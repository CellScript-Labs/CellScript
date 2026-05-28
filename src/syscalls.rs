//! Shared CKB syscall and runtime-helper ABI metadata.
//!
//! This module is the internal source of truth for syscall numbers, wrapper
//! classes, status registers, and fail-closed policy. Codegen/runtime and
//! generated stdlib assembly should consume these specs instead of maintaining
//! independent syscall tables.

#![allow(dead_code)]

use crate::TargetProfile;

pub(crate) const CKB_LOAD_TX_HASH_SYSCALL_NUMBER: u64 = 2061;
pub(crate) const CKB_LOAD_SCRIPT_HASH_SYSCALL_NUMBER: u64 = 2062;
pub(crate) const CKB_LOAD_CELL_SYSCALL_NUMBER: u64 = 2071;
pub(crate) const CKB_LOAD_HEADER_SYSCALL_NUMBER: u64 = 2072;
pub(crate) const CKB_LOAD_INPUT_SYSCALL_NUMBER: u64 = 2073;
pub(crate) const CKB_LOAD_WITNESS_SYSCALL_NUMBER: u64 = 2074;
pub(crate) const CKB_LOAD_SCRIPT_SYSCALL_NUMBER: u64 = 2052;
pub(crate) const CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER: u64 = 2081;
pub(crate) const CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER: u64 = 2082;
pub(crate) const CKB_LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER: u64 = 2083;
pub(crate) const CKB_LOAD_CELL_DATA_SYSCALL_NUMBER: u64 = 2092;
pub(crate) const CKB_CURRENT_CYCLES_SYSCALL_NUMBER: u64 = 2042;

pub(crate) const CKB_VM2_SPAWN_SYSCALL_NUMBER: u64 = 2601;
pub(crate) const CKB_VM2_WAIT_SYSCALL_NUMBER: u64 = 2602;
pub(crate) const CKB_VM2_PROCESS_ID_SYSCALL_NUMBER: u64 = 2603;
pub(crate) const CKB_VM2_PIPE_SYSCALL_NUMBER: u64 = 2604;
pub(crate) const CKB_VM2_PIPE_WRITE_SYSCALL_NUMBER: u64 = 2605;
pub(crate) const CKB_VM2_PIPE_READ_SYSCALL_NUMBER: u64 = 2606;
pub(crate) const CKB_VM2_INHERITED_FD_SYSCALL_NUMBER: u64 = 2607;
pub(crate) const CKB_VM2_CLOSE_SYSCALL_NUMBER: u64 = 2608;

pub(crate) const CKB_SOURCE_INPUT: u64 = 0x01;
pub(crate) const CKB_SOURCE_OUTPUT: u64 = 0x02;
pub(crate) const CKB_SOURCE_CELL_DEP: u64 = 0x03;
pub(crate) const CKB_SOURCE_HEADER_DEP: u64 = 0x04;
pub(crate) const CKB_SOURCE_GROUP_FLAG: u64 = 0x0100_0000_0000_0000;
pub(crate) const CKB_SOURCE_GROUP_INPUT: u64 = CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT;
pub(crate) const CKB_SOURCE_GROUP_OUTPUT: u64 = CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyscallKind {
    Unit,
    Value,
    MultiReturn,
    Recoverable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyscallSizeCheck {
    None,
    CallerChecked,
    LoadedSizePointer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyscallFailBehavior {
    Abort,
    FailClosed,
    ReturnStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LowLevelValueClass {
    Bool,
    DomainU64,
    ErrorCode,
    ExitStatus,
    HelperStatus,
    SyscallStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelperCoverageKind {
    SpecDriven,
    ManualButChecked,
    InternalNoSyscall,
    Deprecated,
    MustMigrate,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HelperInventoryEntry {
    pub(crate) symbol: &'static str,
    pub(crate) coverage: HelperCoverageKind,
    pub(crate) detail: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SyscallSpec {
    pub(crate) dsl_name: &'static str,
    pub(crate) symbol: &'static str,
    pub(crate) number: u64,
    pub(crate) kind: SyscallKind,
    pub(crate) raw_status_reg: &'static str,
    pub(crate) semantic_status_reg: &'static str,
    pub(crate) return_regs: &'static [&'static str],
    pub(crate) arg_comment: &'static str,
    pub(crate) size_check: SyscallSizeCheck,
    pub(crate) fail_behavior: SyscallFailBehavior,
    pub(crate) detail: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RuntimeHelperSpec {
    pub(crate) symbol: &'static str,
    pub(crate) detail: &'static str,
    pub(crate) kind: SyscallKind,
    pub(crate) fail_behavior: SyscallFailBehavior,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceConstantSpec {
    pub(crate) symbol: &'static str,
    pub(crate) detail: &'static str,
    pub(crate) value: u64,
}

const STD_RETURN_A0: &[&str] = &["a0"];
const VM2_RETURN_A0: &[&str] = &["a0"];
const VM2_RETURN_A0_A1: &[&str] = &["a0", "a1"];
const NO_RETURNS: &[&str] = &[];

pub(crate) fn stdlib_syscall_specs(_target_profile: TargetProfile) -> Vec<SyscallSpec> {
    vec![
        SyscallSpec {
            dsl_name: "syscall_load_tx_hash",
            symbol: "__syscall_load_tx_hash",
            number: CKB_LOAD_TX_HASH_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "",
            size_check: SyscallSizeCheck::CallerChecked,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_tx_hash",
        },
        SyscallSpec {
            dsl_name: "syscall_load_script_hash",
            symbol: "__syscall_load_script_hash",
            number: CKB_LOAD_SCRIPT_HASH_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "",
            size_check: SyscallSizeCheck::CallerChecked,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_script_hash",
        },
        SyscallSpec {
            dsl_name: "syscall_load_cell",
            symbol: "__syscall_load_cell",
            number: CKB_LOAD_CELL_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "a0 = index, a1 = source, a2 = field",
            size_check: SyscallSizeCheck::None,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_cell",
        },
        SyscallSpec {
            dsl_name: "syscall_load_header",
            symbol: "__syscall_load_header",
            number: CKB_LOAD_HEADER_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "a0 = buffer, a1 = size pointer, a2 = offset, a3 = index, a4 = source",
            size_check: SyscallSizeCheck::LoadedSizePointer,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_header",
        },
        SyscallSpec {
            dsl_name: "syscall_load_input",
            symbol: "__syscall_load_input",
            number: CKB_LOAD_INPUT_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "a0 = index, a1 = source, a2 = field",
            size_check: SyscallSizeCheck::None,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_input",
        },
        SyscallSpec {
            dsl_name: "syscall_load_witness",
            symbol: "__syscall_load_witness",
            number: CKB_LOAD_WITNESS_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "a0 = buffer, a1 = size pointer, a2 = offset, a3 = index, a4 = source",
            size_check: SyscallSizeCheck::LoadedSizePointer,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_witness",
        },
        SyscallSpec {
            dsl_name: "syscall_load_script",
            symbol: "__syscall_load_script",
            number: CKB_LOAD_SCRIPT_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "a0 = buffer, a1 = size pointer, a2 = offset",
            size_check: SyscallSizeCheck::LoadedSizePointer,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_script",
        },
        SyscallSpec {
            dsl_name: "syscall_load_cell_by_field",
            symbol: "__syscall_load_cell_by_field",
            number: CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "a0 = buffer, a1 = size pointer, a2 = offset, a3 = index, a4 = source, a5 = field",
            size_check: SyscallSizeCheck::LoadedSizePointer,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_cell_by_field",
        },
        SyscallSpec {
            dsl_name: "syscall_load_cell_data",
            symbol: "__syscall_load_cell_data",
            number: CKB_LOAD_CELL_DATA_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "a0 = buffer, a1 = size pointer, a2 = offset, a3 = index, a4 = source",
            size_check: SyscallSizeCheck::LoadedSizePointer,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "load_cell_data",
        },
        SyscallSpec {
            dsl_name: "syscall_current_cycles",
            symbol: "__syscall_current_cycles",
            number: CKB_CURRENT_CYCLES_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a0",
            return_regs: STD_RETURN_A0,
            arg_comment: "",
            size_check: SyscallSizeCheck::None,
            fail_behavior: SyscallFailBehavior::ReturnStatus,
            detail: "current_cycles",
        },
    ]
}

pub(crate) fn vm2_helper_specs() -> &'static [SyscallSpec] {
    &[
        SyscallSpec {
            dsl_name: "spawn",
            symbol: "__ckb_spawn",
            number: CKB_VM2_SPAWN_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a1",
            return_regs: VM2_RETURN_A0,
            arg_comment: "spawn bounded verifier child",
            size_check: SyscallSizeCheck::CallerChecked,
            fail_behavior: SyscallFailBehavior::FailClosed,
            detail: "spawn bounded verifier child",
        },
        SyscallSpec {
            dsl_name: "wait",
            symbol: "__ckb_wait",
            number: CKB_VM2_WAIT_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a1",
            return_regs: VM2_RETURN_A0,
            arg_comment: "wait for bounded verifier child",
            size_check: SyscallSizeCheck::None,
            fail_behavior: SyscallFailBehavior::FailClosed,
            detail: "wait for bounded verifier child",
        },
        SyscallSpec {
            dsl_name: "process_id",
            symbol: "__ckb_process_id",
            number: CKB_VM2_PROCESS_ID_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a1",
            return_regs: VM2_RETURN_A0,
            arg_comment: "current process id",
            size_check: SyscallSizeCheck::None,
            fail_behavior: SyscallFailBehavior::FailClosed,
            detail: "current process id",
        },
        SyscallSpec {
            dsl_name: "pipe",
            symbol: "__ckb_pipe",
            number: CKB_VM2_PIPE_SYSCALL_NUMBER,
            kind: SyscallKind::MultiReturn,
            raw_status_reg: "a0",
            semantic_status_reg: "a1",
            return_regs: VM2_RETURN_A0_A1,
            arg_comment: "create IPC pipe; returns read fd in a0 and write fd in a1",
            size_check: SyscallSizeCheck::None,
            fail_behavior: SyscallFailBehavior::FailClosed,
            detail: "create IPC pipe; returns read fd in a0 and write fd in a1",
        },
        SyscallSpec {
            dsl_name: "pipe_write",
            symbol: "__ckb_pipe_write",
            number: CKB_VM2_PIPE_WRITE_SYSCALL_NUMBER,
            kind: SyscallKind::Unit,
            raw_status_reg: "a0",
            semantic_status_reg: "a1",
            return_regs: NO_RETURNS,
            arg_comment: "write u64 payload to IPC pipe",
            size_check: SyscallSizeCheck::CallerChecked,
            fail_behavior: SyscallFailBehavior::FailClosed,
            detail: "write u64 payload to IPC pipe",
        },
        SyscallSpec {
            dsl_name: "pipe_read",
            symbol: "__ckb_pipe_read",
            number: CKB_VM2_PIPE_READ_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a1",
            return_regs: VM2_RETURN_A0,
            arg_comment: "read u64 payload from IPC pipe",
            size_check: SyscallSizeCheck::CallerChecked,
            fail_behavior: SyscallFailBehavior::FailClosed,
            detail: "read u64 payload from IPC pipe",
        },
        SyscallSpec {
            dsl_name: "inherited_fd",
            symbol: "__ckb_inherited_fd",
            number: CKB_VM2_INHERITED_FD_SYSCALL_NUMBER,
            kind: SyscallKind::Value,
            raw_status_reg: "a0",
            semantic_status_reg: "a1",
            return_regs: VM2_RETURN_A0,
            arg_comment: "resolve inherited fd",
            size_check: SyscallSizeCheck::None,
            fail_behavior: SyscallFailBehavior::FailClosed,
            detail: "resolve inherited fd",
        },
        SyscallSpec {
            dsl_name: "close",
            symbol: "__ckb_close",
            number: CKB_VM2_CLOSE_SYSCALL_NUMBER,
            kind: SyscallKind::Unit,
            raw_status_reg: "a0",
            semantic_status_reg: "a1",
            return_regs: NO_RETURNS,
            arg_comment: "close fd",
            size_check: SyscallSizeCheck::None,
            fail_behavior: SyscallFailBehavior::FailClosed,
            detail: "close fd",
        },
    ]
}

pub(crate) fn fail_closed_runtime_helper_specs() -> &'static [RuntimeHelperSpec] {
    &[
        RuntimeHelperSpec {
            symbol: "__ckb_witness_raw",
            detail: "raw witness bytes",
            kind: SyscallKind::Value,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_witness_lock",
            detail: "WitnessArgs.lock",
            kind: SyscallKind::Value,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_witness_input_type",
            detail: "WitnessArgs.input_type",
            kind: SyscallKind::Value,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_witness_output_type",
            detail: "WitnessArgs.output_type",
            kind: SyscallKind::Value,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_sighash_all",
            detail: "CKB sighash-all digest",
            kind: SyscallKind::Value,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_require_maturity",
            detail: "CKB block-number since maturity",
            kind: SyscallKind::Unit,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_require_time",
            detail: "CKB timestamp since",
            kind: SyscallKind::Unit,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_require_epoch_after",
            detail: "CKB absolute epoch since",
            kind: SyscallKind::Unit,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_require_epoch_relative",
            detail: "CKB relative epoch since",
            kind: SyscallKind::Unit,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
        RuntimeHelperSpec {
            symbol: "__ckb_occupied_capacity",
            detail: "compile-visible occupied capacity floor",
            kind: SyscallKind::Value,
            fail_behavior: SyscallFailBehavior::FailClosed,
        },
    ]
}

pub(crate) fn source_constant_specs() -> &'static [SourceConstantSpec] {
    &[
        SourceConstantSpec { symbol: "__ckb_source_input", detail: "Source::Input", value: CKB_SOURCE_INPUT },
        SourceConstantSpec { symbol: "__ckb_source_output", detail: "Source::Output", value: CKB_SOURCE_OUTPUT },
        SourceConstantSpec { symbol: "__ckb_source_cell_dep", detail: "Source::CellDep", value: CKB_SOURCE_CELL_DEP },
        SourceConstantSpec { symbol: "__ckb_source_header_dep", detail: "Source::HeaderDep", value: CKB_SOURCE_HEADER_DEP },
        SourceConstantSpec { symbol: "__ckb_source_group_input", detail: "Source::GroupInput", value: CKB_SOURCE_GROUP_INPUT },
        SourceConstantSpec { symbol: "__ckb_source_group_output", detail: "Source::GroupOutput", value: CKB_SOURCE_GROUP_OUTPUT },
    ]
}

pub(crate) fn runtime_helper_symbols() -> impl Iterator<Item = &'static str> {
    vm2_helper_specs()
        .iter()
        .map(|spec| spec.symbol)
        .chain(source_constant_specs().iter().map(|spec| spec.symbol))
        .chain(fail_closed_runtime_helper_specs().iter().map(|spec| spec.symbol))
        .chain(manual_runtime_helper_inventory().iter().map(|entry| entry.symbol))
}

pub(crate) fn checked_runtime_helper_spec(symbol: &str) -> Option<SyscallSpec> {
    vm2_helper_specs().iter().copied().find(|spec| spec.symbol == symbol)
}

pub(crate) fn fail_closed_helper_spec(symbol: &str) -> Option<RuntimeHelperSpec> {
    fail_closed_runtime_helper_specs().iter().copied().find(|spec| spec.symbol == symbol)
}

pub(crate) fn helper_inventory_entries(target_profile: TargetProfile) -> Vec<HelperInventoryEntry> {
    let mut entries = Vec::new();
    entries.extend(stdlib_syscall_specs(target_profile).into_iter().map(|spec| HelperInventoryEntry {
        symbol: spec.symbol,
        coverage: HelperCoverageKind::SpecDriven,
        detail: spec.detail,
    }));
    entries.extend(vm2_helper_specs().iter().map(|spec| HelperInventoryEntry {
        symbol: spec.symbol,
        coverage: HelperCoverageKind::SpecDriven,
        detail: spec.detail,
    }));
    entries.extend(source_constant_specs().iter().map(|spec| HelperInventoryEntry {
        symbol: spec.symbol,
        coverage: HelperCoverageKind::SpecDriven,
        detail: spec.detail,
    }));
    entries.extend(fail_closed_runtime_helper_specs().iter().map(|spec| HelperInventoryEntry {
        symbol: spec.symbol,
        coverage: HelperCoverageKind::SpecDriven,
        detail: spec.detail,
    }));
    entries.extend(manual_runtime_helper_inventory());
    entries
}

pub(crate) fn helper_inventory_entry(symbol: &str, target_profile: TargetProfile) -> Option<HelperInventoryEntry> {
    helper_inventory_entries(target_profile).into_iter().find(|entry| entry.symbol == symbol)
}

pub(crate) fn low_level_value_class_for_raw_symbol(symbol: &str) -> Option<LowLevelValueClass> {
    if symbol.starts_with("__syscall_") {
        return Some(LowLevelValueClass::SyscallStatus);
    }
    if checked_runtime_helper_spec(symbol).is_some_and(|spec| spec.kind == SyscallKind::Unit)
        || fail_closed_helper_spec(symbol).is_some_and(|spec| spec.kind == SyscallKind::Unit)
    {
        return Some(LowLevelValueClass::HelperStatus);
    }
    match symbol {
        "__cellscript_error_code" => Some(LowLevelValueClass::ErrorCode),
        "__cellscript_exit_status" => Some(LowLevelValueClass::ExitStatus),
        _ => None,
    }
}

fn manual_runtime_helper_inventory() -> &'static [HelperInventoryEntry] {
    &[
        HelperInventoryEntry {
            symbol: "__env_current_timepoint",
            coverage: HelperCoverageKind::ManualButChecked,
            detail: "checked header epoch helper emitted by runtime and stdlib",
        },
        HelperInventoryEntry {
            symbol: "__ckb_header_epoch_number",
            coverage: HelperCoverageKind::ManualButChecked,
            detail: "checked header epoch number helper emitted by runtime and stdlib",
        },
        HelperInventoryEntry {
            symbol: "__ckb_header_epoch_start_block_number",
            coverage: HelperCoverageKind::ManualButChecked,
            detail: "checked header epoch start block helper emitted by runtime and stdlib",
        },
        HelperInventoryEntry {
            symbol: "__ckb_header_epoch_length",
            coverage: HelperCoverageKind::ManualButChecked,
            detail: "checked header epoch length helper emitted by runtime and stdlib",
        },
        HelperInventoryEntry {
            symbol: "__ckb_input_since",
            coverage: HelperCoverageKind::ManualButChecked,
            detail: "checked input since helper emitted by runtime and stdlib",
        },
        HelperInventoryEntry {
            symbol: "__env_remaining_cycles",
            coverage: HelperCoverageKind::ManualButChecked,
            detail: "checked current cycles helper emitted by stdlib",
        },
        HelperInventoryEntry {
            symbol: "__ckb_hash_chain",
            coverage: HelperCoverageKind::InternalNoSyscall,
            detail: "internal fixed hash-chain helper",
        },
        HelperInventoryEntry {
            symbol: "__ckb_hash_blake2b",
            coverage: HelperCoverageKind::InternalNoSyscall,
            detail: "internal fixed blake2b helper",
        },
        HelperInventoryEntry {
            symbol: "__cellscript_memcmp_fixed",
            coverage: HelperCoverageKind::InternalNoSyscall,
            detail: "internal fixed byte equality helper",
        },
        HelperInventoryEntry {
            symbol: "__cellscript_memzero_fixed",
            coverage: HelperCoverageKind::InternalNoSyscall,
            detail: "internal fixed zero check helper",
        },
        HelperInventoryEntry {
            symbol: "__cellscript_require_min_size",
            coverage: HelperCoverageKind::InternalNoSyscall,
            detail: "internal loaded-size lower-bound helper",
        },
        HelperInventoryEntry {
            symbol: "__cellscript_require_exact_size",
            coverage: HelperCoverageKind::InternalNoSyscall,
            detail: "internal loaded-size exact-width helper",
        },
        HelperInventoryEntry {
            symbol: "__cellscript_validate_molecule_table_offsets",
            coverage: HelperCoverageKind::InternalNoSyscall,
            detail: "internal Molecule table canonicality helper",
        },
        HelperInventoryEntry {
            symbol: "__cellscript_collection_runtime_unavailable",
            coverage: HelperCoverageKind::InternalNoSyscall,
            detail: "non-returning generated collection fail-closed helper",
        },
        HelperInventoryEntry {
            symbol: "__vec_new",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper; dynamic allocation is fail-closed",
        },
        HelperInventoryEntry {
            symbol: "__vec_push",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper; dynamic mutation is fail-closed",
        },
        HelperInventoryEntry {
            symbol: "__vec_len",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper retained for legacy assembly surface",
        },
        HelperInventoryEntry {
            symbol: "__vec_is_empty",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper retained for legacy assembly surface",
        },
        HelperInventoryEntry {
            symbol: "__hashmap_new",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper; dynamic allocation is fail-closed",
        },
        HelperInventoryEntry {
            symbol: "__hashmap_insert",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper; dynamic mutation is fail-closed",
        },
        HelperInventoryEntry {
            symbol: "__hashmap_get",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper; unchecked lookup is fail-closed",
        },
        HelperInventoryEntry {
            symbol: "__hashmap_len",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper retained for legacy assembly surface",
        },
        HelperInventoryEntry {
            symbol: "__hashset_new",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper; aliases fail-closed hashmap allocation",
        },
        HelperInventoryEntry {
            symbol: "__hashset_insert",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper; aliases fail-closed hashmap mutation",
        },
        HelperInventoryEntry {
            symbol: "__hashset_contains",
            coverage: HelperCoverageKind::Deprecated,
            detail: "generated collection helper; aliases fail-closed hashmap lookup",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_low_level_syscall_spec_is_inventoried() {
        for spec in stdlib_syscall_specs(TargetProfile::Ckb) {
            let entry = helper_inventory_entry(spec.symbol, TargetProfile::Ckb).expect("low-level syscall spec must be inventoried");
            assert_eq!(entry.coverage, HelperCoverageKind::SpecDriven, "{entry:?}");
        }
    }

    #[test]
    fn ckb_debug_syscall_is_not_a_production_inventory_surface() {
        assert!(
            stdlib_syscall_specs(TargetProfile::Ckb)
                .into_iter()
                .all(|spec| spec.symbol != "__syscall_debug_print" && spec.number != 2177),
            "debug syscall 2177 must stay out of the CKB production syscall spec table"
        );
        assert!(helper_inventory_entry("__syscall_debug_print", TargetProfile::Ckb).is_none());
    }

    #[test]
    fn ckb_syscall_abi_matches_checked_baseline() {
        let expected_stdlib = baseline_number_pairs("ckb_mainnet_syscalls", "symbol");
        let actual_stdlib = stdlib_syscall_specs(TargetProfile::Ckb)
            .into_iter()
            .map(|spec| (spec.symbol.to_string(), spec.number))
            .collect::<Vec<_>>();
        assert_eq!(actual_stdlib, expected_stdlib);

        let runtime_abi = crate::codegen::runtime_syscall_abi(TargetProfile::Ckb);
        let actual_runtime = vec![
            ("load_header_by_field".to_string(), runtime_abi.load_header_by_field),
            ("load_input_by_field".to_string(), runtime_abi.load_input_by_field),
            ("load_witness".to_string(), runtime_abi.load_witness),
            ("load_script".to_string(), runtime_abi.load_script),
            ("load_cell_by_field".to_string(), runtime_abi.load_cell_by_field),
            ("load_cell_data".to_string(), runtime_abi.load_cell_data),
        ];
        assert_eq!(actual_runtime, baseline_number_pairs("runtime_syscall_abi", "name"));

        let actual_vm2 = vm2_helper_specs().iter().map(|spec| (spec.symbol.to_string(), spec.number)).collect::<Vec<_>>();
        assert_eq!(actual_vm2, baseline_number_pairs("ckb_vm_v2_spawn_ipc_syscalls", "symbol"));
    }

    #[test]
    fn helper_inventory_has_no_duplicate_symbols() {
        let mut seen = BTreeSet::new();
        for entry in helper_inventory_entries(TargetProfile::Ckb) {
            assert!(seen.insert(entry.symbol), "duplicate helper inventory entry for {}", entry.symbol);
        }
    }

    #[test]
    fn emitted_manual_runtime_and_stdlib_helpers_are_classified() {
        for symbol in [
            "__env_current_timepoint",
            "__ckb_header_epoch_number",
            "__ckb_header_epoch_start_block_number",
            "__ckb_header_epoch_length",
            "__ckb_input_since",
            "__env_remaining_cycles",
            "__ckb_hash_chain",
            "__ckb_hash_blake2b",
            "__cellscript_memcmp_fixed",
            "__cellscript_memzero_fixed",
            "__cellscript_require_min_size",
            "__cellscript_require_exact_size",
            "__cellscript_validate_molecule_table_offsets",
            "__cellscript_collection_runtime_unavailable",
        ] {
            let entry = helper_inventory_entry(symbol, TargetProfile::Ckb).unwrap_or_else(|| panic!("{symbol} must be inventoried"));
            assert!(
                matches!(entry.coverage, HelperCoverageKind::ManualButChecked | HelperCoverageKind::InternalNoSyscall),
                "{entry:?}"
            );
        }
    }

    fn baseline_number_pairs(section: &str, name_key: &str) -> Vec<(String, u64)> {
        let baseline: serde_json::Value =
            serde_json::from_str(include_str!("../tests/syscall_abi_baseline.json")).expect("syscall ABI baseline JSON should parse");
        baseline
            .get(section)
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("missing syscall ABI baseline section '{section}'"))
            .iter()
            .map(|entry| {
                let name = entry
                    .get(name_key)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("baseline section '{section}' entry is missing string key '{name_key}'"));
                let number = entry
                    .get("number")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_else(|| panic!("baseline section '{section}' entry '{name}' is missing u64 number"));
                (name.to_string(), number)
            })
            .collect()
    }
}
