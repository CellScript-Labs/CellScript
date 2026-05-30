use super::VerifierCapability;

pub const BIP340_CAPABILITY_NAME: &str = "verifier::btc::bip340";
pub const BIP340_REGISTRY_PACKAGE: &str = "cellscript-labs/btc-bip340";
pub const BIP340_VERIFIER_ID: &str = "btc.bip340.v0";
pub const BIP340_ARTIFACT_ROLE: &str = "spawn-verifier";
pub const BIP340_STABILITY: &str = "runtime-backed-experimental";
pub const BIP340_IPC_ABI: &str = "cellscript-btc-bip340-ipc-v0";
pub const BIP340_RISCV_TARGET: &str = "cellscript_btc_bip340_verifier_riscv";

pub fn bip340_capability() -> VerifierCapability {
    VerifierCapability {
        name: BIP340_CAPABILITY_NAME,
        registry_package: BIP340_REGISTRY_PACKAGE,
        verifier_id: BIP340_VERIFIER_ID,
        artifact_role: BIP340_ARTIFACT_ROLE,
        stability: BIP340_STABILITY,
        ipc_abi: BIP340_IPC_ABI,
        spawn_target: BIP340_RISCV_TARGET,
    }
}
