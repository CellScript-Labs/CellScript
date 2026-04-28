//! std::xudt — eXtensible UDT standard script wrapper for CKB.

use super::{CkbStdlibModule, ProtocolFunction};
use crate::ir::IrType;

pub fn module() -> CkbStdlibModule {
    CkbStdlibModule {
        name: "std::xudt".to_string(),
        path: "std::xudt".to_string(),
        script_type: "type".to_string(),
        proof_plan_trigger: "type_group".to_string(),
        proof_plan_scope: "group".to_string(),
        proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string(), "witness".to_string()],
        builder_assumptions: vec!["extension-dep-available".to_string()],
        compatibility_fixture: "xudt".to_string(),
        stability: "schema-stub".to_string(),
    }
}

pub fn functions() -> Vec<ProtocolFunction> {
    vec![ProtocolFunction {
        name: "xudt_transfer".to_string(),
        module: "std::xudt".to_string(),
        params: vec![
            ("amount".to_string(), IrType::U128),
            ("sender_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
            ("receiver_lock_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
            ("extension_flags".to_string(), IrType::U16),
        ],
        return_type: None,
        proof_plan_trigger: "type_group".to_string(),
        proof_plan_scope: "group".to_string(),
        proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string(), "witness".to_string()],
    }]
}
