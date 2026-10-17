use super::*;

pub(super) fn plans(scope_kind: &str, name: &str, body: &ir::IrBody) -> Vec<ProofPlanMetadata> {
    let mut plans = Vec::new();
    for (block_index, block) in body.blocks.iter().enumerate() {
        for (operation_index, instruction) in block.instructions.iter().enumerate() {
            let IrInstruction::Call { func, args, .. } = instruction else { continue };
            if func != "__ckb_require_instance_script_participant_from_args" {
                continue;
            }
            let parameter = match args.get(1) {
                Some(ir::IrOperand::Var(var)) => var.name.as_str(),
                _ => "invalid-handle-parameter",
            };
            plans.push(ProofPlanMetadata {
                name: format!("{name}#instance-admission-{block_index}-{operation_index}"),
                origin: format!("{scope_kind}:{name}#instance-admission:{block_index}:{operation_index}"),
                category: "script-instance-admission".into(),
                feature: "cellscript-instance-policy-v1".into(),
                evidence_tier: EvidenceTier::CheckedRuntime,
                source_span: None,
                trigger: trigger_for_scope_kind(scope_kind).into(),
                scope: "current-script-and-selected-participant-view".into(),
                reads: vec!["lock_args".into(), "current-script-args".into(), "witness".into(), "selected-code-cell-dep".into(), "selected-participant-script".into()],
                coverage: vec![
                    "encoding:CSINWv1-fixed-1768".into(), "consumer:CSINCv1-fixed-208".into(),
                    format!("parameter:{parameter}"), "implementation:unchanged-CSOHWv1-membership".into(),
                    "constraints:bounded-complete-fixed-args-partition".into(), "subject:concrete-complete-script-hash".into(),
                    "args:observed-on-actual-participant-cell".into(),
                ],
                input_output_relation_checks: vec![], group_cardinality: "one-selected-participant-view".into(),
                identity_lifecycle_policy: "read-only instance admission; owner, genesis, custody and lifecycle transitions remain application predicates".into(),
                preserved_fields: vec![], witness_fields: vec![parameter.into()],
                lock_args_fields: vec!["policy-root".into(), "subject-script-hash".into(), "configuration".into()],
                on_chain_checked: true,
                on_chain_checked_obligations: vec!["script-instance-admission:cellscript-instance-policy-v1=checked-runtime".into(), "authenticated-policy-and-configuration".into(), "bounded-actual-args-constraints".into(), "implementation-dependency-and-concrete-subject".into()],
                builder_assumptions: vec!["bind the private source receipt and exact final consumer/participant transaction bytes before signing".into()],
                codegen_coverage_status: "covered".into(), status: "checked-runtime".into(),
                detail: "Separately checks implementation membership and instance constraints against committed consumer args and the actual participant Script; does not establish peer success, ownership or consensus acceptance".into(),
                diagnostics: vec![],
            });
        }
    }
    plans
}
