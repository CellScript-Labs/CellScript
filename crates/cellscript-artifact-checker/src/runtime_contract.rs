//! Closed runtime helper descriptions shared by emission, inference and checking.
//! Descriptions specify ABI/effect facts; machine evidence is verified separately.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeReturnAbi {
    /// a0 is a scalar value; no error status is returned.
    ScalarValue,
    /// a0 is a scalar on success; errors terminate inside the callee.
    ScalarOrAbort,
    /// a0 is a value, a1 is zero on normal return; failures terminate internally.
    ScalarZeroStatusOrAbort,
    /// a0 is a scalar only when a1 == 0. Every caller must check a1.
    ScalarStatusA1,
    /// a0 is a status. Some EXEC paths only return on failure.
    StatusA0,
    /// Explicit status-valued API/internal predicate; a dedicated caller interprets a0.
    ReturnedStatusA0,
    /// Caller-owned output buffers plus a0 status; requires a dedicated caller.
    BufferStatusA0,
    /// Neither success nor failure returns to the caller.
    NoReturn,
    /// Memory-only operation; caller-saved result registers have no value contract.
    Void,
    /// a0 is status and a1 is an output value (e.g. child process id).
    StatusA0ValueA1,
    /// a0 is status, a1/a2 are output values (e.g. pipe descriptors).
    StatusA0ValuesA1A2,
    /// Witness resolver: a0..a3 are offset/length/index/source, a4 is status.
    ValuesA0ToA3StatusA4,
}

/// Availability is separate from semantic effects and is explicit per helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredFeature {
    CkbSighashAll,
    CkbSinceConstraints,
    CkbOccupiedCapacityFloor,
}
impl DeferredFeature {
    pub const fn feature(self) -> &'static str {
        match self {
            Self::CkbSighashAll => "ckb-sighash-all-deferred",
            Self::CkbSinceConstraints => "ckb-since-constraints-deferred",
            Self::CkbOccupiedCapacityFloor => "ckb-occupied-capacity-deferred",
        }
    }
    pub fn failure(self) -> crate::TypedSemanticRuntimeError {
        let (code, name) = match self {
            Self::CkbSighashAll => (66, "sighash-all-unsupported"),
            Self::CkbSinceConstraints => (86, "since-constraints-unsupported"),
            Self::CkbOccupiedCapacityFloor => (87, "occupied-capacity-unsupported"),
        };
        crate::TypedSemanticRuntimeError { code, name: name.into() }
    }
}

/// Executable availability and exact unconditional failure are separate from
/// effects. One registry row is authoritative for all consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAvailability {
    Implemented,
    Deferred(DeferredFeature),
    Fails(KnownRuntimeFailure),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownRuntimeFailure {
    CollectionRuntimeUnsupported,
}
impl KnownRuntimeFailure {
    pub fn failure(self) -> crate::TypedSemanticRuntimeError {
        match self {
            Self::CollectionRuntimeUnsupported => {
                crate::TypedSemanticRuntimeError { code: 24, name: "collection-runtime-unsupported".into() }
            }
        }
    }
}
impl RuntimeAvailability {
    pub fn failure(self) -> Option<crate::TypedSemanticRuntimeError> {
        match self {
            Self::Implemented => None,
            Self::Deferred(feature) => Some(feature.failure()),
            Self::Fails(failure) => Some(failure.failure()),
        }
    }
}

macro_rules! helpers {
    (surface { $( $variant:ident => ($symbol:literal, $detail:literal, $abi:ident, $availability:expr) ),* $(,)? }
     additional { $( $other:ident => ($other_symbol:literal, $other_abi:ident, $other_availability:expr) ),* $(,)? }
     ipc { $( $ipc:ident => ($ipc_symbol:literal, $ipc_abi:ident, $ipc_availability:expr) ),* $(,)? }
     inline { $( $inline:ident => ($inline_symbol:literal, $inline_abi:ident, $inline_availability:expr) ),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum RuntimeHelper { $( $variant, )* $( $other, )* $( $ipc, )* $( $inline, )* }
        impl RuntimeHelper {
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )* $( Self::$other, )* $( Self::$ipc, )* $( Self::$inline, )* ];
            pub fn from_symbol(symbol: &str) -> Option<Self> {
                match symbol { $( $symbol => Some(Self::$variant), )* $( $other_symbol => Some(Self::$other), )* $( $ipc_symbol => Some(Self::$ipc), )* $( $inline_symbol => Some(Self::$inline), )* _ => None }
            }
            pub const fn symbol(self) -> &'static str {
                match self { $( Self::$variant => $symbol, )* $( Self::$other => $other_symbol, )* $( Self::$ipc => $ipc_symbol, )* $( Self::$inline => $inline_symbol, )* }
            }
            pub const fn availability(self) -> RuntimeAvailability {
                match self { $( Self::$variant => $availability, )* $( Self::$other => $other_availability, )* $( Self::$ipc => $ipc_availability, )* $( Self::$inline => $inline_availability, )* }
            }
            pub const fn deferred(self) -> Option<DeferredFeature> {
                match self.availability() { RuntimeAvailability::Deferred(feature)=>Some(feature),RuntimeAvailability::Implemented|RuntimeAvailability::Fails(_)=>None }
            }
            pub fn failure(self) -> Option<crate::TypedSemanticRuntimeError> { self.availability().failure() }
            pub const fn has_standalone_symbol(self) -> bool {
                match self { $( Self::$variant => true, )* $( Self::$other => true, )* $( Self::$ipc => true, )* $( Self::$inline => false, )* }
            }
            pub const fn return_abi(self) -> RuntimeReturnAbi {
                match self { $( Self::$variant => RuntimeReturnAbi::$abi, )* $( Self::$other => RuntimeReturnAbi::$other_abi, )* $( Self::$ipc => RuntimeReturnAbi::$ipc_abi, )* $( Self::$inline => RuntimeReturnAbi::$inline_abi, )* }
            }
        }
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum IpcHelper { $( $ipc, )* }
        impl IpcHelper {
            pub const ALL: &'static [Self] = &[ $( Self::$ipc, )* ];
            pub const fn helper(self) -> RuntimeHelper { match self { $( Self::$ipc => RuntimeHelper::$ipc, )* } }
            pub const fn symbol(self) -> &'static str { self.helper().symbol() }
        }
        /// The surface dispatcher must match every variant without a fallback.
        /// ```compile_fail
        /// use cellscript_artifact_checker::runtime_contract::SurfaceHelper;
        /// fn incomplete(helper: SurfaceHelper) {
        ///     match helper { SurfaceHelper::CkbRequireTime => () }
        /// }
        /// ```
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum SurfaceHelper { $( $variant, )* }
        impl SurfaceHelper {
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )* ];
            pub const fn helper(self) -> RuntimeHelper { match self { $( Self::$variant => RuntimeHelper::$variant, )* } }
            pub const fn symbol(self) -> &'static str { self.helper().symbol() }
            pub const fn detail(self) -> &'static str { match self { $( Self::$variant => $detail, )* } }
        }

    }
}
helpers! {
    surface {
        CkbExecCellDepU8Args => ("__ckb_exec_cell_dep_u8_args", "non-returning EXEC of a full CellDep ELF with zero to four single-byte C-string arguments", StatusA0, RuntimeAvailability::Implemented),
        CkbExecCellDepHex4 => ("__ckb_exec_cell_dep_hex4", "non-returning EXEC with four hex-encoded local byte-vector segments", StatusA0, RuntimeAvailability::Implemented),
        CkbTransactionU32Le => ("__ckb_transaction_u32_le", "exact complete packed Transaction u32 read", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbWitnessBytes32 => ("__ckb_witness_bytes32", "exact 32 raw witness bytes as [u8; 32], without hashing", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbTransactionBlake2bGather => ("__ckb_transaction_blake2b_gather", "CKB hash over ordered transaction spans with local prefix/suffix", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbWitnessBlake2bSelectChunks => ("__ckb_witness_blake2b_select_chunks", "CKB hash over selected fixed-width witness chunks with local prefix/suffix", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbSpawnWaitCellDepHex4 => ("__ckb_spawn_wait_cell_dep_hex4", "checked returning SPAWN and WAIT with four hexadecimal arguments", StatusA0, RuntimeAvailability::Implemented),
        CkbCurrentRole => ("__ckb_current_role", "current script role inferred from group input lock/type hashes", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCurrentScriptHash => ("__ckb_current_script_hash", "current script hash loaded via LOAD_SCRIPT_HASH", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbTransactionHash => ("__ckb_transaction_hash", "canonical raw transaction hash loaded via LOAD_TX_HASH", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbScriptHash => ("__ckb_script_hash", "canonical bounded Molecule Script CKB Blake2b-256", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbSinceToRaw => ("__ckb_since_to_raw", "explicit typed Since to raw CKB wire bits conversion", ScalarValue, RuntimeAvailability::Implemented),
        CkbEpochNumberToU64 => ("__ckb_epoch_number_to_u64", "explicit EpochNumber to u64 conversion", ScalarValue, RuntimeAvailability::Implemented),
        CkbEpochDurationToU64 => ("__ckb_epoch_duration_to_u64", "explicit EpochDuration to u64 conversion", ScalarValue, RuntimeAvailability::Implemented),
        CkbBlockNumberToU64 => ("__ckb_block_number_to_u64", "explicit BlockNumber to u64 conversion", ScalarValue, RuntimeAvailability::Implemented),
        CkbEpochLengthToU64 => ("__ckb_epoch_length_to_u64", "explicit EpochLength to u64 conversion", ScalarValue, RuntimeAvailability::Implemented),
        CkbTimestampMillisToU64 => ("__ckb_timestamp_millis_to_u64", "explicit TimestampMillis to u64 conversion", ScalarValue, RuntimeAvailability::Implemented),
        CkbCellCapacity => ("__ckb_cell_capacity", "SourceView cell capacity field", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellOccupiedCapacity => ("__ckb_cell_occupied_capacity", "SourceView occupied capacity from CellOutput scripts and data bytes", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellUnoccupiedCapacity => ("__ckb_cell_unoccupied_capacity", "SourceView capacity minus occupied capacity", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellOutputIndex => ("__ckb_cell_output_index", "SourceView output index", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbInputOutPointIndex => ("__ckb_input_out_point_index", "SourceView input OutPoint index", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbInputOutPointTxHashLow => ("__ckb_input_out_point_tx_hash_low", "SourceView input OutPoint tx hash low word", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbInputOutPointTxHash => ("__ckb_input_out_point_tx_hash", "SourceView input OutPoint full tx hash read", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbRequireInputOutPointTxHash => ("__ckb_require_input_out_point_tx_hash", "SourceView input OutPoint full tx-hash binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireInputOutPoint => ("__ckb_require_input_out_point", "SourceView input OutPoint full tx-hash and index binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireMetapointRelative => ("__ckb_require_metapoint_relative", "SourceView MetaPoint relative-distance binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireLockTypeMetapointPairs => ("__ckb_require_lock_type_metapoint_pairs", "current-script lock-only to type-only MetaPoint pair cardinality check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireTypeLockMetapointPairs => ("__ckb_require_type_lock_metapoint_pairs", "current-script type-only to lock-only MetaPoint pair cardinality check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireLockTypeMetapointPairsFromI32Data => ("__ckb_require_lock_type_metapoint_pairs_from_i32_data", "current-script lock-only to type-only MetaPoint pair cardinality check using signed i32 distance loaded from base cell data", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireTypeLockMetapointPairsFromI32Data => ("__ckb_require_type_lock_metapoint_pairs_from_i32_data", "current-script type-only to lock-only MetaPoint pair cardinality check using signed i32 distance loaded from base cell data", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireLockTypeMetapointPairsFromI32DataFiltered => ("__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered", "current-script lock-only to type-only filtered MetaPoint pair cardinality check using signed i32 distance loaded from base cell data", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireTypeLockMetapointPairsFromI32DataFiltered => ("__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered", "current-script type-only to lock-only filtered MetaPoint pair cardinality check using signed i32 distance loaded from base cell data", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireLockMatchMasterOutPointPairsFromData => ("__ckb_require_lock_match_master_out_point_pairs_from_data", "current-script lock-only match order input/output pairing using master OutPoint loaded from order data", StatusA0, RuntimeAvailability::Implemented),
        CkbCellLockHashLow => ("__ckb_cell_lock_hash_low", "SourceView lock hash low word", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellTypeHashLow => ("__ckb_cell_type_hash_low", "SourceView type hash low word", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellLockHash => ("__ckb_cell_lock_hash", "SourceView lock hash full 32-byte read", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbCellTypeHash => ("__ckb_cell_type_hash", "SourceView type hash full 32-byte read", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbCellDataHashField => ("__ckb_cell_data_hash_field", "SourceView consensus DATA_HASH field, all 32 bytes", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbCellDataBytes32 => ("__ckb_cell_data_bytes32", "SourceView cell data 32-byte read at byte offset", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbCellDataBlake2bSpan => ("__ckb_cell_data_blake2b_span", "exact Cell data interval CKB Blake2b-256", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbWitnessBlake2bSpan => ("__ckb_witness_blake2b_span", "exact raw witness interval CKB Blake2b-256", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbRawTransactionHashWithoutCellDeps => ("__ckb_raw_transaction_hash_without_cell_deps", "canonical RawTransaction hash with empty CellDepVec", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbCellLockCodeHash => ("__ckb_cell_lock_code_hash", "SourceView lock Script code_hash read", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbCellTypeCodeHash => ("__ckb_cell_type_code_hash", "SourceView type Script code_hash read", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbCellLockHashType => ("__ckb_cell_lock_hash_type", "SourceView lock Script hash_type read", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellTypeHashType => ("__ckb_cell_type_hash_type", "SourceView type Script hash_type read", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellLockArgsEmpty => ("__ckb_cell_lock_args_empty", "SourceView lock Script args_empty read", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellTypeArgsEmpty => ("__ckb_cell_type_args_empty", "SourceView type Script args_empty read", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellLockArgs32 => ("__ckb_cell_lock_args32", "SourceView lock Script 32-byte args read", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbCellTypeArgs32 => ("__ckb_cell_type_args32", "SourceView type Script 32-byte args read", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockHash => ("__ckb_require_cell_lock_hash", "SourceView lock hash full 32-byte binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeHash => ("__ckb_require_cell_type_hash", "SourceView type hash full 32-byte binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockExactHandle => ("__ckb_require_cell_lock_exact_handle", "CSHDLv1 full-value commitment plus complete SourceView Lock Script identity binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeExactHandle => ("__ckb_require_cell_type_exact_handle", "CSHDLv1 full-value commitment plus complete SourceView Type Script identity binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellDepExactVerifierHandle => ("__ckb_require_cell_dep_exact_verifier_handle", "CSHDLv1 full-value commitment plus exact CellDep artifact data-hash binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellDepOpenScriptHandle => ("__ckb_require_cell_dep_open_script_handle", "CSOHWv1 structural, membership and selected Script CellDep binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellDepOpenVerifierHandle => ("__ckb_require_cell_dep_open_verifier_handle", "CSOHWv1 structural, membership and selected verifier CellDep binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellDepOpenScriptHandleFromArgs => ("__ckb_require_cell_dep_open_script_handle_from_args", "CSOHWv1 membership with the expected root derived from the current Script's committed 32-byte args", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireOpenScriptParticipantFromArgs => ("__ckb_require_open_script_participant_from_args", "CSOHWv1 committed-root membership, exact code dependency and observed participant Script identity", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellDepOpenVerifierHandleFromArgs => ("__ckb_require_cell_dep_open_verifier_handle_from_args", "CSOHWv1 verifier membership with the expected root derived from the current Script's committed 32-byte args", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockDeploymentLineHandle => ("__ckb_require_cell_lock_deployment_line_handle", "active CSLINv1 admission plus exact Lock Script and Type-hash code CellDep binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeDeploymentLineHandle => ("__ckb_require_cell_type_deployment_line_handle", "active CSLINv1 admission plus exact Type Script and Type-hash code CellDep binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellDepDeploymentLineVerifierHandle => ("__ckb_require_cell_dep_deployment_line_verifier_handle", "active CSLINv1 admission plus exact spawned-verifier Type-hash code CellDep binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellDataHash => ("__ckb_require_cell_data_hash", "SourceView data hash full 32-byte binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireBoundedCellDepDataHash => ("__ckb_require_bounded_cell_dep_data_hash", "bounded resolved CellDep data-hash membership check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCurrentScriptArgsEmpty => ("__ckb_require_current_script_args_empty", "current Script empty args requirement", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockArgsEmpty => ("__ckb_require_cell_lock_args_empty", "SourceView lock Script empty args requirement", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeArgsEmpty => ("__ckb_require_cell_type_args_empty", "SourceView type Script empty args requirement", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockArgs32 => ("__ckb_require_cell_lock_args32", "SourceView lock Script 32-byte args binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeArgs32 => ("__ckb_require_cell_type_args32", "SourceView type Script 32-byte args binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockArgsExact => ("__ckb_require_cell_lock_args_exact", "SourceView lock Script arbitrary exact args binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeArgsExact => ("__ckb_require_cell_type_args_exact", "SourceView type Script arbitrary exact args binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockArgsPrefix32 => ("__ckb_require_cell_lock_args_prefix32", "SourceView lock Script 32-byte args prefix binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeArgsPrefix32 => ("__ckb_require_cell_type_args_prefix32", "SourceView type Script 32-byte args prefix binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockArgsSuffix32 => ("__ckb_require_cell_lock_args_suffix32", "SourceView lock Script 32-byte args suffix binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeArgsSuffix32 => ("__ckb_require_cell_type_args_suffix32", "SourceView type Script 32-byte args suffix binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellLockScriptHashType => ("__ckb_require_cell_lock_script_hash_type", "SourceView lock Script code_hash/hash_type binding check", StatusA0, RuntimeAvailability::Implemented),
        CkbRequireCellTypeScriptHashType => ("__ckb_require_cell_type_script_hash_type", "SourceView type Script code_hash/hash_type binding check", StatusA0, RuntimeAvailability::Implemented),
        C256RequireU128ProductLte => ("__c256_require_u128_product_lte", "C256 u128 product <= requirement", StatusA0, RuntimeAvailability::Implemented),
        C256RequireU128ProductEq => ("__c256_require_u128_product_eq", "C256 u128 product == requirement", StatusA0, RuntimeAvailability::Implemented),
        C256RequireU128Sum2ProductsLte => ("__c256_require_u128_sum2_products_lte", "C256 u128 product-sum <= requirement", StatusA0, RuntimeAvailability::Implemented),
        C256RequireU128Sum2ProductsEq => ("__c256_require_u128_sum2_products_eq", "C256 u128 product-sum == requirement", StatusA0, RuntimeAvailability::Implemented),
        CkbCellDataSize => ("__ckb_cell_data_size", "SourceView cell data byte length", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellDataEqual => ("__ckb_cell_data_equal", "exact equality of two complete SourceView Cell data payloads", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSourceBytesEqual => ("__ckb_source_bytes_equal", "exact equality of two bounded byte ranges from CKB syscall sources", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSourceBytesEqualMemory => ("__ckb_source_bytes_equal_memory", "exact equality of a bounded CKB source range and fixed in-memory bytes", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSourceBytesZero => ("__ckb_source_bytes_zero", "exact all-zero check over a bounded CKB source range", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbCellCount => ("__ckb_cell_count", "SourceView complete cell-source cardinality", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellHasType => ("__ckb_cell_has_type", "SourceView optional Type Script presence", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellDataU32Le => ("__ckb_cell_data_u32_le", "SourceView cell data little-endian u32 read", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellDataU64Le => ("__ckb_cell_data_u64_le", "SourceView cell data little-endian u64 read", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        DaoAccumulatedRate => ("__dao_accumulated_rate", "DAO accumulated rate from HeaderDep SourceView", ScalarStatusA1, RuntimeAvailability::Implemented),
        DaoInputAccumulatedRate => ("__dao_input_accumulated_rate", "DAO accumulated rate from an Input/GroupInput committed header", ScalarStatusA1, RuntimeAvailability::Implemented),
        DaoHasDaoType => ("__dao_has_dao_type", "DAO type hash classifier", ScalarStatusA1, RuntimeAvailability::Implemented),
        DaoIsDepositData => ("__dao_is_deposit_data", "DAO deposit data classifier", ScalarStatusA1, RuntimeAvailability::Implemented),
        DaoIsWithdrawalRequestData => ("__dao_is_withdrawal_request_data", "DAO withdrawal request data classifier", ScalarStatusA1, RuntimeAvailability::Implemented),
        DaoRequireHeaderDepForInput => ("__dao_require_header_dep_for_input", "DAO input header to HeaderDep lineage requirement", StatusA0, RuntimeAvailability::Implemented),
        DaoRequireInputSinceAtLeast => ("__dao_require_input_since_at_least", "DAO input since lower-bound requirement", StatusA0, RuntimeAvailability::Implemented),
        DaoRequireInputRelativeEpochSinceAtLeast => ("__dao_require_input_relative_epoch_since_at_least", "DAO relative epoch since maturity requirement", StatusA0, RuntimeAvailability::Implemented),
        XudtAmountLow => ("__xudt_amount_low", "xUDT amount low 64 bits", ScalarStatusA1, RuntimeAvailability::Implemented),
        XudtAmountHigh => ("__xudt_amount_high", "xUDT amount high 64 bits", ScalarStatusA1, RuntimeAvailability::Implemented),
        XudtOwnerModeInputTypeHash => ("__xudt_owner_mode_input_type_hash", "xUDT owner-mode input-type hash low word", ScalarStatusA1, RuntimeAvailability::Implemented),
        XudtRequireOwnerModeInputType => ("__xudt_require_owner_mode_input_type", "xUDT owner-mode input-type binding check", StatusA0, RuntimeAvailability::Implemented),
        XudtRequireOwnerModeTypeArgs => ("__xudt_require_owner_mode_type_args", "xUDT owner-mode type args binding check", StatusA0, RuntimeAvailability::Implemented),
        XudtRequireOwnerModeTypeArgsCurrentScript => ("__xudt_require_owner_mode_type_args_current_script", "xUDT owner-mode type args binding check against current script hash", StatusA0, RuntimeAvailability::Implemented),
        CellscriptRequireFungibleTypeGroupV1 => ("__cellscript_require_fungible_type_group_v1", "chain-neutral fungible type-group v1 conservation", StatusA0, RuntimeAvailability::Implemented),
        XudtRequireGroupAmountConserved => ("__xudt_require_group_amount_conserved", "xUDT group input/output amount conservation", StatusA0, RuntimeAvailability::Implemented),
        XudtRequireGroupAmountMinted => ("__xudt_require_group_amount_minted", "xUDT group output-input amount delta check", StatusA0, RuntimeAvailability::Implemented),
        XudtRequireGroupAmountBurned => ("__xudt_require_group_amount_burned", "xUDT group input-output amount delta check", StatusA0, RuntimeAvailability::Implemented),
        CkbWitnessLockExact32 => ("__ckb_witness_lock_exact32", "exact 32-byte WitnessArgs.lock", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbWitnessInputTypeExact32 => ("__ckb_witness_input_type_exact32", "exact 32-byte WitnessArgs.input_type", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbWitnessOutputTypeExact32 => ("__ckb_witness_output_type_exact32", "exact 32-byte WitnessArgs.output_type", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbWitnessBoundedSize => ("__ckb_witness_bounded_size", "bounded raw witness or owned WitnessArgs field size", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbWitnessBoundedU8 => ("__ckb_witness_bounded_u8", "bounded raw witness or owned WitnessArgs field byte", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbWitnessBoundedU32Le => ("__ckb_witness_bounded_u32_le", "bounded raw witness or owned WitnessArgs field u32", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbWitnessBoundedU64Le => ("__ckb_witness_bounded_u64_le", "bounded raw witness or owned WitnessArgs field u64", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbWitnessBoundedBlake2b => ("__ckb_witness_bounded_blake2b", "bounded raw witness or owned WitnessArgs field CKB Blake2b-256", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbWitnessSize => ("__ckb_witness_size", "witness byte size", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbWitnessCount => ("__ckb_witness_count", "complete transaction witness count", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbWitnessU8 => ("__ckb_witness_u8", "exact raw witness byte", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbWitnessU32Le => ("__ckb_witness_u32_le", "exact raw witness little-endian word", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbWitnessU64Le => ("__ckb_witness_u64_le", "exact raw witness little-endian doubleword", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellDataU8 => ("__ckb_cell_data_u8", "exact Cell data byte", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellLockSize => ("__ckb_cell_lock_size", "complete serialized Cell Lock Script size", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellTypeSize => ("__ckb_cell_type_size", "complete serialized Cell Type Script size", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellLockU8 => ("__ckb_cell_lock_u8", "exact serialized Cell Lock Script byte", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbCellTypeU8 => ("__ckb_cell_type_u8", "exact serialized Cell Type Script byte", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        CkbInputSinceAt => ("__ckb_input_since_at", "source-selected raw Input since", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbRequireWitnessSizeAtLeast => ("__ckb_require_witness_size_at_least", "require witness size lower bound", StatusA0, RuntimeAvailability::Implemented),
        CkbSighashAll => ("__ckb_sighash_all", "CKB sighash-all digest", NoReturn, RuntimeAvailability::Deferred(DeferredFeature::CkbSighashAll)),
        CkbSighashAllZeroLock => ("__ckb_sighash_all_zero_lock", "bounded CKB sighash-all zero-lock message", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbRequireMaturity => ("__ckb_require_maturity", "CKB block-number since maturity", NoReturn, RuntimeAvailability::Deferred(DeferredFeature::CkbSinceConstraints)),
        CkbRequireTime => ("__ckb_require_time", "CKB timestamp since", NoReturn, RuntimeAvailability::Deferred(DeferredFeature::CkbSinceConstraints)),
        CkbRequireEpochAfter => ("__ckb_require_epoch_after", "CKB absolute epoch since", NoReturn, RuntimeAvailability::Deferred(DeferredFeature::CkbSinceConstraints)),
        CkbRequireEpochRelative => ("__ckb_require_epoch_relative", "CKB relative epoch since", NoReturn, RuntimeAvailability::Deferred(DeferredFeature::CkbSinceConstraints)),
        CkbOccupiedCapacity => ("__ckb_occupied_capacity", "compile-visible occupied capacity floor", NoReturn, RuntimeAvailability::Deferred(DeferredFeature::CkbOccupiedCapacityFloor)),
    }
    additional {
        CellscriptAbort => ("__cellscript_abort", NoReturn, RuntimeAvailability::Implemented),
        CellscriptBlake2bCompress => ("__cellscript_blake2b_compress", Void, RuntimeAvailability::Implemented),
        CellscriptMulU128ToU256 => ("__cellscript_mul_u128_to_u256", Void, RuntimeAvailability::Implemented),
        CellscriptLoadU64Le => ("__cellscript_load_u64_le", ScalarValue, RuntimeAvailability::Implemented),
        CellscriptMemcmpFixed => ("__cellscript_memcmp_fixed", ScalarValue, RuntimeAvailability::Implemented),
        CellscriptMemzeroFixed => ("__cellscript_memzero_fixed", ScalarValue, RuntimeAvailability::Implemented),
        CellscriptAddU256 => ("__cellscript_add_u256", ScalarValue, RuntimeAvailability::Implemented),
        CellscriptExactReadCached => ("__cellscript_exact_read_cached", ScalarZeroStatusOrAbort, RuntimeAvailability::Implemented),
        EnvCurrentTimepoint => ("__env_current_timepoint", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbInputSince => ("__ckb_input_since", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbHeaderEpochNumber => ("__ckb_header_epoch_number", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbHeaderEpochStartBlockNumber => ("__ckb_header_epoch_start_block_number", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbHeaderEpochLength => ("__ckb_header_epoch_length", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbHeaderDepEpochNumber => ("__ckb_header_dep_epoch_number", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbHeaderDepEpochStartBlockNumber => ("__ckb_header_dep_epoch_start_block_number", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbHeaderDepEpochLength => ("__ckb_header_dep_epoch_length", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbHeaderDepBlockNumber => ("__ckb_header_dep_block_number", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbHeaderDepTimestampMillis => ("__ckb_header_dep_timestamp_millis", ScalarOrAbort, RuntimeAvailability::Implemented),
        CellscriptCurrentScriptRoleAt => ("__cellscript_current_script_role_at", ScalarStatusA1, RuntimeAvailability::Implemented),
        CellscriptRequireCellMembership => ("__cellscript_require_cell_membership", ReturnedStatusA0, RuntimeAvailability::Implemented),
        CellscriptRequireExactSize => ("__cellscript_require_exact_size", ReturnedStatusA0, RuntimeAvailability::Implemented),
        CellscriptRequireMinSize => ("__cellscript_require_min_size", ReturnedStatusA0, RuntimeAvailability::Implemented),
        CkbRequireSha256dMerkleRoot => ("__ckb_require_sha256d_merkle_root", StatusA0, RuntimeAvailability::Implemented),
        CellscriptMemcpyFixed => ("__cellscript_memcpy_fixed", BufferStatusA0, RuntimeAvailability::Implemented),
        CellscriptSha256Compress => ("__cellscript_sha256_compress", BufferStatusA0, RuntimeAvailability::Implemented),
        CellscriptSha256Fixed => ("__cellscript_sha256_fixed", BufferStatusA0, RuntimeAvailability::Implemented),
        CellscriptBlake2bPrefixedTransactionSpan => ("__cellscript_blake2b_prefixed_transaction_span", BufferStatusA0, RuntimeAvailability::Implemented),
        CellscriptBlake2bSegments => ("__cellscript_blake2b_segments", BufferStatusA0, RuntimeAvailability::Implemented),
        CellscriptBlake2bTransactionSpan => ("__cellscript_blake2b_transaction_span", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashBlake2b => ("__ckb_hash_blake2b", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashBlake2bVar => ("__ckb_hash_blake2b_var", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashChain => ("__ckb_hash_chain", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashPair => ("__ckb_hash_pair", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashSha256 => ("__ckb_hash_sha256", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashSha256Pair => ("__ckb_hash_sha256_pair", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashSha256d => ("__ckb_hash_sha256d", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashSha256dPair => ("__ckb_hash_sha256d_pair", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbSpawnWithFd1 => ("__ckb_spawn_with_fd1", StatusA0ValueA1, RuntimeAvailability::Implemented),
        CellscriptWitnessBoundedResolve => ("__cellscript_witness_bounded_resolve", ValuesA0ToA3StatusA4, RuntimeAvailability::Implemented),
        CkbEpochAdd => ("__ckb_epoch_add", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbEpochDuration => ("__ckb_epoch_duration", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbEpochSub => ("__ckb_epoch_sub", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbRequireInstanceScriptParticipantFromArgs => ("__ckb_require_instance_script_participant_from_args", StatusA0, RuntimeAvailability::Implemented),
        CkbSinceAsAbsoluteBlock => ("__ckb_since_as_absolute_block", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceAsAbsoluteEpoch => ("__ckb_since_as_absolute_epoch", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceAsAbsoluteTimestamp => ("__ckb_since_as_absolute_timestamp", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceAsRelativeBlock => ("__ckb_since_as_relative_block", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceAsRelativeEpoch => ("__ckb_since_as_relative_epoch", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceAsRelativeTimestamp => ("__ckb_since_as_relative_timestamp", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceBlockAbsolute => ("__ckb_since_block_absolute", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceBlockRelative => ("__ckb_since_block_relative", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceDecode => ("__ckb_since_decode", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceEpochAbsolute => ("__ckb_since_epoch_absolute", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceEpochRelative => ("__ckb_since_epoch_relative", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceFromRawChecked => ("__ckb_since_from_raw_checked", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceIsDisabled => ("__ckb_since_is_disabled", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceIsRelative => ("__ckb_since_is_relative", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceMetric => ("__ckb_since_metric", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceTimestampAbsolute => ("__ckb_since_timestamp_absolute", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceTimestampRelative => ("__ckb_since_timestamp_relative", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSinceValue => ("__ckb_since_value", ScalarStatusA1, RuntimeAvailability::Implemented),
        CkbSourceCellDep => ("__ckb_source_cell_dep", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbSourceGroupInput => ("__ckb_source_group_input", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbSourceGroupOutput => ("__ckb_source_group_output", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbSourceHeaderDep => ("__ckb_source_header_dep", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbSourceInput => ("__ckb_source_input", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbSourceOutput => ("__ckb_source_output", ScalarOrAbort, RuntimeAvailability::Implemented),
    }
    ipc {
        CkbWait => ("__ckb_wait", ReturnedStatusA0, RuntimeAvailability::Implemented),
        CkbProcessId => ("__ckb_process_id", ScalarValue, RuntimeAvailability::Implemented),
        CkbPipe => ("__ckb_pipe", StatusA0ValuesA1A2, RuntimeAvailability::Implemented),
        CkbPipeWrite => ("__ckb_pipe_write", ReturnedStatusA0, RuntimeAvailability::Implemented),
        CkbPipeRead => ("__ckb_pipe_read", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbInheritedFd => ("__ckb_inherited_fd", ScalarOrAbort, RuntimeAvailability::Implemented),
        CkbClose => ("__ckb_close", ReturnedStatusA0, RuntimeAvailability::Implemented),
    }
    inline {
        CkbCommitmentOpen => ("__ckb_commitment_open", Void, RuntimeAvailability::Implemented),
        ZkRequireTransition => ("__zk_require_transition", Void, RuntimeAvailability::Implemented),
        Bip340RequireSignature => ("__novaseal_bip340_require_signature", Void, RuntimeAvailability::Implemented),
        Bip340RequireSignatureFromCellDep => ("__novaseal_bip340_require_signature_from_cell_dep", Void, RuntimeAvailability::Implemented),
        BoundedConsumeEachFailClosed => ("__cellscript_bounded_consume_each_fail_closed", NoReturn, RuntimeAvailability::Fails(KnownRuntimeFailure::CollectionRuntimeUnsupported)),
        BoundedCreateEachFailClosed => ("__cellscript_bounded_create_each_fail_closed", NoReturn, RuntimeAvailability::Fails(KnownRuntimeFailure::CollectionRuntimeUnsupported)),
        CkbHashBlake2bPacked => ("__ckb_hash_blake2b_packed", BufferStatusA0, RuntimeAvailability::Implemented),
        CkbHashDataPacked => ("__ckb_hash_data_packed", BufferStatusA0, RuntimeAvailability::Implemented),
    }
}

pub fn return_abi(symbol: &str) -> Option<RuntimeReturnAbi> {
    RuntimeHelper::from_symbol(symbol).map(RuntimeHelper::return_abi)
}
