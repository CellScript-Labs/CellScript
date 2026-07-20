//! No-profile interoperability between CellScript fungible Type Scripts and
//! Fiber's existing CKB UDT boundary.
//!
//! This crate deliberately does not depend on `fiber-lib`. It consumes stable
//! compiler evidence, ordinary CKB deployment/action artifacts, CKB RPC, and a
//! minimal subset of Fiber's JSON-RPC/configuration schema.

mod compat;
mod deployment;
mod descriptor;
mod evidence;
mod fiber_config;
mod fiber_rpc;

pub use compat::{analyze_compile_result, check_path, CheckedFiberAsset, FiberCompatibility, FiberDiagnostic};
pub use deployment::{
    resolve_asset_from_action_plan, resolve_asset_from_live_cell, verify_code_deployment, CkbEvidenceProvider, CodeDeploymentIdentity,
    DependencyMode, HttpCkbEvidenceProvider, LiveCell, LiveCellDepEvidence, OutPointRef, ResolvedAssetScript,
    ResolvedAssetScriptSource,
};
pub use descriptor::{FiberAssetDescriptor, ScriptIdentity};
pub use evidence::{
    write_json_atomic, AcceptanceMatrixReportV1, AcceptanceMatrixRow, EvidenceBinding, FiberCompatibilityReportV1, OperationalState,
    RegistrationReportV1, TopologyReportV1,
};
pub use fiber_config::{
    build_fiber_udt_config, materialize_fiber_config, render_fiber_config_overlay, ExactArgsMatcherEvidence, FiberCellDep,
    FiberScriptConfig, FiberTypeIdScript, FiberUdtArgInfo, FiberUdtDep,
};
pub use fiber_rpc::{verify_local_registration, FiberRpcClient, LocalNodeRegistrationEvidence, NodeInfoSnapshot};

pub const FIBER_COMPATIBILITY_SCHEMA: &str = "cellscript-fiber-compatibility-v1";
pub const FIBER_CONFIG_SCHEMA: &str = "cellscript-fiber-udt-config-v1";
pub const FIBER_REGISTRATION_SCHEMA: &str = "cellscript-fiber-registration-v1";
pub const FIBER_TOPOLOGY_SCHEMA: &str = "cellscript-fiber-topology-v1";
pub const FIBER_ACCEPTANCE_SCHEMA: &str = "cellscript-fiber-acceptance-v1";
pub const FUNGIBLE_ENTRY_CONTRACT: &str = "fungible-type-group-v1";
pub const AUDITED_FIBER_REVISION: &str = "04e091b08953368aa5ee977f562ad628c3000ff4";
pub const AUDITED_FIBER_REVISIONS: &[&str] = &[
    AUDITED_FIBER_REVISION,
    // Source-equivalent to the baseline for every runtime/config/RPC file;
    // the only intervening change is a .gitignore entry.
    "e00d0e3c9a9284ea1c7705d360be615cfce1a5c6",
];

pub fn is_audited_fiber_revision(revision: &str) -> bool {
    AUDITED_FIBER_REVISIONS.contains(&revision)
}
