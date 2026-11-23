//! Optional native proposal implementation. No dependency on the compiler or its CLI.
mod engine;
mod error;
mod summary;

pub use engine::build_report;
pub use error::{Error, Result};
pub use summary::{failure_message, summarize, SummaryRequest};

pub const PLUGIN_ID: &str = "novaseal-profile-v0";
pub const IMPLEMENTATION_ID: &str = "cellscript-novaseal-certification";
/// Source provenance includes policy interpretation and shared producer encoding.
pub const IMPLEMENTATION_SOURCES: &[&str] = &[
    "crates/cellscript-novaseal-certification/Cargo.toml",
    "crates/cellscript-novaseal-certification/src/lib.rs",
    "crates/cellscript-novaseal-certification/src/engine.rs",
    "crates/cellscript-novaseal-certification/src/engine/bitcoin_anchors.rs",
    "crates/cellscript-novaseal-certification/src/engine/bitcoin_proofs.rs",
    "crates/cellscript-novaseal-certification/src/engine/endpoint_evidence.rs",
    "crates/cellscript-novaseal-certification/src/engine/evidence_adapters.rs",
    "crates/cellscript-novaseal-certification/src/engine/external_evidence.rs",
    "crates/cellscript-novaseal-certification/src/engine/fiber_provenance.rs",
    "crates/cellscript-novaseal-certification/src/engine/live_reports.rs",
    "crates/cellscript-novaseal-certification/src/engine/package_sources.rs",
    "crates/cellscript-novaseal-certification/src/engine/profile_certification.rs",
    "crates/cellscript-novaseal-certification/src/engine/profile_packages.rs",
    "crates/cellscript-novaseal-certification/src/engine/profile_readiness.rs",
    "crates/cellscript-novaseal-certification/src/engine/provenance.rs",
    "crates/cellscript-novaseal-certification/src/engine/readiness.rs",
    "crates/cellscript-novaseal-certification/src/engine/report_files.rs",
    "crates/cellscript-novaseal-certification/src/engine/report_origins.rs",
    "crates/cellscript-novaseal-certification/src/engine/source_security.rs",
    "crates/cellscript-novaseal-certification/src/engine/stateful_acceptance.rs",
    "crates/cellscript-novaseal-certification/src/engine/timestamps.rs",
    "crates/cellscript-novaseal-certification/src/engine/wallet_and_operator.rs",
    "crates/cellscript-novaseal-certification/src/error.rs",
    "crates/cellscript-novaseal-certification/src/summary.rs",
    "crates/cellscript-canonical-json/Cargo.toml",
    "crates/cellscript-canonical-json/src/lib.rs",
];
pub const ENGINE_SOURCE_PATH: &str = "crates/cellscript-novaseal-certification/src/engine.rs";

fn ckb_blake2b256(data: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new().hash_length(32).personal(b"ckb-default-hash").hash(data).as_bytes().try_into().expect("32-byte digest")
}
