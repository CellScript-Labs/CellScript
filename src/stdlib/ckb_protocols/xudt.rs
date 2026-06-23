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
        stability: "runtime-backed-v0.17-partial".to_string(),
    }
}

pub fn functions() -> Vec<ProtocolFunction> {
    vec![
        ProtocolFunction {
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
        },
        ProtocolFunction {
            name: "xudt_amount_low".to_string(),
            module: "std::xudt".to_string(),
            params: vec![("source_view".to_string(), IrType::U64)],
            return_type: Some(IrType::U64),
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string()],
        },
        ProtocolFunction {
            name: "xudt_require_owner_mode_input_type".to_string(),
            module: "std::xudt".to_string(),
            params: vec![("source_view".to_string(), IrType::U64), ("expected_type_hash".to_string(), IrType::Hash)],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string()],
        },
        ProtocolFunction {
            name: "xudt_require_owner_mode_type_args".to_string(),
            module: "std::xudt".to_string(),
            params: vec![
                ("source_view".to_string(), IrType::U64),
                ("owner_hash".to_string(), IrType::Hash),
                ("flags".to_string(), IrType::U64),
            ],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string()],
        },
        ProtocolFunction {
            name: "xudt_require_owner_mode_type_args_current_script".to_string(),
            module: "std::xudt".to_string(),
            params: vec![("source_view".to_string(), IrType::U64), ("flags".to_string(), IrType::U64)],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string(), "current_script_hash".to_string()],
        },
        ProtocolFunction {
            name: "xudt_require_group_amount_conserved".to_string(),
            module: "std::xudt".to_string(),
            params: vec![],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string()],
        },
        ProtocolFunction {
            name: "xudt_require_group_amount_minted".to_string(),
            module: "std::xudt".to_string(),
            params: vec![("delta".to_string(), IrType::U128)],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string()],
        },
        ProtocolFunction {
            name: "xudt_require_group_amount_burned".to_string(),
            module: "std::xudt".to_string(),
            params: vec![("delta".to_string(), IrType::U128)],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string()],
        },
    ]
}
