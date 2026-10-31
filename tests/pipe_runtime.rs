//! The PIPE helper's status and two outputs are three different ABI words.
//! Controlled syscall responses exercise successful endpoints and hostile status
//! values independently; the transaction scheduler integration is separate.
use cellscript::{compile, CompileOptions, CompilerDiagnostics};
use ckb_testtool::ckb_types::bytes::Bytes;
use ckb_vm::{
    cost_model::estimate_cycles,
    machine::{trace::TraceMachine, DefaultCoreMachine, DefaultMachineBuilder, VERSION2},
    memory::{sparse::SparseMemory, wxorx::WXorXMemory},
    registers::{A0, A7},
    CoreMachine, DefaultMachineRunner, Error, Memory, SupportMachine, Syscalls, ISA_B, ISA_IMC, ISA_MOP,
};
type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;

#[derive(Clone, Copy)]
struct PipeResponse {
    status: u64,
    read: u64,
    write: u64,
}
impl Syscalls<Core> for PipeResponse {
    fn initialize(&mut self, _machine: &mut Core) -> Result<(), Error> {
        Ok(())
    }
    fn ecall(&mut self, machine: &mut Core) -> Result<bool, Error> {
        match machine.registers()[A7] {
            2604 => {
                let output = machine.registers()[A0];
                // Even a failed syscall populates plausible values. A caller
                // must reject the status before admitting either endpoint.
                machine.memory_mut().store64(&output, &self.read)?;
                machine.memory_mut().store64(&(output + 8), &self.write)?;
                machine.set_register(A0, self.status);
            }
            2608 => machine.set_register(A0, 0),
            _ => return Ok(false),
        }
        Ok(true)
    }
}
fn execute(source: &str, diagnostics: CompilerDiagnostics, response: PipeResponse) -> i8 {
    let result = compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
    let program = Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&result.artifact_bytes));
    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 1_000_000);
    let mut machine = TraceMachine::new(
        DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).syscall(Box::new(response)).build(),
    );
    machine.load_program(&program, std::iter::empty::<Result<Bytes, Error>>()).unwrap();
    machine.run().unwrap()
}
#[test]
fn pipe_consumes_status_before_admitting_both_full_width_endpoints() {
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        for (read, write) in [(17, 29), (0, u64::MAX), (u64::MAX - 1, 0)] {
            let source = format!("module pipe_abi\naction verify() -> u64 {{ verification let ends = pipe() require ends.0 == {read} require ends.1 == {write} close(ends.0) close(ends.1) return 0 }}");
            assert_eq!(execute(&source, diagnostics, PipeResponse { status: 0, read, write }), 0);
            for status in [1, 6, u64::MAX] {
                assert_eq!(execute(&source, diagnostics, PipeResponse { status, read, write }), 1);
            }
        }
        let ignored = "module pipe_abi\naction verify() -> u64 { verification pipe() return 0 }";
        assert_eq!(execute(ignored, diagnostics, PipeResponse { status: 6, read: 17, write: 29 }), 1);
    }
}
