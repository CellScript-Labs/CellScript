//! Execute IPC scalar boundaries in the real scheduler and with controlled
//! syscall responses, including counts larger than the copied fd prefix.

use cellscript::{compile, CompileOptions, CompilerDiagnostics};
use ckb_testtool::ckb_types::bytes::Bytes;
use ckb_vm::{
    cost_model::estimate_cycles,
    machine::{trace::TraceMachine, DefaultCoreMachine, DefaultMachineBuilder, VERSION2},
    memory::{sparse::SparseMemory, wxorx::WXorXMemory},
    registers::{A0, A1, A2, A7},
    CoreMachine, DefaultMachineRunner, Error, Memory, SupportMachine, Syscalls, ISA_B, ISA_IMC, ISA_MOP,
};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;

fn diagnostics() -> [CompilerDiagnostics; 2] {
    [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()]
}

fn elf(body: &str, diagnostics: CompilerDiagnostics) -> Bytes {
    let source = format!("module ipc_runtime\naction verify() -> u64 {{ verification\n{body}\nreturn 0 }}");
    let compiled = compile(&source, CompileOptions { target: Some("riscv64-elf".into()), diagnostics, ..Default::default() }).unwrap();
    Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes))
}

#[derive(Clone, Copy)]
struct IpcResponse {
    status: u64,
    count: u64,
    payload: u64,
}

impl Syscalls<Core> for IpcResponse {
    fn initialize(&mut self, _machine: &mut Core) -> Result<(), Error> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Core) -> Result<bool, Error> {
        match machine.registers()[A7] {
            2606 => {
                let buffer = machine.registers()[A1];
                let length = machine.registers()[A2];
                assert_eq!(machine.memory_mut().load64(&length)?, 8);
                // Model payload delivery independently from status/length so
                // a rejected read cannot accidentally succeed on its contents.
                machine.memory_mut().store64(&buffer, &self.payload)?;
                machine.memory_mut().store64(&length, &self.count)?;
                machine.set_register(A0, self.status);
            }
            2607 => {
                let buffer = machine.registers()[A0];
                let length = machine.registers()[A1];
                let capacity = machine.memory_mut().load64(&length)?;
                // ckb-script copies min(capacity, actual count), then returns
                // the FULL count. Duplicate inherited fds can exceed capacity.
                for index in 0..capacity.min(self.count) {
                    machine.memory_mut().store64(&(buffer + index * 8), &(100 + index))?;
                }
                machine.memory_mut().store64(&length, &self.count)?;
                machine.set_register(A0, self.status);
            }
            2608 => machine.set_register(A0, 0),
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn modeled_exit(elf: &Bytes, response: IpcResponse) -> i8 {
    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 1_000_000);
    let mut machine = TraceMachine::new(
        DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).syscall(Box::new(response)).build(),
    );
    machine.load_program(elf, std::iter::empty::<Result<Bytes, Error>>()).unwrap();
    machine.run().unwrap()
}

#[test]
fn deferred_helpers_abort_even_when_their_results_are_ignored() {
    for diagnostics in diagnostics() {
        for (body, expected) in [
            ("require_maturity(100)", 86),
            ("require_time(1714000000)", 86),
            ("require_epoch_after(10, 0, 1)", 86),
            ("require_epoch_relative(10, 0, 1)", 86),
            ("occupied_capacity(\"Token\")", 87),
        ] {
            let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            let execution = ckb_script_runner::execute_cellscript_script(&elf(body, diagnostics), &fixture);
            assert_eq!(execution.exit_code, expected, "{diagnostics}: {body}");
        }
    }
}

#[test]
fn real_scheduler_errors_cannot_be_used_as_scalar_values_or_ignored() {
    for diagnostics in diagnostics() {
        for body in [
            "let value = pipe_read(999)\nrequire value == 1",
            "pipe_read(999)",
            "let fd = inherited_fd(0)\nrequire fd == 1\nclose(fd)",
        ] {
            let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            let execution = ckb_script_runner::execute_cellscript_script(&elf(body, diagnostics), &fixture);
            assert_eq!(execution.exit_code, 1, "{diagnostics}: {body}");
        }
    }
}

#[test]
fn pipe_read_preserves_every_payload_and_aborts_on_errors_or_partial_delivery() {
    for diagnostics in diagnostics() {
        for payload in [0, 1, u64::MAX] {
            let program = elf(&format!("let value = pipe_read(999)\nrequire value == {payload}"), diagnostics);
            assert_eq!(modeled_exit(&program, IpcResponse { status: 0, count: 8, payload }), 0);
            for count in [0, 1, 7, 9, u64::MAX] {
                assert_eq!(modeled_exit(&program, IpcResponse { status: 0, count, payload }), 1, "length {count}");
            }
            assert_eq!(modeled_exit(&program, IpcResponse { status: 6, count: 8, payload }), 1);
        }
    }
}

#[test]
fn inherited_fd_checks_both_reported_count_and_copied_capacity() {
    for diagnostics in diagnostics() {
        for (index, count, expected_exit) in [(0, 0, 1), (0, 1, 0), (9, 10, 0), (63, 64, 0), (64, 65, 1), (0, 65, 0), (10, 10, 1)] {
            let program = elf(&format!("let fd = inherited_fd({index})\nrequire fd == {}\nclose(fd)", 100 + index), diagnostics);
            assert_eq!(
                modeled_exit(&program, IpcResponse { status: 0, count, payload: 0 }),
                expected_exit,
                "index {index}, count {count}"
            );
            assert_eq!(modeled_exit(&program, IpcResponse { status: 6, count, payload: 0 }), 1);
        }
        // Before the capacity guard this index could address saved caller data
        // whenever the scheduler reported a list longer than the copied prefix.
        let program = elf("let fd = inherited_fd(18446744073709551615)\nclose(fd)", diagnostics);
        assert_eq!(modeled_exit(&program, IpcResponse { status: 0, count: u64::MAX, payload: 0 }), 1);
    }
}
