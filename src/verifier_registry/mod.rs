//! Runtime verifier capability registry.
//!
//! These entries describe executable verifier artifacts that are invoked at
//! runtime, for example through CKB VM2 spawn/IPC. They are not stdlib modules
//! and must stay distinct from source-only library dependencies.

pub mod btc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifierCapability {
    pub name: &'static str,
    pub registry_package: &'static str,
    pub verifier_id: &'static str,
    pub artifact_role: &'static str,
    pub stability: &'static str,
    pub ipc_abi: &'static str,
    pub spawn_target: &'static str,
}

pub fn capabilities() -> Vec<VerifierCapability> {
    vec![btc::bip340_capability()]
}
