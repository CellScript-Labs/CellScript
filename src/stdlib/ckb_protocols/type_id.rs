//! std::type_id — CKB TYPE_ID script wrapper for unique cell identity.

use super::{CkbStdlibModule, ProtocolFunction};
use crate::ir::IrType;

pub fn module() -> CkbStdlibModule {
    CkbStdlibModule {
        name: "std::type_id".to_string(),
        path: "std::type_id".to_string(),
        script_type: "type".to_string(),
        proof_plan_trigger: "type_group".to_string(),
        proof_plan_scope: "group".to_string(),
        proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string()],
        builder_assumptions: vec!["type_id_builder_plan".to_string()],
        compatibility_fixture: "type-id".to_string(),
        stability: "schema-stub".to_string(),
    }
}

pub fn functions() -> Vec<ProtocolFunction> {
    vec![ProtocolFunction {
        name: "type_id_create".to_string(),
        module: "std::type_id".to_string(),
        params: vec![
            ("first_input_tx_hash".to_string(), IrType::Array(Box::new(IrType::U8), 32)),
            ("output_index".to_string(), IrType::U32),
        ],
        return_type: Some(IrType::Array(Box::new(IrType::U8), 32)),
        proof_plan_trigger: "type_group".to_string(),
        proof_plan_scope: "group".to_string(),
        proof_plan_reads: vec!["group_input".to_string(), "group_output".to_string()],
    }]
}
