//! Browser-facing WASM bindings for the CellScript compiler.
//!
//! This crate exposes the pure in-memory compile path
//! (`lex -> parse -> types -> flow -> ir -> metadata`) to JavaScript
//! via `wasm-bindgen`. It does NOT expose the ELF codegen path in v1
//! (that would inflate the bundle beyond the 600KB budget and is
//! tracked as RFC path B / v2).
//!
//! The single exported function `compile_metadata_json` takes source
//! text and an optional target profile, and returns a JSON string.
//! On success the string is the serialized `CompileMetadata`; on
//! failure it is `{"error": "..."}` so the playground can parse it
//! uniformly and render diagnostics.

use wasm_bindgen::prelude::*;

/// Compile CellScript source to metadata JSON (path A, no ELF).
///
/// Returns a JSON string. On success this is the serialized
/// `CompileMetadata` (module, types, actions with effect_class /
/// consume_set / create_set / estimated_cycles, etc.). On error it
/// is `{"error": "<message>"}`.
///
/// The `target` argument is optional; pass `None` for the default
/// (ckb) target profile.
#[wasm_bindgen]
pub fn compile_metadata_json(source: &str, target: Option<String>) -> String {
    match cellscript::compile_metadata(source, target) {
        Ok(metadata) => serde_json::to_string(&metadata)
            .unwrap_or_else(|e| error_json(&format!("failed to serialize metadata: {e}"))),
        Err(e) => error_json(&e.to_string()),
    }
}

/// Return the compiler version string (e.g. "0.17.0").
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn error_json(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}
