//! Static instruction counts in validated ELF text, including unreachable code.
//! These counts do not measure executed memory traffic or syscall memory access.

use cellscript_artifact_checker::{parse_elf, CheckerBudgets};
use serde_json::{json, Value};

pub fn measure(elf: &[u8]) -> Value {
    let parsed = parse_elf(elf, CheckerBudgets::default().instructions).expect("decode cost ELF text");
    let loads = parsed.instructions.iter().filter(|instruction| instruction.word & 0x7f == 0x03).count();
    let stores = parsed.instructions.iter().filter(|instruction| instruction.word & 0x7f == 0x23).count();
    json!({
        "schema": "cellscript-static-memory-counts-v1",
        "scope": "decoded_elf_text_including_unreachable",
        "instruction_count": parsed.instructions.len(),
        "load_instruction_count": loads,
        "store_instruction_count": stores,
    })
}
