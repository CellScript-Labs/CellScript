//! Diagnostic instruction stepping with the real CKB scheduler and syscalls.
//! The ordinary group verifier remains the oracle. Reports are usable only
//! when its exit and cycle total exactly match this separate replay.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use ckb_testtool::{
    ckb_chain_spec::consensus::ConsensusBuilder,
    ckb_script::{
        generate_ckb_syscalls,
        types::{DebugPrinter, SgData, VmContext, VmId},
        ScriptGroupType, TransactionScriptsVerifier, TxVerifyEnv,
    },
    ckb_types::{
        core::{
            hardfork::{HardForks, CKB2021, CKB2023},
            HeaderBuilder, TransactionView,
        },
        packed,
    },
    context::Context,
};
use ckb_vm::{
    decoder::build_decoder,
    elf::ProgramMetadata,
    instructions::{extract_opcode, instruction_opcode_name},
    machine::DefaultMachine,
    registers::{A7, SP},
    Bytes, CoreMachine, DefaultCoreMachine, DefaultMachineRunner, Error, SparseMemory, SupportMachine, Syscalls, WXorXMemory,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::cost_measurement::{measure_group, resolve_transaction, CycleObservation};

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;
type Shared = Arc<Mutex<BTreeMap<(VmId, u64), VmTrace>>>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PcCost {
    pub attempts: u64,
    pub instruction_cycles: u64,
    pub synchronous_syscall_cycles: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmTrace {
    pub vm_id: VmId,
    pub generation: u64,
    pub program_sha256: String,
    pub vm_version: u32,
    pub isa: u8,
    pub entry_sp: u64,
    pub minimum_sp: u64,
    pub observed_stack_bytes: u64,
    pub instructions: BTreeMap<String, u64>,
    pub syscalls: BTreeMap<u64, u64>,
    pub pcs: BTreeMap<u64, PcCost>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTrace {
    pub schema: String,
    pub scope: String,
    pub stack_baseline: String,
    pub status: String,
    pub unavailable_reason: Option<String>,
    pub exit_code: Option<i8>,
    pub authoritative_cycles: Option<u64>,
    pub stepped_cycles: Option<u64>,
    pub scheduler_and_loading_cycles: Option<u64>,
    pub vms: Vec<VmTrace>,
}

// The pinned scheduler calls its syscall generator immediately before
// M::new, on the same thread. This bridge associates the runner with that
// scheduler's VM ID without changing the production VM or syscall code.
// Taking the slot prevents stale association; test threads have isolated slots.
thread_local! {
    static NEXT_VM: RefCell<Option<(VmId, Shared)>> = const { RefCell::new(None) };
}

fn syscalls(id: &VmId, data: &SgData<Context>, context: &VmContext<Context>, traces: &Shared) -> Vec<Box<dyn Syscalls<Core>>> {
    NEXT_VM.with(|next| {
        assert!(next.borrow().is_none(), "scheduler must consume each runner association");
        *next.borrow_mut() = Some((*id, traces.clone()));
    });
    let printer: DebugPrinter = Arc::new(|_, _| {});
    generate_ckb_syscalls(id, data, context, &printer)
}

struct ObservedMachine {
    machine: DefaultMachine<Core>,
    id: VmId,
    traces: Shared,
    generation: u64,
    loaded: bool,
    program_sha256: String,
}

impl DefaultMachineRunner for ObservedMachine {
    type Inner = Core;

    fn new(machine: DefaultMachine<Core>) -> Self {
        let (id, traces) = NEXT_VM.with(|next| next.borrow_mut().take()).expect("scheduler runner association");
        // A restored VM resumes the current generation. A subsequent ELF
        // load (EXEC) starts a new generation under the same process ID.
        let generation =
            traces.lock().unwrap().keys().filter(|(vm, _)| *vm == id).map(|(_, generation)| *generation).max().unwrap_or(0);
        let program_sha256 =
            traces.lock().unwrap().get(&(id, generation)).map(|trace| trace.program_sha256.clone()).unwrap_or_default();
        Self { machine, id, traces, generation, loaded: false, program_sha256 }
    }

    fn machine(&self) -> &DefaultMachine<Core> {
        &self.machine
    }
    fn machine_mut(&mut self) -> &mut DefaultMachine<Core> {
        &mut self.machine
    }

    fn load_program_with_metadata(
        &mut self,
        program: &Bytes,
        metadata: &ProgramMetadata,
        args: impl ExactSizeIterator<Item = Result<Bytes, Error>>,
    ) -> Result<u64, Error> {
        let bytes = self.machine.load_program_with_metadata(program, metadata, args)?;
        if self.loaded || self.traces.lock().unwrap().contains_key(&(self.id, self.generation)) {
            self.generation += 1;
        }
        self.loaded = true;
        self.program_sha256 = hex::encode(Sha256::digest(program));
        Ok(bytes)
    }

    fn run(&mut self) -> Result<i8, Error> {
        let mut decoder = build_decoder::<u64>(self.machine.isa(), self.machine.version());
        self.machine.set_running(true);
        while self.machine.running() {
            if self.machine.pause().has_interrupted() {
                self.machine.pause().free();
                return Err(Error::Pause);
            }
            if self.machine.reset_signal() {
                decoder.reset_instructions_cache();
            }
            let pc = *self.machine.pc();
            let before_sp = self.machine.registers()[SP];
            let syscall = self.machine.registers()[A7];
            let instruction = decoder.decode(self.machine.memory_mut(), pc)?;
            let name = instruction_opcode_name(extract_opcode(instruction));
            let charged = (self.machine.instruction_cycle_func())(instruction);
            let before_cycles = self.machine.cycles();
            let result = self.machine.step(&mut decoder);
            let delta = self
                .machine
                .cycles()
                .checked_sub(before_cycles)
                .ok_or_else(|| Error::Unexpected("trace cycle counter moved backwards".into()))?;
            let after_sp = self.machine.registers()[SP];
            let mut traces = self.traces.lock().unwrap();
            let trace = traces.entry((self.id, self.generation)).or_insert_with(|| VmTrace {
                vm_id: self.id,
                generation: self.generation,
                program_sha256: self.program_sha256.clone(),
                vm_version: self.machine.version(),
                isa: self.machine.isa(),
                entry_sp: before_sp,
                minimum_sp: before_sp,
                observed_stack_bytes: 0,
                instructions: BTreeMap::new(),
                syscalls: BTreeMap::new(),
                pcs: BTreeMap::new(),
            });
            // Decode coverage is deliberately bounded; diagnostics must not
            // grow without limit on an adversarial dynamically loaded program.
            if trace.pcs.len() >= 1_000_000 && !trace.pcs.contains_key(&pc) {
                return Err(Error::Unexpected("trace PC budget exceeded".into()));
            }
            trace.minimum_sp = trace.minimum_sp.min(before_sp).min(after_sp);
            trace.observed_stack_bytes = trace.entry_sp - trace.minimum_sp;
            *trace.instructions.entry(name.into()).or_default() += 1;
            if name == "ECALL" {
                *trace.syscalls.entry(syscall).or_default() += 1;
            }
            let cost = trace.pcs.entry(pc).or_default();
            cost.attempts += 1;
            // A cycle-limit failure may reject before charging/executing the
            // instruction. Partial traces remain diagnostic and unavailable.
            cost.instruction_cycles += charged.min(delta);
            cost.synchronous_syscall_cycles += delta.saturating_sub(charged);
            drop(traces);
            result?;
        }
        Ok(self.machine.exit_code())
    }
}

pub fn measure(
    context: &Context,
    tx: &TransactionView,
    script: &packed::Script,
    role: ScriptGroupType,
    max_cycles: u64,
) -> ExecutionTrace {
    let mut report = ExecutionTrace {
        schema: "cellscript-execution-trace-v1".into(),
        scope: "script-group-scheduler-separate-replay".into(),
        stack_baseline: "entry-sp-after-argv-before-first-instruction-per-vm-generation".into(),
        status: "unavailable".into(),
        unavailable_reason: None,
        exit_code: None,
        authoritative_cycles: None,
        stepped_cycles: None,
        scheduler_and_loading_cycles: None,
        vms: Vec::new(),
    };
    let oracle = measure_group(context, tx, script, role, max_cycles);
    let CycleObservation::Measured { cycles, exit_code } = oracle.observation else {
        report.unavailable_reason = Some(format!("authoritative group outcome unavailable: {:?}", oracle.observation));
        return report;
    };
    report.authoritative_cycles = Some(cycles);
    report.exit_code = Some(exit_code);
    let resolved = match resolve_transaction(context, tx) {
        Ok(resolved) => resolved,
        Err(reason) => {
            report.unavailable_reason = Some(reason);
            return report;
        }
    };
    let consensus = ConsensusBuilder::default()
        .hardfork_switch(HardForks { ckb2021: CKB2021::new_dev_default(), ckb2023: CKB2023::new_dev_default() })
        .build();
    let tip = HeaderBuilder::default().number(0).build();
    let traces = Shared::default();
    let verifier = TransactionScriptsVerifier::<_, _, ObservedMachine>::new_with_generator(
        Arc::new(resolved),
        context.clone(),
        Arc::new(consensus),
        Arc::new(TxVerifyEnv::new_submit(&tip)),
        syscalls,
        traces.clone(),
    );
    let group = verifier.find_script_group(role, &script.calc_script_hash()).expect("oracle found group");
    let result = verifier.detailed_run(group, max_cycles);
    report.vms = traces.lock().unwrap().values().cloned().collect();
    match result {
        Ok(result) if result.exit_code == exit_code && result.consumed_cycles == cycles => {
            let attributed: u64 =
                report.vms.iter().flat_map(|vm| vm.pcs.values()).map(|pc| pc.instruction_cycles + pc.synchronous_syscall_cycles).sum();
            report.stepped_cycles = Some(result.consumed_cycles);
            report.scheduler_and_loading_cycles = cycles.checked_sub(attributed);
            if report.scheduler_and_loading_cycles.is_some() {
                report.status = "measured".into();
            } else {
                report.unavailable_reason = Some("attributed cycles exceed scheduler total".into());
            }
        }
        Ok(result) => {
            report.unavailable_reason = Some(format!(
                "diagnostic replay changed verdict/cycles: {}/{}, oracle {exit_code}/{cycles}",
                result.exit_code, result.consumed_cycles
            ))
        }
        Err(error) => report.unavailable_reason = Some(format!("diagnostic replay failed: {error}")),
    }
    report
}
