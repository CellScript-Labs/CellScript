use super::super::output::{read_json_value, CommandOutcome};
use super::transaction_solver::transaction_solver_template;
use super::{compile_cli_input, read_metadata_json, write_or_print_json, CommandExecutor, SolveTxArgs, ValidateTxArgs};
use crate::error::Result;
use crate::CompileOptions;

impl CommandExecutor {
    pub(super) fn validate_tx(args: ValidateTxArgs) -> Result<()> {
        let metadata = read_metadata_json(&args.against)?;
        let tx = read_json_value(&args.tx)?;
        let report = crate::assumptions::validate_transaction_against_metadata(&metadata, &tx);
        let summary = serde_json::json!({
            "status": report.status,
            "validation_level": "cellscript-metadata-evidence",
            "ckb_vm_execution": false,
            "tx_pool_acceptance": false,
            "metadata": args.against.display().to_string(),
            "tx": args.tx.display().to_string(),
            "validation": report,
        });
        let failed = summary["status"] == "failed";
        let mut human_lines = vec![format!("Transaction validation: {}", report.status)];
        if !report.unverified_builder_obligations.is_empty() {
            human_lines.push(format!(
                "  Builder policy obligations remain unverified: {} (fee/change/signature/failure handling)",
                report.unverified_builder_obligations.len()
            ));
        }
        let outcome = CommandOutcome { machine: summary, human_lines };
        if failed {
            return Err(outcome.failure("transaction violates builder assumptions"));
        }
        outcome.emit(args.json)
    }

    pub(super) fn solve_tx(args: SolveTxArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
            CompileOptions {
                source_contracts: false,
                edition: crate::CURRENT_EDITION,
                diagnostics: crate::CompilerDiagnostics::default(),
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let template = transaction_solver_template(&result.metadata);
        write_or_print_json(args.output.as_ref(), &template, args.json, "Transaction template generated (tx solve is not a solver)")?;
        Ok(())
    }
}
