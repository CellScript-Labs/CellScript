//! std::dao — NervosDAO helper surface for CKB HeaderDep and since checks.

use super::{CkbStdlibModule, ProtocolFunction};
use crate::ir::IrType;

pub fn module() -> CkbStdlibModule {
    CkbStdlibModule {
        name: "std::dao".to_string(),
        path: "std::dao".to_string(),
        script_type: "type".to_string(),
        proof_plan_trigger: "type_group".to_string(),
        proof_plan_scope: "transaction".to_string(),
        proof_plan_reads: vec!["group_input".to_string(), "header_dep".to_string()],
        builder_assumptions: vec![],
        compatibility_fixture: "dao".to_string(),
        stability: "runtime-backed-v0.17-partial".to_string(),
    }
}

pub fn functions() -> Vec<ProtocolFunction> {
    vec![
        ProtocolFunction {
            name: "dao_accumulated_rate".to_string(),
            module: "std::dao".to_string(),
            params: vec![("header_view".to_string(), IrType::U64)],
            return_type: Some(IrType::U64),
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "transaction".to_string(),
            proof_plan_reads: vec!["header_dep".to_string()],
        },
        ProtocolFunction {
            name: "dao_input_accumulated_rate".to_string(),
            module: "std::dao".to_string(),
            params: vec![("input_view".to_string(), IrType::U64)],
            return_type: Some(IrType::U64),
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "transaction".to_string(),
            proof_plan_reads: vec!["input".to_string(), "group_input".to_string()],
        },
        ProtocolFunction {
            name: "dao_has_dao_type".to_string(),
            module: "std::dao".to_string(),
            params: vec![("source_view".to_string(), IrType::U64)],
            return_type: Some(IrType::Bool),
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "transaction".to_string(),
            proof_plan_reads: vec!["input".to_string(), "output".to_string()],
        },
        ProtocolFunction {
            name: "dao_is_deposit_data".to_string(),
            module: "std::dao".to_string(),
            params: vec![("source_view".to_string(), IrType::U64)],
            return_type: Some(IrType::Bool),
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "transaction".to_string(),
            proof_plan_reads: vec!["input".to_string(), "output".to_string()],
        },
        ProtocolFunction {
            name: "dao_is_withdrawal_request_data".to_string(),
            module: "std::dao".to_string(),
            params: vec![("source_view".to_string(), IrType::U64)],
            return_type: Some(IrType::Bool),
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "transaction".to_string(),
            proof_plan_reads: vec!["input".to_string(), "output".to_string()],
        },
        ProtocolFunction {
            name: "dao_require_header_dep_for_input".to_string(),
            module: "std::dao".to_string(),
            params: vec![("input_view".to_string(), IrType::U64), ("header_view".to_string(), IrType::U64)],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "transaction".to_string(),
            proof_plan_reads: vec!["input".to_string(), "header_dep".to_string()],
        },
        ProtocolFunction {
            name: "dao_require_input_since_at_least".to_string(),
            module: "std::dao".to_string(),
            params: vec![("input_view".to_string(), IrType::U64), ("required_since".to_string(), IrType::U64)],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["input".to_string(), "group_input".to_string()],
        },
        ProtocolFunction {
            name: "dao_require_input_relative_epoch_since_at_least".to_string(),
            module: "std::dao".to_string(),
            params: vec![
                ("input_view".to_string(), IrType::U64),
                ("number".to_string(), IrType::U64),
                ("index".to_string(), IrType::U64),
                ("length".to_string(), IrType::U64),
            ],
            return_type: None,
            proof_plan_trigger: "type_group".to_string(),
            proof_plan_scope: "group".to_string(),
            proof_plan_reads: vec!["input".to_string(), "group_input".to_string()],
        },
    ]
}
