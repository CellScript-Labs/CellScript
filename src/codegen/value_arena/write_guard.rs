//! Bound dynamic collection writes to their owned frame region. These guards
//! establish noninterference with retained Cell views, not allocation semantics.
use super::super::*;

pub(in crate::codegen) struct WriteGuard {
    pub start: usize,
    pub end: usize,
    pub failure: String,
}

impl CodeGenerator {
    pub(in crate::codegen) fn emit_allocated_guarded_store(&mut self, instruction: &str, guard: &WriteGuard) -> bool {
        let Some((opcode, operands)) = instruction.split_once(' ') else { return false };
        let width = match opcode {
            "sb" => 1,
            "sh" => 2,
            "sw" => 4,
            "sd" => 8,
            _ => return false,
        };
        let Some((source, address)) = operands.split_once(',') else { return false };
        let source = source.trim();
        let Some((offset, base)) = memory_operand_offset_and_base(address.trim()) else { return false };
        let Ok(base_register) = parse_register(base) else { return false };
        let Ok(source_register) = parse_register(source) else { return false };
        if matches!(base_register, 2 | 8) || source_register == 2 {
            return false;
        }
        // Preserve every scratch register. Neither the original address nor
        // stored value may be used as guard scratch. The 32-byte temporary
        // subframe is aligned and contains no live caller arguments.
        let registers = ["a6", "a7", "t6", "t5", "t4"]
            .into_iter()
            .filter(|register| {
                let register = parse_register(register).expect("fixed scratch register");
                register != source_register && register != base_register
            })
            .take(3)
            .collect::<Vec<_>>();
        let (pointer, lower, upper) = (registers[0], registers[1], registers[2]);
        self.emit_stack_adjustment(-32);
        for (index, register) in registers.iter().enumerate() {
            self.emit_stack_store(register, index * 8);
        }
        if (-2048..=2047).contains(&offset) {
            self.emit(format!("addi {pointer}, {base}, {offset}"));
        } else {
            self.emit(format!("li {pointer}, {offset}"));
            self.emit(format!("add {pointer}, {base}, {pointer}"));
        }
        self.emit_sp_addi(lower, guard.start + 32);
        self.emit_sp_addi(upper, guard.end - width + 32);
        self.emit(format!("bltu {pointer}, {lower}, {}", guard.failure));
        self.emit(format!("bltu {upper}, {pointer}, {}", guard.failure));
        self.emit(format!("{opcode} {source}, 0({pointer})"));
        for (index, register) in registers.iter().enumerate() {
            self.emit_stack_load(register, index * 8);
        }
        self.emit_stack_adjustment(32);
        true
    }
}
