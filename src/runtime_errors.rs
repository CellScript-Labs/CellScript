/// Stable runtime verifier error codes emitted by generated CellScript artifacts.
///
/// These codes are part of the debugging and release-reporting surface. They are
/// intentionally stable: the same generated verifier condition should return
/// the same numeric code across releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u64)]
pub enum CellScriptRuntimeError {
    SyscallFailed = 1,
    BoundsCheckFailed = 2,
    CellLoadFailed = 3,
    ExactSizeMismatch = 4,
    AssertionFailed = 5,
    LifecycleTransitionMismatch = 7,
    LifecycleNewStateInvalid = 8,
    LifecycleOldStateInvalid = 9,
    EntryWitnessMagicMismatch = 10,
    TypeHashPreservationMismatch = 11,
    LockHashPreservationMismatch = 12,
    FieldPreservationMismatch = 13,
    MutateTransitionMismatch = 14,
    DataPreservationMismatch = 15,
    DynamicFieldBoundsInvalid = 16,
    TypeHashMismatch = 17,
    FixedByteComparisonUnresolved = 18,
    ClaimSignatureFailed = 19,
    NumericOrDiscriminantInvalid = 20,
    CollectionBoundsInvalid = 21,
    ConsumeInvalidOperand = 22,
    DestroyInvalidOperand = 23,
    CollectionRuntimeUnsupported = 24,
    EntryWitnessAbiInvalid = 25,
    CkbSourceViewInvalid = 26,
    HeaderDepMissing = 27,
    DaoFieldMalformed = 28,
    ScriptRoleMismatch = 29,
    XudtBindingMismatch = 30,
    AggregateAmountMismatch = 31,
    DynamicFieldValueMismatch = 32,
    OutPointMismatch = 33,
    ScriptFieldMalformed = 34,
    DaoHeaderLineageMismatch = 35,
    DaoMaturityViolation = 36,
    CkbSinceMalformed = 37,
    ScriptArgsMismatch = 38,
    MetaPointMismatch = 39,
    MetaPointCardinalityMismatch = 40,
    ScriptIdentityMismatch = 41,
    WitnessMalformed = 42,
    WitnessFieldTruncated = 43,
}

impl CellScriptRuntimeError {
    pub const fn code(self) -> u64 {
        self as u64
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::SyscallFailed => "syscall-failed",
            Self::BoundsCheckFailed => "bounds-check-failed",
            Self::CellLoadFailed => "cell-load-failed",
            Self::ExactSizeMismatch => "exact-size-mismatch",
            Self::AssertionFailed => "assertion-failed",
            Self::LifecycleTransitionMismatch => "lifecycle-transition-mismatch",
            Self::LifecycleNewStateInvalid => "lifecycle-new-state-invalid",
            Self::LifecycleOldStateInvalid => "lifecycle-old-state-invalid",
            Self::EntryWitnessMagicMismatch => "entry-witness-magic-mismatch",
            Self::TypeHashPreservationMismatch => "type-hash-preservation-mismatch",
            Self::LockHashPreservationMismatch => "lock-hash-preservation-mismatch",
            Self::FieldPreservationMismatch => "field-preservation-mismatch",
            Self::MutateTransitionMismatch => "mutate-transition-mismatch",
            Self::DataPreservationMismatch => "data-preservation-mismatch",
            Self::DynamicFieldBoundsInvalid => "dynamic-field-bounds-invalid",
            Self::TypeHashMismatch => "type-hash-mismatch",
            Self::FixedByteComparisonUnresolved => "fixed-byte-comparison-unresolved",
            Self::ClaimSignatureFailed => "claim-signature-failed",
            Self::NumericOrDiscriminantInvalid => "numeric-or-discriminant-invalid",
            Self::CollectionBoundsInvalid => "collection-bounds-invalid",
            Self::ConsumeInvalidOperand => "consume-invalid-operand",
            Self::DestroyInvalidOperand => "destroy-invalid-operand",
            Self::CollectionRuntimeUnsupported => "collection-runtime-unsupported",
            Self::EntryWitnessAbiInvalid => "entry-witness-abi-invalid",
            Self::CkbSourceViewInvalid => "ckb-source-view-invalid",
            Self::HeaderDepMissing => "header-dep-missing",
            Self::DaoFieldMalformed => "dao-field-malformed",
            Self::ScriptRoleMismatch => "script-role-mismatch",
            Self::XudtBindingMismatch => "xudt-binding-mismatch",
            Self::AggregateAmountMismatch => "aggregate-amount-mismatch",
            Self::DynamicFieldValueMismatch => "dynamic-field-value-mismatch",
            Self::OutPointMismatch => "out-point-mismatch",
            Self::ScriptFieldMalformed => "script-field-malformed",
            Self::DaoHeaderLineageMismatch => "dao-header-lineage-mismatch",
            Self::DaoMaturityViolation => "dao-maturity-violation",
            Self::CkbSinceMalformed => "ckb-since-malformed",
            Self::ScriptArgsMismatch => "script-args-mismatch",
            Self::MetaPointMismatch => "metapoint-mismatch",
            Self::MetaPointCardinalityMismatch => "metapoint-cardinality-mismatch",
            Self::ScriptIdentityMismatch => "script-identity-mismatch",
            Self::WitnessMalformed => "witness-malformed",
            Self::WitnessFieldTruncated => "witness-field-truncated",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::SyscallFailed => "A target VM syscall returned a non-zero status while loading transaction context.",
            Self::BoundsCheckFailed => "Loaded bytes were smaller than the verifier-required minimum.",
            Self::CellLoadFailed => "Cell data or field loading failed or returned an unusable result.",
            Self::ExactSizeMismatch => "Loaded bytes did not match the exact fixed-size schema requirement.",
            Self::AssertionFailed => "A source-level assert or invariant check evaluated to false.",
            Self::LifecycleTransitionMismatch => "A lifecycle state transition did not match the declared transition rule.",
            Self::LifecycleNewStateInvalid => "A created or replacement lifecycle state was outside the declared state range.",
            Self::LifecycleOldStateInvalid => "A consumed lifecycle state was outside the declared state range.",
            Self::EntryWitnessMagicMismatch => "Entry witness bytes did not start with the CellScript witness ABI magic.",
            Self::TypeHashPreservationMismatch => "A replacement output did not preserve the consumed input type hash.",
            Self::LockHashPreservationMismatch => "A replacement output did not preserve the consumed input lock hash.",
            Self::FieldPreservationMismatch => "An output field required to be preserved differs from its input field.",
            Self::MutateTransitionMismatch => "A mutable replacement output failed its declared field transition check.",
            Self::DataPreservationMismatch => "Replacement output data outside transition ranges differs from the input data.",
            Self::DynamicFieldBoundsInvalid => "A Molecule dynamic field offset or length failed bounds validation.",
            Self::TypeHashMismatch => "A loaded cell type hash did not match the expected CellScript type identity.",
            Self::FixedByteComparisonUnresolved => "A fixed-byte verifier comparison could not resolve its trusted source bytes.",
            Self::ClaimSignatureFailed => "Claim authorization signature verification failed.",
            Self::NumericOrDiscriminantInvalid => "A numeric verifier check, enum discriminant, or arithmetic guard failed.",
            Self::CollectionBoundsInvalid => "A runtime collection index, length, or capacity check failed.",
            Self::ConsumeInvalidOperand => "A consume operation reached codegen with an invalid or unsupported operand.",
            Self::DestroyInvalidOperand => "A destroy operation reached codegen with an invalid or unsupported operand.",
            Self::CollectionRuntimeUnsupported => "A runtime collection helper shape is not supported by the current backend.",
            Self::EntryWitnessAbiInvalid => "Entry witness payload layout, width, or parameter ABI placement was invalid.",
            Self::CkbSourceViewInvalid => "A CKB SourceView value was malformed or used with an incompatible source-specific helper.",
            Self::HeaderDepMissing => "A required HeaderDep source view was absent or could not be bound to the requested header.",
            Self::DaoFieldMalformed => "A loaded DAO header or cell field did not match the expected encoded layout.",
            Self::ScriptRoleMismatch => "The script was used in a lock/type role that violates the declared invariant.",
            Self::XudtBindingMismatch => "An xUDT type args, owner-mode, or amount binding check failed.",
            Self::AggregateAmountMismatch => "A lowered aggregate/C256 accounting equality or inequality check failed.",
            Self::DynamicFieldValueMismatch => "A dynamic Molecule field value did not match the expected verifier source.",
            Self::OutPointMismatch => "A loaded input OutPoint field did not match the expected transaction lineage.",
            Self::ScriptFieldMalformed => "A loaded CKB Script field did not match the expected Molecule Script layout.",
            Self::DaoHeaderLineageMismatch => {
                "The DAO field loaded from an input's committed block header did not match the supplied HeaderDep."
            }
            Self::DaoMaturityViolation => "The DAO input since value was below the required maturity lower bound.",
            Self::CkbSinceMalformed => "A CKB since value or requested since constructor argument was malformed.",
            Self::ScriptArgsMismatch => "A loaded CKB Script args field did not match the expected args policy.",
            Self::MetaPointMismatch => {
                "A loaded CKB MetaPoint relation did not match the expected input/output index and relative distance."
            }
            Self::MetaPointCardinalityMismatch => {
                "A current-script lock/type MetaPoint pair scan found a duplicate, missing, or unbalanced relation."
            }
            Self::ScriptIdentityMismatch => "A loaded CKB Script code_hash or hash_type did not match the expected identity.",
            Self::WitnessMalformed => "Loaded witness bytes did not match the expected Molecule WitnessArgs layout or ABI magic.",
            Self::WitnessFieldTruncated => "A WitnessArgs field offset or length exceeded the loaded witness byte range.",
        }
    }

    pub const fn hint(self) -> &'static str {
        match self {
            Self::SyscallFailed => {
                "Check transaction input/output/cell_dep indexes, source flags, and target-profile syscall compatibility."
            }
            Self::BoundsCheckFailed => "Check witness or cell data length against the schema manifest and entry ABI report.",
            Self::CellLoadFailed => {
                "Check that the expected input, output, or dep cell exists and is reachable by the generated script."
            }
            Self::ExactSizeMismatch => {
                "Check fixed-width schema fields and ensure the builder encodes the exact Molecule byte length."
            }
            Self::AssertionFailed => "Inspect the action invariant or assert expression and the transaction values that feed it.",
            Self::LifecycleTransitionMismatch => {
                "Compare consumed and produced lifecycle state fields with the declared lifecycle transitions."
            }
            Self::LifecycleNewStateInvalid => "Check created output lifecycle state values and declared lifecycle states.",
            Self::LifecycleOldStateInvalid => "Check consumed input lifecycle state values and declared lifecycle states.",
            Self::EntryWitnessMagicMismatch => {
                "Encode entry witnesses with cellc entry-witness or the documented CSARGv1 wire format."
            }
            Self::TypeHashPreservationMismatch => "Check the replacement output type script and builder output ordering.",
            Self::LockHashPreservationMismatch => "Check the replacement output lock script and builder output ordering.",
            Self::FieldPreservationMismatch => "Check replacement output fields that should preserve lock/type/data identity.",
            Self::MutateTransitionMismatch => "Check the mutable field delta against the documented transition formula.",
            Self::DataPreservationMismatch => "Check that non-transition output data bytes are copied from the consumed input.",
            Self::DynamicFieldBoundsInvalid => "Validate Molecule table offsets, field count, and dynamic field lengths.",
            Self::TypeHashMismatch => "Check type script hash/hash_type/args and the expected CellScript type identity.",
            Self::FixedByteComparisonUnresolved => "Use schema-backed parameters or fixed-byte values that the verifier can address.",
            Self::ClaimSignatureFailed => "Check the authorization domain, signer public key hash, signature, and target profile.",
            Self::NumericOrDiscriminantInvalid => "Check enum tags, arithmetic bounds, and generated collection length arithmetic.",
            Self::CollectionBoundsInvalid => "Check collection length, index, and capacity values in witness or cell data.",
            Self::ConsumeInvalidOperand => "This indicates an unsupported lowering path; inspect compiler metadata blockers.",
            Self::DestroyInvalidOperand => "This indicates an unsupported lowering path; inspect compiler metadata blockers.",
            Self::CollectionRuntimeUnsupported => {
                "Avoid advertising this collection helper as production-ready until support is implemented."
            }
            Self::EntryWitnessAbiInvalid => {
                "Inspect cellc constraints or cellc abi output for parameter slots and witness byte layout."
            }
            Self::CkbSourceViewInvalid => "Pass a SourceView produced by the matching source::* helper and keep indexes in range.",
            Self::HeaderDepMissing => "Add the required header dep and bind it to the input/deposit whose DAO data is read.",
            Self::DaoFieldMalformed => "Check DAO header bytes, accumulated-rate width, and deposit/withdrawal cell data layout.",
            Self::ScriptRoleMismatch => "Check whether the script is deployed and invoked as the expected lock or type script.",
            Self::XudtBindingMismatch => "Check xUDT type args, owner-mode flags, input type hash, and token data layout.",
            Self::AggregateAmountMismatch => {
                "Compare generated aggregate inputs/outputs and inspect overflow or exact-equality assumptions."
            }
            Self::DynamicFieldValueMismatch => "Check dynamic Molecule field encoding and the value source used by the verifier.",
            Self::OutPointMismatch => "Check the input OutPoint tx hash/index and the expected lineage binding.",
            Self::ScriptFieldMalformed => {
                "Check the lock/type Script Molecule encoding, args length, and whether the cell actually has that script field."
            }
            Self::DaoHeaderLineageMismatch => {
                "Bind the HeaderDep to the exact input/deposit header used for DAO accumulated-rate accounting."
            }
            Self::DaoMaturityViolation => {
                "Check the withdrawal request since value and ensure the consumed DAO input has reached the required maturity."
            }
            Self::CkbSinceMalformed => "Check since flags, metric type, epoch number/index/length bounds, and index < length.",
            Self::ScriptArgsMismatch => "Check lock/type script args and whether this protocol path requires empty script args.",
            Self::MetaPointMismatch => "Check the paired input OutPoints or output indexes and the signed relative-distance field.",
            Self::MetaPointCardinalityMismatch => {
                "Check current-script lock-only/type-only cell counts and ensure every MetaPoint has exactly one paired cell."
            }
            Self::ScriptIdentityMismatch => "Check Script code_hash, hash_type, deployed dep, and whether lock/type role is correct.",
            Self::WitnessMalformed => "Check witness byte layout, WitnessArgs table header, and entry ABI magic bytes.",
            Self::WitnessFieldTruncated => "Check that expected WitnessArgs field offsets plus field lengths stay within loaded witness size.",
        }
    }

    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            1 => Some(Self::SyscallFailed),
            2 => Some(Self::BoundsCheckFailed),
            3 => Some(Self::CellLoadFailed),
            4 => Some(Self::ExactSizeMismatch),
            5 => Some(Self::AssertionFailed),
            7 => Some(Self::LifecycleTransitionMismatch),
            8 => Some(Self::LifecycleNewStateInvalid),
            9 => Some(Self::LifecycleOldStateInvalid),
            10 => Some(Self::EntryWitnessMagicMismatch),
            11 => Some(Self::TypeHashPreservationMismatch),
            12 => Some(Self::LockHashPreservationMismatch),
            13 => Some(Self::FieldPreservationMismatch),
            14 => Some(Self::MutateTransitionMismatch),
            15 => Some(Self::DataPreservationMismatch),
            16 => Some(Self::DynamicFieldBoundsInvalid),
            17 => Some(Self::TypeHashMismatch),
            18 => Some(Self::FixedByteComparisonUnresolved),
            19 => Some(Self::ClaimSignatureFailed),
            20 => Some(Self::NumericOrDiscriminantInvalid),
            21 => Some(Self::CollectionBoundsInvalid),
            22 => Some(Self::ConsumeInvalidOperand),
            23 => Some(Self::DestroyInvalidOperand),
            24 => Some(Self::CollectionRuntimeUnsupported),
            25 => Some(Self::EntryWitnessAbiInvalid),
            26 => Some(Self::CkbSourceViewInvalid),
            27 => Some(Self::HeaderDepMissing),
            28 => Some(Self::DaoFieldMalformed),
            29 => Some(Self::ScriptRoleMismatch),
            30 => Some(Self::XudtBindingMismatch),
            31 => Some(Self::AggregateAmountMismatch),
            32 => Some(Self::DynamicFieldValueMismatch),
            33 => Some(Self::OutPointMismatch),
            34 => Some(Self::ScriptFieldMalformed),
            35 => Some(Self::DaoHeaderLineageMismatch),
            36 => Some(Self::DaoMaturityViolation),
            37 => Some(Self::CkbSinceMalformed),
            38 => Some(Self::ScriptArgsMismatch),
            39 => Some(Self::MetaPointMismatch),
            40 => Some(Self::MetaPointCardinalityMismatch),
            41 => Some(Self::ScriptIdentityMismatch),
            42 => Some(Self::WitnessMalformed),
            43 => Some(Self::WitnessFieldTruncated),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellScriptRuntimeErrorInfo {
    pub code: u64,
    pub name: &'static str,
    pub description: &'static str,
    pub hint: &'static str,
}

pub const ALL_RUNTIME_ERRORS: &[CellScriptRuntimeError] = &[
    CellScriptRuntimeError::SyscallFailed,
    CellScriptRuntimeError::BoundsCheckFailed,
    CellScriptRuntimeError::CellLoadFailed,
    CellScriptRuntimeError::ExactSizeMismatch,
    CellScriptRuntimeError::AssertionFailed,
    CellScriptRuntimeError::LifecycleTransitionMismatch,
    CellScriptRuntimeError::LifecycleNewStateInvalid,
    CellScriptRuntimeError::LifecycleOldStateInvalid,
    CellScriptRuntimeError::EntryWitnessMagicMismatch,
    CellScriptRuntimeError::TypeHashPreservationMismatch,
    CellScriptRuntimeError::LockHashPreservationMismatch,
    CellScriptRuntimeError::FieldPreservationMismatch,
    CellScriptRuntimeError::MutateTransitionMismatch,
    CellScriptRuntimeError::DataPreservationMismatch,
    CellScriptRuntimeError::DynamicFieldBoundsInvalid,
    CellScriptRuntimeError::TypeHashMismatch,
    CellScriptRuntimeError::FixedByteComparisonUnresolved,
    CellScriptRuntimeError::ClaimSignatureFailed,
    CellScriptRuntimeError::NumericOrDiscriminantInvalid,
    CellScriptRuntimeError::CollectionBoundsInvalid,
    CellScriptRuntimeError::ConsumeInvalidOperand,
    CellScriptRuntimeError::DestroyInvalidOperand,
    CellScriptRuntimeError::CollectionRuntimeUnsupported,
    CellScriptRuntimeError::EntryWitnessAbiInvalid,
    CellScriptRuntimeError::CkbSourceViewInvalid,
    CellScriptRuntimeError::HeaderDepMissing,
    CellScriptRuntimeError::DaoFieldMalformed,
    CellScriptRuntimeError::ScriptRoleMismatch,
    CellScriptRuntimeError::XudtBindingMismatch,
    CellScriptRuntimeError::AggregateAmountMismatch,
    CellScriptRuntimeError::DynamicFieldValueMismatch,
    CellScriptRuntimeError::OutPointMismatch,
    CellScriptRuntimeError::ScriptFieldMalformed,
    CellScriptRuntimeError::DaoHeaderLineageMismatch,
    CellScriptRuntimeError::DaoMaturityViolation,
    CellScriptRuntimeError::CkbSinceMalformed,
    CellScriptRuntimeError::ScriptArgsMismatch,
    CellScriptRuntimeError::MetaPointMismatch,
    CellScriptRuntimeError::MetaPointCardinalityMismatch,
    CellScriptRuntimeError::ScriptIdentityMismatch,
    CellScriptRuntimeError::WitnessMalformed,
    CellScriptRuntimeError::WitnessFieldTruncated,
];

pub fn runtime_error_info(error: CellScriptRuntimeError) -> CellScriptRuntimeErrorInfo {
    CellScriptRuntimeErrorInfo { code: error.code(), name: error.name(), description: error.description(), hint: error.hint() }
}

pub fn runtime_error_info_by_code(code: u64) -> Option<CellScriptRuntimeErrorInfo> {
    CellScriptRuntimeError::from_code(code).map(runtime_error_info)
}

pub fn runtime_error_info_by_name(name: &str) -> Option<CellScriptRuntimeErrorInfo> {
    ALL_RUNTIME_ERRORS.iter().copied().find(|error| error.name() == name).map(runtime_error_info)
}

pub fn runtime_error_info_for_diagnostic_message(message: &str) -> Option<CellScriptRuntimeErrorInfo> {
    if let Some(info) = ALL_RUNTIME_ERRORS.iter().copied().map(runtime_error_info).find(|info| message.contains(info.name)) {
        return Some(info);
    }

    if message.contains("fixed-byte-comparison") {
        return Some(runtime_error_info(CellScriptRuntimeError::FixedByteComparisonUnresolved));
    }
    if message.contains("collection-") || message.contains("cell-backed collection") {
        return Some(runtime_error_info(CellScriptRuntimeError::CollectionRuntimeUnsupported));
    }
    if message.contains("entry witness") || message.contains("entry-witness") {
        return Some(runtime_error_info(CellScriptRuntimeError::EntryWitnessAbiInvalid));
    }
    if message.contains("mutate-field-transition") {
        return Some(runtime_error_info(CellScriptRuntimeError::MutateTransitionMismatch));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn runtime_error_registry_roundtrips_and_has_unique_codes() {
        let mut codes = BTreeSet::new();
        let mut names = BTreeSet::new();
        for error in ALL_RUNTIME_ERRORS {
            let info = runtime_error_info(*error);
            assert_eq!(CellScriptRuntimeError::from_code(info.code), Some(*error));
            assert_eq!(runtime_error_info_by_code(info.code), Some(info));
            assert_eq!(runtime_error_info_by_name(info.name), Some(info));
            assert!(!info.name.is_empty());
            assert!(!info.description.is_empty());
            assert!(!info.hint.is_empty());
            assert!(codes.insert(info.code), "duplicate runtime error code {}", info.code);
            assert!(names.insert(info.name), "duplicate runtime error name {}", info.name);
        }
    }

    #[test]
    fn diagnostic_messages_map_to_runtime_error_codes_where_possible() {
        assert_eq!(
            runtime_error_info_for_diagnostic_message("fail-closed runtime features: collection-push").map(|info| info.code),
            Some(24)
        );
        assert_eq!(runtime_error_info_for_diagnostic_message("fixed-byte-comparison unresolved").map(|info| info.code), Some(18));
        assert_eq!(runtime_error_info_for_diagnostic_message("ordinary type mismatch").map(|info| info.code), None);
    }

    #[test]
    fn runtime_error_docs_cover_every_registered_code() {
        let docs = include_str!("../docs/CELLSCRIPT_RUNTIME_ERROR_CODES.md");
        for error in ALL_RUNTIME_ERRORS {
            let info = runtime_error_info(*error);
            assert!(docs.contains(&format!("| {} |", info.code)), "docs missing code {}", info.code);
            assert!(docs.contains(info.name), "docs missing runtime error name {}", info.name);
        }
    }

    #[test]
    fn codegen_does_not_emit_unregistered_numeric_fail_literals() {
        let codegen = include_str!("codegen/mod.rs");
        for code in 1..=64 {
            assert!(
                !codegen.contains(&format!("emit_fail({})", code)),
                "codegen must use CellScriptRuntimeError instead of emit_fail({})",
                code
            );
            assert!(
                !codegen.contains(&format!("emit_return_on_syscall_error({})", code)),
                "codegen must use CellScriptRuntimeError instead of emit_return_on_syscall_error({})",
                code
            );
        }
    }
}
