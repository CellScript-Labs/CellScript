//! Controlled syscall execution complements the independent finite decoder.
//! It exercises the RFC0050 argument buffers and full-width child-id transport;
//! it does not model a child program or establish scheduler consensus behavior.
use super::*;
use ckb_vm::{
    machine::{DefaultCoreMachine, DefaultMachineBuilder, VERSION2},
    memory::{sparse::SparseMemory, wxorx::WXorXMemory},
    registers::{A0, A3, A4, A7, SP},
    Bytes, CoreMachine, Error, Memory, SupportMachine, Syscalls, ISA_B, ISA_IMC, ISA_MOP,
};

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;
type VmResult<T> = std::result::Result<T, Error>;
const FD: u64 = 0x0102_0304_0506_0708;
const CHILD: u64 = 0xfedc_ba98_7654_3210;

struct SpawnResponse {
    status: u64,
}

impl Syscalls<Core> for SpawnResponse {
    fn initialize(&mut self, _machine: &mut Core) -> VmResult<()> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Core) -> VmResult<bool> {
        if machine.registers()[A7] != 2601 {
            return Ok(false);
        }
        assert_eq!(&machine.registers()[A0..=A3], &[3, 3, 0, 0]);
        let arguments = machine.registers()[A4];
        assert_eq!(machine.memory_mut().load64(&arguments)?, 0);
        assert_eq!(machine.memory_mut().load64(&(arguments + 8))?, 0);
        let child = machine.memory_mut().load64(&(arguments + 16))?;
        let fds = machine.memory_mut().load64(&(arguments + 24))?;
        assert_eq!(machine.memory_mut().load64(&fds)?, FD);
        assert_eq!(machine.memory_mut().load64(&(fds + 8))?, 0);
        let ranges = [(child, 8), (fds, 16), (arguments, 32)];
        let stack = machine.registers()[SP];
        for (index, &(start, width)) in ranges.iter().enumerate() {
            assert!(stack <= start && start + width <= stack + 72);
            for &(other, other_width) in &ranges[index + 1..] {
                assert!(start + width <= other || other + other_width <= start);
            }
        }
        if self.status == 0 {
            machine.memory_mut().store64(&child, &CHILD)?;
        }
        machine.set_register(A0, self.status);
        Ok(true)
    }
}

fn execute(artifact: Vec<u8>, status: u64) -> i8 {
    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 1_000_000);
    let mut machine = DefaultMachineBuilder::new(core).syscall(Box::new(SpawnResponse { status })).build();
    machine.load_program(&Bytes::from(artifact), std::iter::empty::<VmResult<Bytes>>()).unwrap();
    machine.run().unwrap()
}

#[test]
fn spawn_syscall_buffers_separate_status_and_full_width_child_id() {
    for diagnostics in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
        for status in [0, 73] {
            let (mut assembly, _) = runtime_inventory(diagnostics);
            let entry = assembly.iter().position(|line| line.trim() == "ret").unwrap();
            let caller = format!(
                "li a0, 3\nli a1, {FD}\ncall __ckb_spawn_with_fd1\nli t0, {}\nbne a0, t0, .Lbad_spawn_result\n{}\nli a0, 0\nli a7, 93\necall\n.Lbad_spawn_result:\nli a0, 5\nli a7, 93\necall",
                u64::from(status != 0),
                if status == 0 { format!("li t0, {CHILD}\nbne a1, t0, .Lbad_spawn_result") } else { String::new() }
            );
            assembly.splice(entry..=entry, caller.lines().map(str::to_owned));
            let artifact = assembler::assemble_generated_elf(&assembly).unwrap();
            assert_eq!(execute(artifact.clone(), status), 0, "{diagnostics}: status {status}");
            if status == 0 {
                // Recompute the relocated body start after replacing the entry.
                let ast =
                    crate::frontend::parse_diagnostics("module stack_inventory;", crate::CellScriptEdition::Edition2027).unwrap();
                let ir = crate::ir::generate(&ast).unwrap();
                let layout = assembler::machine_layout_evidence(&assembly, &BTreeMap::new(), &ir).unwrap();
                let start = layout.symbols["__ckb_spawn_with_fd1"];
                let end = layout
                    .globals
                    .iter()
                    .filter_map(|name| layout.symbols.get(name))
                    .copied()
                    .filter(|address| *address > start)
                    .min()
                    .unwrap_or(layout.text_end);
                let elf = cellscript_artifact_checker::parse_elf(&artifact, 1_000_000).unwrap();
                let load = elf
                    .instructions
                    .iter()
                    .find(|instruction| (start..end).contains(&instruction.address) && instruction.word == 0x01813583)
                    .unwrap();
                let mut changed = artifact;
                let offset = (elf.text.offset + load.address - elf.text.address) as usize;
                let byte_load = (load.word & !(7 << 12)) | (4 << 12);
                changed[offset..offset + 4].copy_from_slice(&byte_load.to_le_bytes());
                assert_eq!(execute(changed, status), 5, "a truncated child id must change the observed result");
            }
        }
    }
}
