//! Stable packed-hash wire domain vocabulary.
//! Source and executable consumers retain AST arguments and admitted IR nodes;
//! rendered type spellings are not semantic type or ownership evidence.

pub(crate) const COMMITMENT_TYPE: &str = "Commitment";
pub(crate) const OPENING_TYPE: &str = "Opening";
pub(crate) const COMMIT_FUNCTION: &str = "commitment::commit";
pub(crate) const OPEN_FUNCTION: &str = "commitment::open";
pub(crate) const PACKED_HASH_DOMAIN_PREFIX: &[u8] = b"CellScriptPackedHashV0\0";
pub(crate) const PACKED_HASH_SCRATCH_BYTES: usize = 512;
