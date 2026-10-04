use super::*;

pub(super) fn plans(scope_kind: &str, scope_name: &str, body: &ir::IrBody) -> Vec<ProofPlanMetadata> {
    body.blocks.iter().flat_map(|block| &block.instructions).filter_map(|instruction| {
        let IrInstruction::Call { func, args, .. } = instruction else { return None };
        if func != crate::zk_contract::REQUIRE_HELPER { return None; }
        let hash = |index| match &args[index] {
            ir::IrOperand::Const(ir::IrConst::Hash(value)) => hex::encode(value),
            _ => "unresolved".into(),
        };
        let policy = match &args[0] {
            ir::IrOperand::Const(ir::IrConst::Array(bytes)) => bytes.iter().filter_map(|byte| match byte {
                ir::IrConst::U8(byte) => Some(char::from(*byte)), _ => None,
            }).collect::<String>(),
            _ => "unresolved".into(),
        };
        let contract = cellscript_artifact_checker::canonical_hash("cellscript-zk-verifier-contract-v2", &(
            cellscript_artifact_checker::zk::PROFILE, hash(3), hash(4), hash(5), hash(6),
            "ckb-vm2-imc-b-mop", 128u32, 464u32, 15u32, 1u32, 250_000_000u64,
        )).expect("bounded tuple serializes");
        Some(ProofPlanMetadata {
            name: format!("{scope_name}#zk:{policy}"), origin: format!("{scope_kind}:{scope_name}"),
            category: "cryptographic-verifier-call".into(), feature: contract.clone(),
            evidence_tier: EvidenceTier::CheckedRuntime, source_span: body.zk_origins.first().map(|origin| ProofPlanSourceSpanMetadata {
                start: origin.start, end: origin.end, line: origin.line, column: origin.column,
            }),
            trigger: trigger_for_scope_kind(scope_kind).into(), scope: "single-type-group-transition".into(),
            reads: vec!["witness".into(),"GroupInput#0.data_hash".into(), "GroupInput#0.previous_output".into(), "GroupOutput#0.data_hash".into(), "current-script-hash".into(), "raw-transaction-hash".into()],
            coverage: vec![format!("policy:{policy}"), format!("profile:{}", cellscript_artifact_checker::zk::PROFILE), format!("exact-handle:{}", hash(3)), format!("vk-data-hash:{}", hash(4)), format!("domain:{}", hash(5)), format!("action:{}", hash(6)), "proof:arkworks-0.5-compressed-bn254-128".into(), "public-inputs:15-canonical-fr-u128-limbs-le".into(), "request:strict-molecule-464".into(), "witness:entry-input_type".into(), "processes:1;inherited-fds:1;words:58;cycles:250000000".into()],
            input_output_relation_checks: vec!["exactly one GroupInput and one GroupOutput; both Type hashes equal current Script".into(), "full raw transaction hash binds all inputs, outputs, output data and dependency outpoints".into()],
            group_cardinality: "1->1".into(), identity_lifecycle_policy: "immutable exact verifier handle and VK data hash; grants no lifecycle authority".into(),
            preserved_fields: vec![], witness_fields: vec!["ZkTransitionProof".into(), "ExactScriptHandle".into()], lock_args_fields: vec![],
            on_chain_checked: true, on_chain_checked_obligations: vec![format!("cryptographic-verifier-call:{contract}=checked-runtime"),"exact-handle preflight; transaction-derived context; complete request; close writer; wait exact PID; reject nonzero child exit; bounded cycles".into()],
            builder_assumptions: vec!["derive statement after finalizing raw transaction; bind exact handle receipt, VK and child artifact; regenerate proof after any raw transaction change".into(), "circuit, setup and cryptographic verifier correctness remain external assumptions".into()],
            codegen_coverage_status: "covered".into(), status: "checked-runtime".into(),
            detail: "Experimental exact Groth16 composition evidence; excludes circuit authorization, privacy, trusted setup and deployment claims".into(), diagnostics: vec![],
        })
    }).collect()
}
