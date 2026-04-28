//! CKB standard library protocol modules for v0.16.
//!
//! Each module wraps a CKB standard script pattern with ProofPlan metadata,
//! builder assumption transparency, and compatibility fixture references.

pub mod acp;
pub mod cheque;
pub mod htlc;
pub mod sudt;
pub mod type_id;
pub mod xudt;

use crate::ir::IrType;
use serde::{Deserialize, Serialize};

/// Schema for a CKB stdlib protocol module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CkbStdlibModule {
    pub name: String,
    pub path: String,
    pub script_type: String,
    pub proof_plan_trigger: String,
    pub proof_plan_scope: String,
    pub proof_plan_reads: Vec<String>,
    pub builder_assumptions: Vec<String>,
    pub compatibility_fixture: String,
    pub stability: String,
}

/// Protocol module function descriptor for the CKB stdlib.
#[derive(Debug, Clone)]
pub struct ProtocolFunction {
    pub name: String,
    pub module: String,
    pub params: Vec<(String, IrType)>,
    pub return_type: Option<IrType>,
    pub proof_plan_trigger: String,
    pub proof_plan_scope: String,
    pub proof_plan_reads: Vec<String>,
}

/// All CKB stdlib protocol modules.
pub fn ckb_stdlib_modules() -> Vec<CkbStdlibModule> {
    vec![sudt::module(), xudt::module(), type_id::module(), htlc::module(), cheque::module(), acp::module()]
}

/// All CKB stdlib protocol functions.
pub fn ckb_stdlib_functions() -> Vec<ProtocolFunction> {
    let mut functions = Vec::new();
    functions.extend(sudt::functions());
    functions.extend(xudt::functions());
    functions.extend(type_id::functions());
    functions.extend(htlc::functions());
    functions.extend(cheque::functions());
    functions.extend(acp::functions());
    functions
}
