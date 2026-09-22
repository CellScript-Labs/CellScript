//! Explicit Type-policy entry dispatch. The policy envelope is not a replacement
//! for the positional entry ABI: each selected record feeds that existing decoder
//! through a private, preloaded adapter, after the entire envelope is validated.

use super::*;
use crate::artifact::ArtifactDeclaration;
use crate::policy_witness::{
    PolicyScriptRole, MAX_POLICY_WITNESS_BUNDLE_BYTES, MAX_POLICY_WITNESS_BYTES, MAX_POLICY_WITNESS_RECORDS, POLICY_WITNESS_MAGIC,
};

// Keep the witness offsets identical to abi.rs so its canonical WitnessArgs
// normalizer can be reused. All scanner state beyond the buffer is explicit;
// no caller-owned stack memory or callee-saved registers are used.
const POLICY_HASH_SIZE_OFFSET: usize = ENTRY_WITNESS_BUFFER_OFFSET + ENTRY_WITNESS_BUFFER_SIZE;
const POLICY_HASH_BUFFER_OFFSET: usize = POLICY_HASH_SIZE_OFFSET + 8;
const POLICY_ARGS_POINTER_OFFSET: usize = POLICY_HASH_BUFFER_OFFSET + 32;
const POLICY_ARGS_LENGTH_OFFSET: usize = POLICY_ARGS_POINTER_OFFSET + 8;
const POLICY_TAG_OFFSET: usize = POLICY_ARGS_LENGTH_OFFSET + 8;
const POLICY_FOUND_OFFSET: usize = POLICY_TAG_OFFSET + 8;
const POLICY_ENTRY_FRAME_SIZE: usize = (POLICY_FOUND_OFFSET + 8 + 8 + 15) & !15;
const POLICY_ENTRY_RA_OFFSET: usize = POLICY_ENTRY_FRAME_SIZE - 8;
const _: () = assert!(POLICY_ENTRY_FRAME_SIZE.is_multiple_of(16) && POLICY_FOUND_OFFSET + 8 <= POLICY_ENTRY_RA_OFFSET);

const POLICY_RECORD_FIXED_BYTES: usize = 61;
const POLICY_RECORD_OFFSETS: [usize; 4] = [20, 21, 53, 57];

impl CodeGenerator {
    pub(super) fn emit_policy_entry_wrapper(&mut self, declaration: &ArtifactDeclaration, ir: &IrModule) -> Result<()> {
        let declaration = declaration.canonicalized()?;
        let mut actions = Vec::with_capacity(declaration.actions.len());
        for variant in &declaration.actions {
            let action = policy_action(ir, &variant.action)?;
            actions.push((variant.tag, action, self.fresh_label("policy_action_adapter"), self.fresh_label("policy_variant")));
        }
        for name in &declaration.common_checks {
            let action = policy_action(ir, name)?;
            if !action.params.is_empty() {
                return Err(CompileError::without_span(format!("policy common check '{name}' must have no parameters")));
            }
        }

        let fail = self.fresh_label("policy_witness_fail");
        let done = self.fresh_label("policy_entry_done");
        self.entry_frame_sizes.insert(ENTRY_WITNESS_LABEL.to_string(), POLICY_ENTRY_FRAME_SIZE as u32);
        self.emit_global(ENTRY_WITNESS_LABEL);
        self.emit_label(ENTRY_WITNESS_LABEL);
        self.emit(format!("# cellscript policy entry: {} Type-group policy-witness-v1", declaration.name));
        self.emit_large_addi("sp", "sp", -(POLICY_ENTRY_FRAME_SIZE as i64));
        self.emit_stack_store("ra", POLICY_ENTRY_RA_OFFSET);
        self.emit_policy_load_witness(&fail);
        self.emit_policy_validate_and_select_record(&fail);

        if !declaration.common_checks.is_empty() {
            let checked_tag = self.fresh_label("policy_declared_tag");
            self.emit_stack_load("t0", POLICY_TAG_OFFSET);
            for (tag, _, _, _) in &actions {
                self.emit(format!("li t1, {tag}"));
                self.emit(format!("beq t0, t1, {checked_tag}"));
            }
            self.emit(format!("j {fail}"));
            self.emit_label(&checked_tag);
            self.emit("# cellscript policy: common Unit actions return status zero; preserve first nonzero failure");
            for name in &declaration.common_checks {
                self.emit_entry_call_target(name, 0);
                self.emit(format!("bnez a0, {done}"));
            }
        }

        self.emit_stack_load("t0", POLICY_TAG_OFFSET);
        for (tag, _, _, selected) in &actions {
            self.emit(format!("li t1, {tag}"));
            self.emit(format!("beq t0, t1, {selected}"));
        }
        self.emit(format!("j {fail}"));
        for (_, _, adapter, selected) in &actions {
            self.emit_label(selected);
            self.emit_stack_load("a0", POLICY_ARGS_POINTER_OFFSET);
            self.emit_stack_load("a1", POLICY_ARGS_LENGTH_OFFSET);
            self.emit_entry_call_target(adapter, 0);
            self.emit(format!("j {done}"));
        }
        self.emit_label(&fail);
        self.emit_process_failure(CellScriptRuntimeError::EntryWitnessAbiInvalid);
        self.emit_label(&done);
        self.emit_stack_load("ra", POLICY_ENTRY_RA_OFFSET);
        self.emit_large_addi("sp", "sp", POLICY_ENTRY_FRAME_SIZE as i64);
        self.emit("ret");

        let mut groups: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
        for (index, (_, action, _, _)) in actions.iter().enumerate() {
            if let Some((key, frame)) = self.policy_decoder_key(&action.name) {
                groups.entry(key).or_default().push((index, frame));
            }
        }
        let mut selected_decoders = BTreeMap::new();
        let mut decoders = Vec::new();
        for group in groups.values().filter(|group| group.len() > 1) {
            let label = self.fresh_label("policy_shared_decoder");
            for (index, frame) in group {
                selected_decoders.insert(*index, (label.clone(), *frame));
            }
            decoders.push((group[0].0, label));
        }
        for (index, (_, action, adapter, _)) in actions.iter().enumerate() {
            if let Some((decoder, frame)) = selected_decoders.get(&index) {
                self.emit_policy_decoder_stub(&action.name, adapter, decoder, *frame);
            } else {
                self.emit_policy_action_adapter(&action.name, &action.params, adapter)?;
            }
        }
        for (index, label) in decoders {
            let action = actions[index].1;
            self.emit_policy_shared_decoder(&action.name, &action.params, &label)?;
        }
        Ok(())
    }

    fn emit_policy_load_witness(&mut self, fail: &str) {
        let loaded = self.fresh_label("policy_witness_loaded");
        let output_only = self.fresh_label("policy_output_only");
        self.emit("# cellscript policy: GroupInput#0 witness; output fallback only after proving an empty input group");
        self.emit_load_witness_syscall_to_offsets(
            "policy_args",
            CKB_SOURCE_GROUP_INPUT,
            0,
            ENTRY_WITNESS_SIZE_OFFSET,
            ENTRY_WITNESS_BUFFER_OFFSET,
            ENTRY_WITNESS_BUFFER_SIZE,
        );
        self.emit(format!("beqz a0, {loaded}"));
        self.emit_load_cell_by_field_syscall_to_offsets(
            "policy_input_group_presence",
            CKB_SOURCE_GROUP_INPUT,
            0,
            CKB_CELL_FIELD_CAPACITY,
            ENTRY_WITNESS_SIZE_OFFSET,
            ENTRY_WITNESS_BUFFER_OFFSET,
            8,
        );
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {output_only}"));
        self.emit(format!("j {fail}"));
        self.emit_label(&output_only);
        self.emit_load_witness_syscall_to_offsets(
            "policy_args_output_only",
            CKB_SOURCE_GROUP_OUTPUT,
            0,
            ENTRY_WITNESS_SIZE_OFFSET,
            ENTRY_WITNESS_BUFFER_OFFSET,
            ENTRY_WITNESS_BUFFER_SIZE,
        );
        self.emit(format!("bnez a0, {fail}"));
        self.emit_label(&loaded);
        self.emit_stack_load("t0", ENTRY_WITNESS_SIZE_OFFSET);
        self.emit(format!("li t1, {MAX_POLICY_WITNESS_BYTES}"));
        self.emit(format!("bltu t1, t0, {fail}"));
        self.emit_entry_normalize_witness_args_input_type_v2(fail);
    }

    fn emit_policy_validate_and_select_record(&mut self, fail: &str) {
        let record_loop = self.fresh_label("policy_record");
        let final_record = self.fresh_label("policy_final_record");
        let have_end = self.fresh_label("policy_record_end");
        let args_valid = self.fresh_label("policy_record_args_valid");
        let key_loop = self.fresh_label("policy_key_order_loop");
        let ordered = self.fresh_label("policy_key_ordered");
        let hash_loop = self.fresh_label("policy_current_hash_loop");
        let next_record = self.fresh_label("policy_next_record");

        self.emit("# cellscript policy: validate full canonical bounded DynVec before any action executes");
        self.emit_stack_load("t0", ENTRY_WITNESS_SIZE_OFFSET);
        self.emit(format!("li t1, {}", POLICY_WITNESS_MAGIC.len() + 8 + POLICY_RECORD_FIXED_BYTES));
        self.emit(format!("bltu t0, t1, {fail}"));
        self.emit(format!("li t1, {MAX_POLICY_WITNESS_BUNDLE_BYTES}"));
        self.emit(format!("bltu t1, t0, {fail}"));
        for (index, byte) in POLICY_WITNESS_MAGIC.iter().enumerate() {
            self.emit_stack_load_byte("t0", ENTRY_WITNESS_BUFFER_OFFSET + index);
            self.emit(format!("li t1, {byte}"));
            self.emit(format!("bne t0, t1, {fail}"));
        }

        // Syscalls precede the scanner. No argument register is assumed live
        // across them. Require a complete current Script hash, not code_hash.
        self.emit("li t0, 32");
        self.emit_stack_store("t0", POLICY_HASH_SIZE_OFFSET);
        self.emit_sp_addi("a0", POLICY_HASH_BUFFER_OFFSET);
        self.emit_sp_addi("a1", POLICY_HASH_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", self.runtime_abi().load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {fail}"));
        self.emit_stack_load("t0", POLICY_HASH_SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit(format!("bne t0, t1, {fail}"));
        self.emit_stack_store("zero", POLICY_FOUND_OFFSET);

        // Scanner ABI: a4=DynVec base, a7=end, a2=offset cursor,
        // a3=header end, a5=current record, a6=previous 33-byte key.
        // These survive every inline helper here; t0..t6 are scratch. All
        // durable selected-record state is stored before any action call.
        self.emit_sp_addi("a4", ENTRY_WITNESS_BUFFER_OFFSET + POLICY_WITNESS_MAGIC.len());
        self.emit_stack_load("t1", ENTRY_WITNESS_SIZE_OFFSET);
        self.emit(format!("addi t1, t1, -{}", POLICY_WITNESS_MAGIC.len()));
        self.emit_u32_le_from_base_to("t0", "a4", 0, "t4");
        self.emit(format!("bne t0, t1, {fail}"));
        self.emit("add a7, a4, t0");
        self.emit_u32_le_from_base_to("t1", "a4", 4, "t4");
        self.emit("li t2, 8");
        self.emit(format!("bltu t1, t2, {fail}"));
        self.emit(format!("li t2, {}", 4 * (MAX_POLICY_WITNESS_RECORDS + 1)));
        self.emit(format!("bltu t2, t1, {fail}"));
        self.emit("li t2, 3");
        self.emit("and t2, t1, t2");
        self.emit(format!("bnez t2, {fail}"));
        self.emit(format!("bltu t0, t1, {fail}"));
        self.emit("add a3, a4, t1");
        self.emit("addi a2, a4, 4");
        self.emit("li a6, 0");

        self.emit_label(&record_loop);
        self.emit_u32_le_from_base_to("t0", "a2", 0, "t4");
        self.emit("addi a2, a2, 4");
        self.emit(format!("beq a2, a3, {final_record}"));
        self.emit_u32_le_from_base_to("t1", "a2", 0, "t4");
        self.emit(format!("j {have_end}"));
        self.emit_label(&final_record);
        self.emit("sub t1, a7, a4");
        self.emit_label(&have_end);
        self.emit("sub t2, a3, a4");
        self.emit(format!("bltu t0, t2, {fail}"));
        self.emit(format!("bgeu t0, t1, {fail}"));
        self.emit("sub t2, a7, a4");
        self.emit(format!("bltu t2, t1, {fail}"));
        self.emit("sub t2, t1, t0");
        self.emit(format!("li t3, {POLICY_RECORD_FIXED_BYTES}"));
        self.emit(format!("bltu t2, t3, {fail}"));
        self.emit("add a5, a4, t0");
        self.emit_u32_le_from_base_to("t3", "a5", 0, "t4");
        self.emit(format!("bne t2, t3, {fail}"));
        for (index, offset) in POLICY_RECORD_OFFSETS.iter().enumerate() {
            self.emit_u32_le_from_base_to("t3", "a5", (index + 1) * 4, "t4");
            self.emit(format!("li t4, {offset}"));
            self.emit(format!("bne t3, t4, {fail}"));
        }
        self.emit_u32_le_from_base_to("t3", "a5", 57, "t4");
        self.emit(format!("addi t2, t2, -{POLICY_RECORD_FIXED_BYTES}"));
        self.emit(format!("bne t2, t3, {fail}"));
        self.emit("lbu t0, 20(a5)");
        self.emit("li t1, 2");
        self.emit(format!("bgeu t0, t1, {fail}"));

        // Foreign records have no local selector interpretation, but their
        // args still obey the common empty-or-CSARG framing contract.
        self.emit(format!("beqz t3, {args_valid}"));
        self.emit(format!("li t4, {}", ENTRY_WITNESS_MAGIC.len()));
        self.emit(format!("bltu t3, t4, {fail}"));
        for (index, byte) in ENTRY_WITNESS_MAGIC.iter().enumerate() {
            self.emit(format!("lbu t0, {}(a5)", POLICY_RECORD_FIXED_BYTES + index));
            self.emit(format!("li t1, {byte}"));
            self.emit(format!("bne t0, t1, {fail}"));
        }
        self.emit_label(&args_valid);
        self.emit(format!("beqz a6, {ordered}"));
        self.emit("addi t0, a6, 0");
        self.emit("addi t1, a5, 20");
        self.emit("li t2, 33");
        self.emit_label(&key_loop);
        self.emit("lbu t3, 0(t0)");
        self.emit("lbu t4, 0(t1)");
        self.emit(format!("bltu t3, t4, {ordered}"));
        self.emit(format!("bltu t4, t3, {fail}"));
        self.emit("addi t0, t0, 1");
        self.emit("addi t1, t1, 1");
        self.emit("addi t2, t2, -1");
        self.emit(format!("bnez t2, {key_loop}"));
        self.emit(format!("j {fail}"));
        self.emit_label(&ordered);
        self.emit("addi a6, a5, 20");
        self.emit("lbu t0, 20(a5)");
        self.emit(format!("li t1, {}", PolicyScriptRole::Type.as_byte()));
        self.emit(format!("bne t0, t1, {next_record}"));
        self.emit("addi t0, a5, 21");
        self.emit_sp_addi("t1", POLICY_HASH_BUFFER_OFFSET);
        self.emit("li t2, 32");
        self.emit_label(&hash_loop);
        self.emit("lbu t3, 0(t0)");
        self.emit("lbu t4, 0(t1)");
        self.emit(format!("bne t3, t4, {next_record}"));
        self.emit("addi t0, t0, 1");
        self.emit("addi t1, t1, 1");
        self.emit("addi t2, t2, -1");
        self.emit(format!("bnez t2, {hash_loop}"));
        self.emit_stack_load("t0", POLICY_FOUND_OFFSET);
        self.emit(format!("bnez t0, {fail}"));
        self.emit("li t0, 1");
        self.emit_stack_store("t0", POLICY_FOUND_OFFSET);
        self.emit_u32_le_from_base_to("t0", "a5", 53, "t4");
        self.emit_stack_store("t0", POLICY_TAG_OFFSET);
        self.emit_u32_le_from_base_to("t0", "a5", 57, "t4");
        self.emit_stack_store("t0", POLICY_ARGS_LENGTH_OFFSET);
        self.emit(format!("addi t0, a5, {POLICY_RECORD_FIXED_BYTES}"));
        self.emit_stack_store("t0", POLICY_ARGS_POINTER_OFFSET);
        self.emit_label(&next_record);
        self.emit(format!("bltu a2, a3, {record_loop}"));
        self.emit_stack_load("t0", POLICY_FOUND_OFFSET);
        self.emit(format!("beqz t0, {fail}"));
    }
}

fn policy_action<'a>(ir: &'a IrModule, name: &str) -> Result<&'a IrAction> {
    let mut matches = ir.items.iter().filter_map(|item| match item {
        IrItem::Action(action) if action.name == name => Some(action),
        _ => None,
    });
    let action = matches.next().ok_or_else(|| CompileError::without_span(format!("policy action '{name}' is unresolved")))?;
    if matches.next().is_some() || action.return_type.as_ref().is_some_and(|ty| *ty != IrType::Unit) {
        return Err(CompileError::without_span(format!("policy action '{name}' must resolve once with unit return type")));
    }
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_witness::{encode_policy_witness_bundle, PolicyWitnessRecord};

    #[test]
    fn policy_frame_and_record_offsets_match_host_contract() {
        assert_eq!(POLICY_ENTRY_FRAME_SIZE % 16, 0);
        assert_eq!(ENTRY_WITNESS_BUFFER_SIZE, MAX_POLICY_WITNESS_BYTES);
        let encoded = encode_policy_witness_bundle(&[PolicyWitnessRecord {
            role: PolicyScriptRole::Type,
            script_hash: [0x42; 32],
            tag: u32::MAX,
            args: Vec::new(),
        }])
        .unwrap();
        let record = &encoded[POLICY_WITNESS_MAGIC.len() + 8..];
        assert_eq!(record.len(), POLICY_RECORD_FIXED_BYTES);
        for (index, offset) in POLICY_RECORD_OFFSETS.iter().enumerate() {
            assert_eq!(u32::from_le_bytes(record[(index + 1) * 4..(index + 2) * 4].try_into().unwrap()) as usize, *offset);
        }
        assert_eq!(record[20], PolicyScriptRole::Type.as_byte());
        assert_eq!(&record[21..53], &[0x42; 32]);
        assert_eq!(u32::from_le_bytes(record[53..57].try_into().unwrap()), u32::MAX);
        assert_eq!(u32::from_le_bytes(record[57..61].try_into().unwrap()), 0);
    }
}
