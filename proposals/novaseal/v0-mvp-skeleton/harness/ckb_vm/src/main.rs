use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ckb_vm::{
    Bytes, DefaultCoreMachine, DefaultMachineBuilder, ISA_B, ISA_IMC, ISA_MOP, SparseMemory, SupportMachine, Syscalls, TraceMachine,
    WXorXMemory,
    cost_model::estimate_cycles,
    machine::VERSION2,
    memory::Memory,
    registers::{A0, A1, A2, A7},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFAULT_ELF: &str = "target/novaseal-btc-verifier-riscv-shell-release.elf";
const DEFAULT_VECTORS: &str = "target/novaseal-btc-verifier-ipc-vectors.json";
const DEFAULT_OUTPUT: &str = "target/novaseal-ckb-vm-child-verifier-report.json";
const MAX_CYCLES_DEFAULT: u64 = 100_000_000;
const IPC_WORD_COUNT: usize = 18;
const CHILD_INPUT_FD: u64 = 100;
const CKB_VM2_PIPE_READ_SYSCALL_NUMBER: u64 = 2606;
const CKB_VM2_INHERITED_FD_SYSCALL_NUMBER: u64 = 2607;
const CKB_VM2_CLOSE_SYSCALL_NUMBER: u64 = 2608;

type HarnessMachine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;

#[derive(Debug, Error)]
enum HarnessError {
    #[error("usage: novaseal_ckb_vm_harness [--elf PATH] [--vectors PATH] [--output PATH] [--max-cycles N] [--pretty]")]
    Usage,
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
}

#[derive(Debug)]
struct Args {
    elf: PathBuf,
    vectors: PathBuf,
    output: PathBuf,
    max_cycles: u64,
    pretty: bool,
}

#[derive(Debug, Deserialize)]
struct IpcVectorReport {
    vectors: Vec<IpcVector>,
    malformed: Vec<IpcVector>,
}

#[derive(Debug, Deserialize)]
struct IpcVector {
    id: String,
    expected: String,
    ipc_blob: String,
    ipc_blob_len: usize,
    #[serde(default)]
    source_case: Option<String>,
    #[serde(default)]
    mutation: Option<String>,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    classification: &'static str,
    elf: ElfReport,
    vectors: VectorInputReport,
    summary: Summary,
    cases: Vec<CaseReport>,
    limits: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct ElfReport {
    path: String,
    size_bytes: usize,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct VectorInputReport {
    path: String,
    total_cases: usize,
}

#[derive(Debug, Serialize)]
struct Summary {
    child_verifier_ckb_vm_executed: bool,
    parent_lock_spawn_executed: bool,
    total_cases: usize,
    expected_accept: usize,
    expected_reject: usize,
    accepted: usize,
    rejected: usize,
    matched_expected: usize,
    mismatched: usize,
    malformed_word_streams: usize,
    min_cycles: u64,
    max_cycles: u64,
    total_cycles: u64,
    inherited_fd_calls: usize,
    pipe_read_calls: usize,
    close_calls: usize,
}

#[derive(Debug, Serialize)]
struct CaseReport {
    id: String,
    expected: String,
    source_case: Option<String>,
    mutation: Option<String>,
    ipc_blob_len: usize,
    word_count: usize,
    partial_tail_bytes: usize,
    exit_code: i8,
    cycles: u64,
    accepted: bool,
    matched_expected: bool,
    syscall_trace: SyscallTrace,
}

#[derive(Clone, Debug, Default, Serialize)]
struct SyscallTrace {
    inherited_fd_calls: usize,
    pipe_read_calls: usize,
    close_calls: usize,
    pipe_read_failures: usize,
    inherited_fd_failures: usize,
    close_failures: usize,
    closed: bool,
}

struct SpawnInputSyscalls {
    fd: u64,
    words: Vec<u64>,
    cursor: usize,
    trace: Arc<Mutex<SyscallTrace>>,
}

impl<Mac: SupportMachine<REG = u64>> Syscalls<Mac> for SpawnInputSyscalls {
    fn initialize(&mut self, _machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Mac) -> Result<bool, ckb_vm::Error> {
        match machine.registers()[A7] {
            CKB_VM2_INHERITED_FD_SYSCALL_NUMBER => {
                self.inherited_fd(machine)?;
                Ok(true)
            }
            CKB_VM2_PIPE_READ_SYSCALL_NUMBER => {
                self.pipe_read(machine)?;
                Ok(true)
            }
            CKB_VM2_CLOSE_SYSCALL_NUMBER => {
                self.close(machine);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl SpawnInputSyscalls {
    fn inherited_fd<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        let buffer = machine.registers()[A0];
        let length_ptr = machine.registers()[A1];
        let length = machine.memory_mut().load64(&length_ptr)?;
        let mut trace = self.trace.lock().expect("trace mutex poisoned");
        trace.inherited_fd_calls += 1;
        if length >= 1 {
            machine.memory_mut().store64(&buffer, &self.fd)?;
            machine.memory_mut().store64(&length_ptr, &1)?;
            machine.set_register(A0, 0);
        } else {
            trace.inherited_fd_failures += 1;
            machine.memory_mut().store64(&length_ptr, &1)?;
            machine.set_register(A0, 1);
        }
        Ok(())
    }

    fn pipe_read<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) -> Result<(), ckb_vm::Error> {
        let fd = machine.registers()[A0];
        let buffer = machine.registers()[A1];
        let length_ptr = machine.registers()[A2];
        let length = machine.memory_mut().load64(&length_ptr)?;
        let mut trace = self.trace.lock().expect("trace mutex poisoned");
        trace.pipe_read_calls += 1;
        if fd != self.fd || trace.closed || self.cursor >= self.words.len() || length < 8 {
            trace.pipe_read_failures += 1;
            machine.set_register(A0, 1);
            return Ok(());
        }
        let word = self.words[self.cursor];
        self.cursor += 1;
        machine.memory_mut().store64(&buffer, &word)?;
        machine.memory_mut().store64(&length_ptr, &8)?;
        machine.set_register(A0, 0);
        Ok(())
    }

    fn close<Mac: SupportMachine<REG = u64>>(&mut self, machine: &mut Mac) {
        let fd = machine.registers()[A0];
        let mut trace = self.trace.lock().expect("trace mutex poisoned");
        trace.close_calls += 1;
        if fd == self.fd && !trace.closed {
            trace.closed = true;
            machine.set_register(A0, 0);
        } else {
            trace.close_failures += 1;
            machine.set_register(A0, 1);
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), HarnessError> {
    let args = parse_args()?;
    let elf = fs::read(&args.elf)?;
    let vector_report: IpcVectorReport = serde_json::from_slice(&fs::read(&args.vectors)?)?;
    let cases = vector_report.vectors.into_iter().chain(vector_report.malformed).collect::<Vec<_>>();
    if cases.is_empty() {
        return Err(HarnessError::Message("IPC vector report contains no cases".to_string()));
    }

    let mut case_reports = Vec::with_capacity(cases.len());
    for case in cases {
        case_reports.push(run_case(&elf, case, args.max_cycles)?);
    }
    let report = build_report(&args, &elf, case_reports);
    write_report(&args.output, &report, args.pretty)?;
    print_summary(&args.output, &report);

    if report.summary.mismatched == 0 {
        Ok(())
    } else {
        Err(HarnessError::Message(format!("{} CKB VM child verifier case(s) mismatched", report.summary.mismatched)))
    }
}

fn parse_args() -> Result<Args, HarnessError> {
    let mut elf = PathBuf::from(DEFAULT_ELF);
    let mut vectors = PathBuf::from(DEFAULT_VECTORS);
    let mut output = PathBuf::from(DEFAULT_OUTPUT);
    let mut max_cycles = MAX_CYCLES_DEFAULT;
    let mut pretty = false;

    let mut raw = env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--elf" => elf = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--vectors" => vectors = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--output" => output = raw.next().map(PathBuf::from).ok_or(HarnessError::Usage)?,
            "--max-cycles" => {
                let value = raw.next().ok_or(HarnessError::Usage)?;
                max_cycles = value.parse().map_err(|_| HarnessError::Message(format!("invalid --max-cycles value: {value}")))?;
            }
            "--pretty" => pretty = true,
            "-h" | "--help" => return Err(HarnessError::Usage),
            _ => return Err(HarnessError::Message(format!("unknown argument: {arg}"))),
        }
    }

    Ok(Args { elf, vectors, output, max_cycles, pretty })
}

fn run_case(elf: &[u8], case: IpcVector, max_cycles: u64) -> Result<CaseReport, HarnessError> {
    let blob = decode_hex_blob(&case.ipc_blob)?;
    let (words, partial_tail_bytes) = words_from_blob(&blob);
    let trace = Arc::new(Mutex::new(SyscallTrace::default()));
    let (exit_code, cycles) = run_child_elf(elf, &words, Arc::clone(&trace), max_cycles)?;
    let trace = trace.lock().expect("trace mutex poisoned").clone();
    let accepted = exit_code == 0;
    let matched_expected = match case.expected.as_str() {
        "accept" => accepted,
        "reject" => !accepted,
        other => return Err(HarnessError::Message(format!("case {} has unsupported expected value: {other}", case.id))),
    };

    Ok(CaseReport {
        id: case.id,
        expected: case.expected,
        source_case: case.source_case,
        mutation: case.mutation,
        ipc_blob_len: case.ipc_blob_len,
        word_count: words.len(),
        partial_tail_bytes,
        exit_code,
        cycles,
        accepted,
        matched_expected,
        syscall_trace: trace,
    })
}

fn run_child_elf(elf: &[u8], words: &[u64], trace: Arc<Mutex<SyscallTrace>>, max_cycles: u64) -> Result<(i8, u64), HarnessError> {
    let core_machine = DefaultCoreMachine::<u64, WXorXMemory<SparseMemory<u64>>>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, max_cycles);
    let syscall = SpawnInputSyscalls { fd: CHILD_INPUT_FD, words: words.to_vec(), cursor: 0, trace };
    let builder =
        DefaultMachineBuilder::new(core_machine).instruction_cycle_func(Box::new(estimate_cycles)).syscall(Box::new(syscall));
    let mut machine = HarnessMachine::new(builder.build());
    machine
        .load_program(&Bytes::copy_from_slice(elf), &[])
        .map_err(|error| HarnessError::Message(format!("failed to load verifier ELF in ckb-vm: {error}")))?;
    let exit_code = machine.run().map_err(|error| HarnessError::Message(format!("verifier ELF ckb-vm execution failed: {error}")))?;
    Ok((exit_code, machine.machine.cycles()))
}

fn decode_hex_blob(value: &str) -> Result<Vec<u8>, HarnessError> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    Ok(hex::decode(trimmed)?)
}

fn words_from_blob(blob: &[u8]) -> (Vec<u64>, usize) {
    let mut words = Vec::with_capacity(IPC_WORD_COUNT);
    let mut chunks = blob.chunks_exact(core::mem::size_of::<u64>());
    for chunk in &mut chunks {
        let mut bytes = [0u8; core::mem::size_of::<u64>()];
        bytes.copy_from_slice(chunk);
        words.push(u64::from_le_bytes(bytes));
    }
    (words, chunks.remainder().len())
}

fn build_report(args: &Args, elf: &[u8], cases: Vec<CaseReport>) -> Report {
    let total_cases = cases.len();
    let expected_accept = cases.iter().filter(|case| case.expected == "accept").count();
    let expected_reject = cases.iter().filter(|case| case.expected == "reject").count();
    let accepted = cases.iter().filter(|case| case.accepted).count();
    let rejected = total_cases - accepted;
    let matched_expected = cases.iter().filter(|case| case.matched_expected).count();
    let mismatched = total_cases - matched_expected;
    let malformed_word_streams = cases.iter().filter(|case| case.word_count != IPC_WORD_COUNT || case.partial_tail_bytes != 0).count();
    let min_cycles = cases.iter().map(|case| case.cycles).min().unwrap_or_default();
    let max_cycles = cases.iter().map(|case| case.cycles).max().unwrap_or_default();
    let total_cycles = cases.iter().map(|case| case.cycles).sum();
    let inherited_fd_calls = cases.iter().map(|case| case.syscall_trace.inherited_fd_calls).sum();
    let pipe_read_calls = cases.iter().map(|case| case.syscall_trace.pipe_read_calls).sum();
    let close_calls = cases.iter().map(|case| case.syscall_trace.close_calls).sum();

    Report {
        schema: "novaseal-ckb-vm-child-verifier-report-v0.1",
        classification: "child_verifier_ckb_vm_dry_run_evidence",
        elf: ElfReport { path: args.elf.display().to_string(), size_bytes: elf.len(), sha256: sha256_hex(elf) },
        vectors: VectorInputReport { path: args.vectors.display().to_string(), total_cases },
        summary: Summary {
            child_verifier_ckb_vm_executed: true,
            parent_lock_spawn_executed: false,
            total_cases,
            expected_accept,
            expected_reject,
            accepted,
            rejected,
            matched_expected,
            mismatched,
            malformed_word_streams,
            min_cycles,
            max_cycles,
            total_cycles,
            inherited_fd_calls,
            pipe_read_calls,
            close_calls,
        },
        cases,
        limits: vec![
            "Executes the staged child verifier ELF in ckb-vm with harness-provided inherited_fd, pipe_read, and close syscalls.",
            "Does not execute the parent CellScript lock.",
            "Does not prove CKB VM spawn syscall 2601, wait syscall 2602, or parent-observed child status.",
            "Does not construct a full CKB transaction, ScriptGroup, cell_deps, witnesses, capacity, or tx-size evidence.",
        ],
    }
}

fn write_report(path: &Path, report: &Report, pretty: bool) -> Result<(), HarnessError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = if pretty { serde_json::to_string_pretty(report)? } else { serde_json::to_string(report)? };
    fs::write(path, format!("{json}\n"))?;
    Ok(())
}

fn print_summary(path: &Path, report: &Report) {
    println!("wrote {}", path.display());
    println!(
        "summary: child_vm_executed={} parent_spawn_executed={} total={} accepted={} rejected={} matched_expected={} mismatched={} max_cycles={}",
        report.summary.child_verifier_ckb_vm_executed,
        report.summary.parent_lock_spawn_executed,
        report.summary.total_cases,
        report.summary.accepted,
        report.summary.rejected,
        report.summary.matched_expected,
        report.summary.mismatched,
        report.summary.max_cycles
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
