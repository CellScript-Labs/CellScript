use super::super::output::{read_json_value, CommandOutcome};
use super::analysis_reports::json_diff_report;
use super::{
    compile_cli_input, dependency_lock_json, deployment_plan_json, print_or_text_json, verify_deploy_plan_json, write_or_print_json,
    CommandExecutor, DeployPlanArgs, DiffDeployArgs, LockDepsArgs, VerifyDeployArgs,
};
use crate::error::Result;
use crate::CompileOptions;

impl CommandExecutor {
    pub(super) fn deploy_plan(args: DeployPlanArgs) -> Result<()> {
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
        let plan = deployment_plan_json(&result.metadata);
        write_or_print_json(args.output.as_ref(), &plan, args.json, "Deployment plan generated")?;
        Ok(())
    }

    pub(super) fn verify_deploy(args: VerifyDeployArgs) -> Result<()> {
        let plan = read_json_value(&args.plan)?;
        let violations = verify_deploy_plan_json(&plan);
        let summary = serde_json::json!({
            "status": if violations.is_empty() { "ok" } else { "failed" },
            "plan": args.plan.display().to_string(),
            "violations": violations,
        });
        let failed = summary["status"] == "failed";
        let outcome = CommandOutcome {
            human_lines: vec![format!("Deploy plan verification: {}", summary["status"].as_str().unwrap_or("unknown"))],
            machine: summary,
        };
        if failed {
            return Err(outcome.failure("deploy plan verification failed"));
        }
        outcome.emit(args.json)
    }

    pub(super) fn diff_deploy(args: DiffDeployArgs) -> Result<()> {
        let old = read_json_value(&args.old)?;
        let new = read_json_value(&args.new)?;
        let diff = json_diff_report("deploy", &old, &new);
        print_or_text_json(args.json, &diff, "Deploy diff")?;
        Ok(())
    }

    pub(super) fn lock_deps(args: LockDepsArgs) -> Result<()> {
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
        let lock = dependency_lock_json(&result.metadata);
        write_or_print_json(args.output.as_ref(), &lock, args.json, "Dependency lock generated")?;
        Ok(())
    }
}
