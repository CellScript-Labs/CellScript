#[cfg(feature = "vm-runner")]
use super::{run_elf_in_ckb_vm, run_entry_outcome};
use super::{CommandExecutor, RunArgs, RunEntryOutcome, RunOutcome};
use crate::error::Result;
use crate::{compile_path, CompileOptions};

impl CommandExecutor {
    pub(super) fn run(args: RunArgs) -> Result<()> {
        let diagnostics = crate::CompilerDiagnostics::default();
        let compile_result = compile_path(
            ".",
            CompileOptions {
                source_contracts: false,
                edition: crate::CURRENT_EDITION,
                diagnostics,
                output: None,
                debug: false,
                target: Some("riscv64-elf".to_string()),
                target_profile: None,
                primitive_compat: None,
            },
        );

        if args.simulate {
            let result = compile_result?;
            return Self::run_simulate(&result, &args);
        }

        #[cfg(feature = "vm-runner")]
        {
            let result = compile_result?;

            let parameterized_entries = result
                .metadata
                .actions
                .iter()
                .filter(|action| !action.params.is_empty())
                .map(|action| format!("action {}", action.name))
                .chain(result.metadata.locks.iter().filter(|lock| !lock.params.is_empty()).map(|lock| format!("lock {}", lock.name)))
                .collect::<Vec<_>>();
            if !parameterized_entries.is_empty() {
                return Err(crate::error::CompileError::without_span(format!(
                    "cellc run executes only no-argument pure ELF entrypoints; {} requires transaction/parameter ABI context; use --simulate explicitly for development interpretation",
                    parameterized_entries.join(", ")
                )));
            }

            if result.metadata.runtime.ckb_runtime_required {
                return Err(crate::error::CompileError::without_span(format!(
                    "cellc run cannot provide CKB transaction/syscall context required by {}; use --simulate explicitly or an executable transaction scenario",
                    result.metadata.runtime.ckb_runtime_features.join(", ")
                )));
            }

            if !result.metadata.runtime.standalone_runner_compatible {
                return Err(crate::error::CompileError::without_span(
                    "cellc run requires a standalone-compatible pure ELF; use --simulate explicitly or an executable transaction scenario",
                ));
            }

            let vm_args = args.args.into_iter().map(|arg| arg.into_bytes()).collect::<Vec<_>>();
            let cycles = run_elf_in_ckb_vm(&result.artifact_bytes, &vm_args)?;
            RunOutcome {
                status: "ok",
                mode: "ckb-vm",
                artifact_format: result.artifact_format.display_name().to_string(),
                entry: run_entry_outcome(&result.metadata),
                cycles: Some(cycles),
                steps: None,
                has_cell_operations: None,
                result: None,
                trace: Vec::new(),
            }
            .emit(args.json)
        }

        #[cfg(not(feature = "vm-runner"))]
        {
            let mode = if args.release { "release" } else { "debug" };
            Self::experimental_command(
                "run",
                &format!(
                    "feature-gated VM backend is not enabled (requested {}, {} argument(s)); use --simulate for AST-level simulation or compile with --features vm-runner to execute",
                    mode,
                    args.args.len()
                ),
            )
        }
    }

    fn run_simulate(compile_result: &crate::CompileResult, args: &RunArgs) -> Result<()> {
        use crate::simulate::{SimValue, SimulateInterpreter};

        let modules = crate::load_modules_for_input(".")?;
        let module =
            modules.iter().find(|module| module.ast.name == compile_result.metadata.module).map(|module| &module.ast).ok_or_else(
                || {
                    crate::error::CompileError::without_span(format!(
                        "failed to load module '{}' for simulation",
                        compile_result.metadata.module
                    ))
                },
            )?;

        let entry = compile_result
            .metadata
            .actions
            .iter()
            .find(|a| a.name == "main")
            .or_else(|| compile_result.metadata.actions.iter().find(|a| a.params.is_empty()));

        let Some(entry) = entry else {
            return Err(crate::error::CompileError::without_span(
                "no suitable entry point found for simulation; define an action main() or a zero-argument action",
            ));
        };

        let mut interp = SimulateInterpreter::new(module, 100_000);
        let sim_args: Vec<SimValue> = Vec::new();
        let sim_result = interp
            .simulate_action(&entry.name, &sim_args)
            .map_err(|e| crate::error::CompileError::without_span(format!("simulation error: {}", e)))?;

        RunOutcome {
            status: "ok",
            mode: "simulate",
            artifact_format: compile_result.artifact_format.display_name().to_string(),
            entry: Some(RunEntryOutcome { kind: "action".to_string(), name: sim_result.entry_name }),
            cycles: None,
            steps: Some(sim_result.steps),
            has_cell_operations: Some(sim_result.has_cell_ops),
            result: Some(sim_result.return_value.to_string()),
            trace: sim_result.trace.iter().map(ToString::to_string).collect(),
        }
        .emit(args.json)
    }
}
