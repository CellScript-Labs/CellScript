pub mod ckb_protocols;
pub mod collections;

use crate::{ckb_blake2b256, ir::IrType, runtime_errors::CellScriptRuntimeError, TargetProfile};

pub struct StdLib;

impl StdLib {
    pub fn functions() -> Vec<StdFunction> {
        vec![
            StdFunction {
                name: "math_min".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_max".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_isqrt".to_string(),
                params: vec![("n".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_abs_diff".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction { name: "env_current_timepoint".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "ckb_header_epoch_number".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "ckb_header_epoch_start_block_number".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "ckb_header_epoch_length".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "ckb_input_since".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "env_remaining_cycles".to_string(), params: vec![], return_type: Some(IrType::U64) },
        ]
    }

    pub fn is_std_function(name: &str) -> bool {
        Self::functions().iter().any(|f| f.name == name)
    }

    pub fn get_function(name: &str) -> Option<StdFunction> {
        Self::functions().into_iter().find(|f| f.name == name)
    }

    pub fn generate_assembly() -> String {
        Self::generate_assembly_for_target_profile(TargetProfile::Ckb)
    }

    pub fn generate_assembly_for_target_profile(target_profile: TargetProfile) -> String {
        let mut asm = String::new();

        asm.push_str("# CellScript Standard Library\n\n");
        asm.push_str(".section .text\n\n");

        asm.push_str(&Self::generate_math());

        asm.push_str(&Self::generate_env(target_profile));

        asm
    }

    fn generate_math() -> String {
        let mut asm = String::new();

        // math_min
        asm.push_str("# Math: min\n");
        asm.push_str(".global __math_min\n");
        asm.push_str("__math_min:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    bltu a0, a1, .Lmin_ret_a\n");
        asm.push_str("    mv a0, a1\n");
        asm.push_str(".Lmin_ret_a:\n");
        asm.push_str("    ret\n\n");

        // math_max
        asm.push_str("# Math: max\n");
        asm.push_str(".global __math_max\n");
        asm.push_str("__math_max:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    bltu a1, a0, .Lmax_ret_a\n");
        asm.push_str("    mv a0, a1\n");
        asm.push_str(".Lmax_ret_a:\n");
        asm.push_str("    ret\n\n");

        asm.push_str("# Math: isqrt (integer square root)\n");
        asm.push_str(".global __math_isqrt\n");
        asm.push_str("__math_isqrt:\n");
        asm.push_str("    addi sp, sp, -32\n");
        asm.push_str("    sd ra, 24(sp)\n");
        asm.push_str("    sd s0, 16(sp)\n");
        asm.push_str("    sd s1, 8(sp)\n");
        asm.push_str("    # a0 = n\n");
        asm.push_str("    beqz a0, .Lisqrt_ret\n");
        asm.push_str("    mv s0, a0          # x = n\n");
        asm.push_str("    srli s1, a0, 1\n");
        asm.push_str("    addi s1, s1, 1     # y = (x + 1) / 2\n");
        asm.push_str(".Lisqrt_loop:\n");
        asm.push_str("    bgeu s1, s0, .Lisqrt_ret\n");
        asm.push_str("    mv s0, s1          # x = y\n");
        asm.push_str("    divu t0, a0, s0\n");
        asm.push_str("    add s1, s0, t0\n");
        asm.push_str("    srli s1, s1, 1     # y = (x + n/x) / 2\n");
        asm.push_str("    j .Lisqrt_loop\n");
        asm.push_str(".Lisqrt_ret:\n");
        asm.push_str("    mv a0, s0\n");
        asm.push_str("    ld ra, 24(sp)\n");
        asm.push_str("    ld s0, 16(sp)\n");
        asm.push_str("    ld s1, 8(sp)\n");
        asm.push_str("    addi sp, sp, 32\n");
        asm.push_str("    ret\n\n");

        // math_abs_diff
        asm.push_str("# Math: abs_diff\n");
        asm.push_str(".global __math_abs_diff\n");
        asm.push_str("__math_abs_diff:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    sub t0, a0, a1\n");
        asm.push_str("    bgez t0, .Labs_diff_ret\n");
        asm.push_str("    neg t0, t0\n");
        asm.push_str(".Labs_diff_ret:\n");
        asm.push_str("    mv a0, t0\n");
        asm.push_str("    ret\n\n");

        asm
    }

    fn generate_env(target_profile: TargetProfile) -> String {
        let mut asm = String::new();

        asm.push_str("# Env: current_timepoint (CKB epoch number, not Unix timestamp)\n");
        Self::push_ckb_header_epoch_helper(&mut asm, "__env_current_timepoint", "ckb_epoch_number", 0, true);

        Self::push_ckb_header_epoch_helper(
            &mut asm,
            "__ckb_header_epoch_number",
            "ckb_epoch_number",
            0,
            target_profile == TargetProfile::Ckb,
        );
        Self::push_ckb_header_epoch_helper(
            &mut asm,
            "__ckb_header_epoch_start_block_number",
            "ckb_epoch_start_block_number",
            1,
            target_profile == TargetProfile::Ckb,
        );
        Self::push_ckb_header_epoch_helper(
            &mut asm,
            "__ckb_header_epoch_length",
            "ckb_epoch_length",
            2,
            target_profile == TargetProfile::Ckb,
        );
        Self::push_ckb_input_since_helper(&mut asm, target_profile == TargetProfile::Ckb);

        // env_remaining_cycles
        asm.push_str("# Env: remaining_cycles\n");
        asm.push_str(".global __env_remaining_cycles\n");
        asm.push_str("__env_remaining_cycles:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2042  # CURRENT_CYCLES\n");
        asm.push_str("    ecall\n");
        asm.push_str("    # a0 = current cycles\n");
        asm.push_str("    li t0, 10000000  # max cycles\n");
        asm.push_str("    sub a0, t0, a0   # remaining\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        asm
    }

    fn push_ckb_header_epoch_helper(asm: &mut String, symbol: &str, field_name: &str, field_id: u64, enabled: bool) {
        asm.push_str(&format!("# Env: {}\n", field_name));
        asm.push_str(&format!(".global {}\n", symbol));
        asm.push_str(&format!("{}:\n", symbol));
        if !enabled {
            asm.push_str("    # rejected outside ckb target-profile policy\n");
            asm.push_str(&format!(
                "    # cellscript runtime error {} {}\n",
                CellScriptRuntimeError::ConsumeInvalidOperand.code(),
                CellScriptRuntimeError::ConsumeInvalidOperand.name()
            ));
            asm.push_str("    li a0, 0\n");
            asm.push_str(&format!("    li a1, {}\n", CellScriptRuntimeError::ConsumeInvalidOperand.code()));
            asm.push_str("    ret\n\n");
            return;
        }
        let fail_label = format!(".L{}_fail", symbol.trim_start_matches("__"));
        asm.push_str("    addi sp, sp, -32\n");
        asm.push_str("    sd ra, 24(sp)\n");
        asm.push_str("    # Load from CKB header dep\n");
        asm.push_str("    li t0, 8\n");
        asm.push_str("    sd t0, 8(sp)\n");
        asm.push_str("    addi a0, sp, 16\n");
        asm.push_str("    addi a1, sp, 8\n");
        asm.push_str("    li a2, 0     # offset\n");
        asm.push_str("    li a3, 0     # header index\n");
        asm.push_str("    li a4, 4     # Source::HeaderDep\n");
        asm.push_str(&format!("    li a5, {}     # field = {}\n", field_id, field_name));
        asm.push_str("    li a7, 2082  # LOAD_HEADER_BY_FIELD\n");
        asm.push_str("    ecall\n");
        asm.push_str(&format!("    bnez a0, {}\n", fail_label));
        asm.push_str("    ld t0, 8(sp)\n");
        asm.push_str("    li t1, 8\n");
        asm.push_str(&format!("    bne t0, t1, {}\n", fail_label));
        asm.push_str("    ld a0, 16(sp)\n");
        asm.push_str("    li a1, 0\n");
        asm.push_str("    ld ra, 24(sp)\n");
        asm.push_str("    addi sp, sp, 32\n");
        asm.push_str("    ret\n");
        asm.push_str(&format!("{}:\n", fail_label));
        asm.push_str(&format!(
            "    # cellscript runtime error {} {}\n",
            CellScriptRuntimeError::SyscallFailed.code(),
            CellScriptRuntimeError::SyscallFailed.name()
        ));
        asm.push_str("    li a0, 0\n");
        asm.push_str(&format!("    li a1, {}\n", CellScriptRuntimeError::SyscallFailed.code()));
        asm.push_str("    ld ra, 24(sp)\n");
        asm.push_str("    addi sp, sp, 32\n");
        asm.push_str("    ret\n\n");
    }

    fn push_ckb_input_since_helper(asm: &mut String, enabled: bool) {
        asm.push_str("# Env: ckb_input_since\n");
        asm.push_str(".global __ckb_input_since\n");
        asm.push_str("__ckb_input_since:\n");
        if !enabled {
            asm.push_str("    # rejected outside ckb target-profile policy\n");
            asm.push_str(&format!(
                "    # cellscript runtime error {} {}\n",
                CellScriptRuntimeError::ConsumeInvalidOperand.code(),
                CellScriptRuntimeError::ConsumeInvalidOperand.name()
            ));
            asm.push_str("    li a0, 0\n");
            asm.push_str(&format!("    li a1, {}\n", CellScriptRuntimeError::ConsumeInvalidOperand.code()));
            asm.push_str("    ret\n\n");
            return;
        }
        let fail_label = ".Lckb_input_since_fail";
        asm.push_str("    addi sp, sp, -32\n");
        asm.push_str("    sd ra, 24(sp)\n");
        asm.push_str("    # Load CKB input since from current script group\n");
        asm.push_str("    li t0, 8\n");
        asm.push_str("    sd t0, 8(sp)\n");
        asm.push_str("    addi a0, sp, 16\n");
        asm.push_str("    addi a1, sp, 8\n");
        asm.push_str("    li a2, 0     # offset\n");
        asm.push_str("    li a3, 0     # group input index\n");
        asm.push_str("    li a4, 72057594037927937  # Source::GroupInput\n");
        asm.push_str("    li a5, 1     # field = Since\n");
        asm.push_str("    li a7, 2083  # LOAD_INPUT_BY_FIELD\n");
        asm.push_str("    ecall\n");
        asm.push_str(&format!("    bnez a0, {}\n", fail_label));
        asm.push_str("    ld t0, 8(sp)\n");
        asm.push_str("    li t1, 8\n");
        asm.push_str(&format!("    bne t0, t1, {}\n", fail_label));
        asm.push_str("    ld a0, 16(sp)\n");
        asm.push_str("    li a1, 0\n");
        asm.push_str("    ld ra, 24(sp)\n");
        asm.push_str("    addi sp, sp, 32\n");
        asm.push_str("    ret\n");
        asm.push_str(&format!("{}:\n", fail_label));
        asm.push_str(&format!(
            "    # cellscript runtime error {} {}\n",
            CellScriptRuntimeError::SyscallFailed.code(),
            CellScriptRuntimeError::SyscallFailed.name()
        ));
        asm.push_str("    li a0, 0\n");
        asm.push_str(&format!("    li a1, {}\n", CellScriptRuntimeError::SyscallFailed.code()));
        asm.push_str("    ld ra, 24(sp)\n");
        asm.push_str("    addi sp, sp, 32\n");
        asm.push_str("    ret\n\n");
    }
}

#[derive(Debug, Clone)]
pub struct StdFunction {
    pub name: String,
    pub params: Vec<(String, IrType)>,
    pub return_type: Option<IrType>,
}

pub struct SchedulerMetadata;

/// Scheduler-visible CKB runtime access summary.
#[derive(Debug, Clone)]
pub struct SchedulerAccess {
    pub operation: String,
    pub source: String,
    pub index: u32,
    pub binding: String,
}

impl SchedulerMetadata {
    pub fn generate(
        effect_class: &str,
        parallelizable: bool,
        touches_shared: Vec<[u8; 32]>,
        estimated_cycles: u64,
        accesses: Vec<SchedulerAccess>,
    ) -> Vec<u8> {
        Self::generate_molecule(effect_class, parallelizable, touches_shared, estimated_cycles, accesses)
    }

    pub fn generate_molecule(
        effect_class: &str,
        parallelizable: bool,
        touches_shared: Vec<[u8; 32]>,
        estimated_cycles: u64,
        accesses: Vec<SchedulerAccess>,
    ) -> Vec<u8> {
        let effect_class_id = match effect_class {
            "Pure" => 0,
            "ReadOnly" => 1,
            "Mutating" => 2,
            "Creating" => 3,
            "Destroying" => 4,
            _ => 0,
        };

        let accesses = accesses
            .into_iter()
            .map(|access| {
                let mut out = Vec::with_capacity(38);
                out.push(scheduler_operation_id(&access.operation));
                out.push(scheduler_source_id(&access.source));
                out.extend_from_slice(&access.index.to_le_bytes());
                out.extend_from_slice(&ckb_blake2b256(access.binding.as_bytes()));
                out
            })
            .collect::<Vec<_>>();

        scheduler_molecule_encode_table(&[
            0xCE11u16.to_le_bytes().to_vec(),
            vec![1],
            vec![effect_class_id],
            vec![u8::from(parallelizable)],
            (touches_shared.len() as u32).to_le_bytes().to_vec(),
            scheduler_molecule_encode_fixvec_byte32(&touches_shared),
            estimated_cycles.to_le_bytes().to_vec(),
            (accesses.len() as u32).to_le_bytes().to_vec(),
            scheduler_molecule_encode_fixvec_access(&accesses),
        ])
    }
}

fn scheduler_operation_id(operation: &str) -> u8 {
    match operation {
        "consume" => 1,
        "transfer" => 2,
        "destroy" => 3,
        "claim" => 4,
        "settle" => 5,
        "read_ref" => 6,
        "create" => 7,
        "mutate-input" => 8,
        "mutate-output" => 9,
        _ => 0,
    }
}

fn scheduler_source_id(source: &str) -> u8 {
    match source {
        "Input" => 1,
        "CellDep" => 2,
        "Output" => 3,
        _ => 0,
    }
}

fn scheduler_molecule_pack_number(value: usize) -> [u8; 4] {
    (value as u32).to_le_bytes()
}

fn scheduler_molecule_encode_table(fields: &[Vec<u8>]) -> Vec<u8> {
    let header_size = 4 * (fields.len() + 1);
    let total_size = header_size + fields.iter().map(Vec::len).sum::<usize>();
    let mut out = Vec::with_capacity(total_size);
    out.extend_from_slice(&scheduler_molecule_pack_number(total_size));

    let mut offset = header_size;
    for field in fields {
        out.extend_from_slice(&scheduler_molecule_pack_number(offset));
        offset += field.len();
    }
    for field in fields {
        out.extend_from_slice(field);
    }
    out
}

fn scheduler_molecule_encode_fixvec_byte32(values: &[[u8; 32]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + values.len() * 32);
    out.extend_from_slice(&scheduler_molecule_pack_number(values.len()));
    for value in values {
        out.extend_from_slice(value);
    }
    out
}

fn scheduler_molecule_encode_fixvec_access(accesses: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + accesses.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(&scheduler_molecule_pack_number(accesses.len()));
    for access in accesses {
        out.extend_from_slice(access);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_std_functions() {
        let funcs = StdLib::functions();
        assert!(!funcs.is_empty());
        assert!(!StdLib::is_std_function("syscall_load_cell"));
        assert!(!StdLib::is_std_function("syscall_load_script"));
        assert!(!StdLib::is_std_function("syscall_load_cell_by_field"));
        assert!(!StdLib::is_std_function("syscall_load_cell_data"));
        assert!(!StdLib::is_std_function("syscall_current_cycles"));
        assert!(StdLib::is_std_function("math_isqrt"));
        assert!(StdLib::is_std_function("env_current_timepoint"));
    }

    #[test]
    fn test_get_function() {
        let func = StdLib::get_function("math_min");
        assert!(func.is_some());
        let func = func.unwrap();
        assert_eq!(func.params.len(), 2);
    }

    #[test]
    fn test_generate_assembly() {
        let asm = StdLib::generate_assembly();
        assert!(!asm.contains(".global __syscall_"));
        assert!(!asm.contains("__syscall_load_cell"));
        assert!(!asm.contains("__syscall_load_script:\n"));
        assert!(!asm.contains("__syscall_load_cell_by_field:\n"));
        assert!(!asm.contains("__syscall_load_cell_data:\n"));
        assert!(!asm.contains("__hash"));
        assert!(!asm.contains("3001"));
        assert!(!asm.contains("li a7, 2100"));
        assert!(asm.contains("__math_isqrt"));
        assert!(asm.contains("__env_current_timepoint"));
    }

    #[test]
    fn test_generate_ckb_assembly_uses_checked_env_helpers() {
        let asm = StdLib::generate_assembly_for_target_profile(TargetProfile::Ckb);
        assert!(!asm.contains("# Syscall: load_script (2052)"));
        assert!(!asm.contains("li a7, 2052"));
        assert!(!asm.contains("li a7, 2075"));
        assert!(asm.contains("current_timepoint"));
        assert!(asm.contains("__ckb_input_since"));
        assert!(asm.contains("li a7, 2083  # LOAD_INPUT_BY_FIELD"));
        assert!(asm.contains("Source::GroupInput"));
        assert!(asm.contains("addi a0, sp, 16"));
        assert!(asm.contains("addi a1, sp, 8"));
        assert!(asm.contains("bnez a0, .Lenv_current_timepoint_fail"));
        assert!(asm.contains("li a1, 0"));
        assert!(
            asm.contains("# cellscript runtime error 1 syscall-failed\n    li a0, 0\n    li a1, 1"),
            "generated stdlib helpers must return failure via status a1 instead of forging data:\n{}",
            asm
        );
    }

    #[test]
    fn generated_stdlib_omits_raw_syscall_wrappers() {
        let asm = StdLib::generate_assembly_for_target_profile(TargetProfile::Ckb);
        assert!(!asm.contains(".global __syscall_"), "generated stdlib must not expose raw syscall wrappers:\n{}", asm);
        assert!(!asm.contains("# Syscall:"), "generated stdlib must not duplicate raw syscall ABI comments:\n{}", asm);
    }

    #[test]
    fn generated_stdlib_has_no_raw_syscall_wrapper_symbols() {
        let asm = StdLib::generate_assembly_for_target_profile(TargetProfile::Ckb);
        let generated = asm
            .lines()
            .filter_map(|line| line.strip_prefix(".global "))
            .filter(|symbol| symbol.starts_with("__syscall_"))
            .collect::<std::collections::BTreeSet<_>>();

        assert!(generated.is_empty(), "generated stdlib must not emit raw syscall wrappers: {generated:?}");
    }

    #[test]
    fn test_scheduler_metadata_generate_molecule_uses_table_layout() {
        let bytes = SchedulerMetadata::generate(
            "Creating",
            false,
            vec![[0x42; 32]],
            64,
            vec![SchedulerAccess {
                operation: "create".to_string(),
                source: "Output".to_string(),
                index: 0,
                binding: "create:Output#0".to_string(),
            }],
        );

        assert!(!bytes.starts_with(&[0x11, 0xCE, 1]));
        assert_eq!(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize, bytes.len());
        assert_eq!(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 40);
        assert_eq!(&bytes[40..42], &[0x11, 0xCE]);
    }
}
