//! std::htlc — Hash Time-Locked Contract pattern for CKB.

use super::{CkbStdlibModule, ProtocolFunction};
use crate::ir::IrType;

pub fn module() -> CkbStdlibModule {
    CkbStdlibModule {
        name: "std::htlc".to_string(),
        path: "std::htlc".to_string(),
        script_type: "lock".to_string(),
        proof_plan_trigger: "lock_group".to_string(),
        proof_plan_scope: "group".to_string(),
        proof_plan_reads: vec!["group_input".to_string(), "witness".to_string(), "lock_args".to_string()],
        builder_assumptions: vec!["htlc-preimage-or-timelock-evidence".to_string()],
        compatibility_fixture: "htlc".to_string(),
        stability: "schema-stub".to_string(),
    }
}

pub fn functions() -> Vec<ProtocolFunction> {
    vec![
        ProtocolFunction {
            name: "htlc_claim_with_preimage".to_string(),
            module: "std::htlc".to_string(),
            params: vec![
                ("preimage".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ("hash_lock".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
            ],
            return_type: Some(IrType::Bool),
            proof_plan_trigger: "lock_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "witness".to_string(), "lock_args".to_string()],
        },
        ProtocolFunction {
            name: "htlc_claim_with_timelock".to_string(),
            module: "std::htlc".to_string(),
            params: vec![
                ("since_epoch".to_string(), IrType::U64),
                ("refund_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
            ],
            return_type: Some(IrType::Bool),
            proof_plan_trigger: "lock_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec![
                "group_input".to_string(),
                "witness".to_string(),
                "lock_args".to_string(),
                "header_dep".to_string(),
            ],
        },
    ]
}
