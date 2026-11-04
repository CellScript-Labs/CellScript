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
    "crates/cellscript-novaseal-certification/src/error.rs",
    "crates/cellscript-novaseal-certification/src/summary.rs",
    "crates/cellscript-canonical-json/Cargo.toml",
    "crates/cellscript-canonical-json/src/lib.rs",
];
pub const ENGINE_SOURCE_PATH: &str = "crates/cellscript-novaseal-certification/src/engine.rs";

fn ckb_blake2b256(data: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new().hash_length(32).personal(b"ckb-default-hash").hash(data).as_bytes().try_into().expect("32-byte digest")
}
