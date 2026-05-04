//! std::cheque — CKB Cheque script wrapper for conditional payments.

use super::{CkbStdlibModule, ProtocolFunction};
use crate::ir::IrType;

pub fn module() -> CkbStdlibModule {
    CkbStdlibModule {
        name: "std::cheque".to_string(),
        path: "std::cheque".to_string(),
        script_type: "lock".to_string(),
        proof_plan_trigger: "lock_group".to_string(),
        proof_plan_scope: "group".to_string(),
        proof_plan_reads: vec!["group_input".to_string(), "output".to_string(), "witness".to_string()],
        builder_assumptions: vec![],
        compatibility_fixture: "cheque".to_string(),
        stability: "schema-stub".to_string(),
    }
}

pub fn functions() -> Vec<ProtocolFunction> {
    vec![
        ProtocolFunction {
            name: "cheque_claim".to_string(),
            module: "std::cheque".to_string(),
            params: vec![
                ("sender_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ("receiver_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ("amount".to_string(), IrType::U128),
            ],
            return_type: None,
            proof_plan_trigger: "lock_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "output".to_string(), "witness".to_string()],
        },
        ProtocolFunction {
            name: "cheque_refund".to_string(),
            module: "std::cheque".to_string(),
            params: vec![
                ("sender_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ("receiver_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
            ],
            return_type: None,
            proof_plan_trigger: "lock_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "witness".to_string()],
        },
    ]
}
