//! Pure source/runtime constants for exact Script handles.
//!
//! This module stays available to the wasm metadata compiler. Native receipt
//! construction lives in `script_handle`, which additionally depends on the
//! package, artifact-checker, and ProtocolBundle boundary.

pub const EXACT_SCRIPT_HANDLE_TYPE: &str = "ExactScriptHandle";
pub const EXACT_SCRIPT_HANDLE_ENCODING: &str = "CSHDLv1-fixed-202";
pub const EXACT_SCRIPT_HANDLE_BYTES: usize = 202;
pub const EXACT_SCRIPT_HANDLE_MAGIC: &[u8; 8] = b"CSHDLv1\0";

pub const EXACT_SCRIPT_HANDLE_CLASS_OFFSET: usize = 8;
pub const EXACT_SCRIPT_HANDLE_ROLE_OFFSET: usize = 9;
pub const EXACT_SCRIPT_HANDLE_RECEIPT_HASH_OFFSET: usize = 10;
pub const EXACT_SCRIPT_HANDLE_SCRIPT_HASH_OFFSET: usize = 42;
pub const EXACT_SCRIPT_HANDLE_INTERFACE_HASH_OFFSET: usize = 74;
pub const EXACT_SCRIPT_HANDLE_ARTIFACT_HASH_OFFSET: usize = 106;
pub const EXACT_SCRIPT_HANDLE_TARGET_PROFILE_HASH_OFFSET: usize = 138;
pub const EXACT_SCRIPT_HANDLE_RUNTIME_ABI_HASH_OFFSET: usize = 170;

pub const EXACT_SCRIPT_HANDLE_HASH_BYTES: usize = 32;
pub const EXACT_SCRIPT_HANDLE_CLASS_SCRIPT: u8 = 0;
pub const EXACT_SCRIPT_HANDLE_CLASS_VERIFIER: u8 = 1;
pub const EXACT_SCRIPT_HANDLE_ROLE_LOCK: u8 = 0;
pub const EXACT_SCRIPT_HANDLE_ROLE_TYPE: u8 = 1;
pub const EXACT_SCRIPT_HANDLE_ROLE_SPAWNED_VERIFIER: u8 = 2;

pub const DEPLOYMENT_LINE_HANDLE_ENCODING: &str = "CSLINv1-fixed-386";
pub const DEPLOYMENT_LINE_HANDLE_TYPE: &str = "DeploymentLineHandle";
pub const DEPLOYMENT_LINE_HANDLE_BYTES: usize = 386;
pub const DEPLOYMENT_LINE_HANDLE_MAGIC: &[u8; 8] = b"CSLINv1\0";
pub const DEPLOYMENT_LINE_HANDLE_CLASS_OFFSET: usize = 8;
pub const DEPLOYMENT_LINE_HANDLE_ROLE_OFFSET: usize = 9;
pub const DEPLOYMENT_LINE_HANDLE_STATUS_OFFSET: usize = 10;
pub const DEPLOYMENT_LINE_HANDLE_RESERVED_OFFSET: usize = 11;
pub const DEPLOYMENT_LINE_HANDLE_RESERVED_BYTES: usize = 5;
pub const DEPLOYMENT_LINE_HANDLE_SEQUENCE_OFFSET: usize = 16;
pub const DEPLOYMENT_LINE_HANDLE_LINE_ID_OFFSET: usize = 24;
pub const DEPLOYMENT_LINE_HANDLE_POLICY_HASH_OFFSET: usize = 56;
pub const DEPLOYMENT_LINE_HANDLE_RECEIPT_HASH_OFFSET: usize = 88;
pub const DEPLOYMENT_LINE_HANDLE_PREVIOUS_RECEIPT_HASH_OFFSET: usize = 120;
pub const DEPLOYMENT_LINE_HANDLE_ADMISSION_TYPE_HASH_OFFSET: usize = 152;
pub const DEPLOYMENT_LINE_HANDLE_EXACT_HANDLE_OFFSET: usize = 184;
pub const DEPLOYMENT_LINE_HANDLE_STATUS_ACTIVE: u8 = 0;
pub const DEPLOYMENT_LINE_HANDLE_STATUS_YANKED: u8 = 1;
pub const DEPLOYMENT_LINE_COMMITMENT_MAGIC: &[u8; 7] = b"CSREGv1";

/// Reserved nominal class names for the #28 compatible-open handle surface
/// (H2). The source syntax is not admitted yet: both spellings fail closed at
/// type validation and declaration registration until the frozen
/// interface-parameter binding ships with its runtime enforcement.
pub const OPEN_SCRIPT_HANDLE_TYPE: &str = "ScriptHandle";
pub const OPEN_VERIFIER_HANDLE_TYPE: &str = "VerifierHandle";

/// Bounded runtime layout of the 656-byte compatible-open selection witness
/// (`CSOHWv1\0`), shared by the on-chain requirement helpers and the tests.
pub const OPEN_HANDLE_SELECTION_BYTES: usize = 656;
pub const OPEN_HANDLE_SELECTION_MAGIC: &[u8; 8] = b"CSOHWv1\0";
pub const OPEN_HANDLE_SELECTION_HEADER_OFFSET: usize = 8;
pub const OPEN_HANDLE_HEADER_CLASS_OFFSET: usize = 8;
pub const OPEN_HANDLE_HEADER_ROLE_OFFSET: usize = 9;
#[allow(dead_code)]
pub const OPEN_HANDLE_HEADER_MODE_OFFSET: usize = 10;
pub const OPEN_HANDLE_HEADER_MEMBER_COUNT_OFFSET: usize = 11;
pub const OPEN_HANDLE_MEMBER_OFFSET: usize = 196;
pub const OPEN_HANDLE_MEMBER_STATUS_OFFSET: usize = 196;
pub const OPEN_HANDLE_MEMBER_COMPLETE_SCRIPT_OFFSET: usize = 324;
pub const OPEN_HANDLE_INDEX_OFFSET: usize = 488;
pub const OPEN_HANDLE_SIBLING_OFFSET: usize = 496;
pub const OPEN_HANDLE_TREE_DEPTH: usize = 5;
pub const OPEN_HANDLE_MEMBER_DOMAIN: &[u8] = b"cellscript-open-handle-member-v1\0";
pub const OPEN_HANDLE_NODE_DOMAIN: &[u8] = b"cellscript-open-handle-node-v1\0";
pub const OPEN_HANDLE_POLICY_DOMAIN: &[u8] = b"cellscript-open-handle-policy-v1\0";
/// The mode byte accepts exact (0) and compatible (1); the header role byte
/// selects the Script binding (Lock or Type) or the spawned verifier class.
/// These document the runtime checks; the assembler compares them inline.
pub const OPEN_HANDLE_CLASS_SCRIPT: u8 = 0;
pub const OPEN_HANDLE_CLASS_VERIFIER: u8 = 1;
#[allow(dead_code)]
pub const OPEN_HANDLE_ROLE_LOCK: u8 = 0;
#[allow(dead_code)]
pub const OPEN_HANDLE_ROLE_TYPE: u8 = 1;
pub const OPEN_HANDLE_ROLE_SPAWNED_VERIFIER: u8 = 2;
