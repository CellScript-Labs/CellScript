//! std::acp — CKB Anyone-Can-Pay script wrapper.

use super::{CkbStdlibModule, ProtocolFunction};
use crate::ir::IrType;

pub fn module() -> CkbStdlibModule {
    CkbStdlibModule {
        name: "std::acp".to_string(),
        path: "std::acp".to_string(),
        script_type: "lock".to_string(),
        proof_plan_trigger: "lock_group".to_string(),
        proof_plan_scope: "group".to_string(),
        proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string(), "witness".to_string(), "lock_args".to_string()],
        builder_assumptions: vec![],
        compatibility_fixture: "acp".to_string(),
        stability: "schema-stub".to_string(),
    }
}

pub fn functions() -> Vec<ProtocolFunction> {
    vec![
        ProtocolFunction {
            name: "acp_deposit".to_string(),
            module: "std::acp".to_string(),
            params: vec![
                ("owner_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ("amount".to_string(), IrType::U128),
            ],
            return_type: None,
            proof_plan_trigger: "lock_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string(), "lock_args".to_string()],
        },
        ProtocolFunction {
            name: "acp_withdraw".to_string(),
            module: "std::acp".to_string(),
            params: vec![
                ("owner_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
                ("amount".to_string(), IrType::U128),
            ],
            return_type: None,
            proof_plan_trigger: "lock_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec![
                "group_input".to_string(),
                "group_output".to_string(),
                "witness".to_string(),
                "lock_args".to_string(),
            ],
        },
    ]
}
