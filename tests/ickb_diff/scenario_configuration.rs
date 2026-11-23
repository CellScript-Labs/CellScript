#[derive(Debug)]
struct DepositPhase1SideRun {
    status: &'static str,
    exit_code: i64,
    cycles: u64,
    tx_context_sha256: String,
    tx_size_bytes: u64,
    occupied_capacity_shannons: u64,
    fee_shannons: u64,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum DepositPhase1DaoTypeShape {
    Valid,
    Missing,
    Wrong,
}

impl DepositPhase1DaoTypeShape {
    fn failure_mode(self) -> Option<&'static str> {
        match self {
            DepositPhase1DaoTypeShape::Valid => None,
            DepositPhase1DaoTypeShape::Missing => Some("deposit_missing_dao_type"),
            DepositPhase1DaoTypeShape::Wrong => Some("deposit_wrong_dao_type"),
        }
    }

    fn fixture_type_label(self) -> Value {
        match self {
            DepositPhase1DaoTypeShape::Valid => json!("dao"),
            DepositPhase1DaoTypeShape::Missing => Value::Null,
            DepositPhase1DaoTypeShape::Wrong => json!("always_success_wrong_dao_type"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DepositPhase1LockShape {
    Valid,
    Wrong,
}

impl DepositPhase1LockShape {
    fn failure_mode(self) -> Option<&'static str> {
        match self {
            DepositPhase1LockShape::Valid => None,
            DepositPhase1LockShape::Wrong => Some("deposit_wrong_ickb_lock"),
        }
    }

    fn fixture_lock_label(self) -> &'static str {
        match self {
            DepositPhase1LockShape::Valid => "script_under_test",
            DepositPhase1LockShape::Wrong => "always_success_wrong_deposit_lock",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DepositPhase1DepositDataShape {
    Valid,
    Short,
    NonZero,
    LongTrailingZeros,
}

impl DepositPhase1DepositDataShape {
    fn failure_mode(self) -> Option<&'static str> {
        match self {
            DepositPhase1DepositDataShape::Valid => None,
            DepositPhase1DepositDataShape::Short => Some("deposit_short_dao_data"),
            DepositPhase1DepositDataShape::NonZero => Some("deposit_nonzero_dao_data"),
            DepositPhase1DepositDataShape::LongTrailingZeros => Some("deposit_long_dao_data"),
        }
    }

    fn expected_status(self) -> &'static str {
        match self {
            DepositPhase1DepositDataShape::Valid => "pass",
            DepositPhase1DepositDataShape::Short
            | DepositPhase1DepositDataShape::NonZero
            | DepositPhase1DepositDataShape::LongTrailingZeros => "fail",
        }
    }

    fn scenario(self) -> &'static str {
        match self {
            DepositPhase1DepositDataShape::Valid => "deposit_phase1",
            DepositPhase1DepositDataShape::Short => "deposit_short_dao_data",
            DepositPhase1DepositDataShape::NonZero => "deposit_nonzero_dao_data",
            DepositPhase1DepositDataShape::LongTrailingZeros => "deposit_long_dao_data",
        }
    }

    fn data(self) -> Bytes {
        match self {
            DepositPhase1DepositDataShape::Valid => Bytes::from(vec![0u8; 8]),
            DepositPhase1DepositDataShape::Short => Bytes::from(vec![0u8; 4]),
            DepositPhase1DepositDataShape::NonZero => Bytes::from(1u64.to_le_bytes().to_vec()),
            DepositPhase1DepositDataShape::LongTrailingZeros => Bytes::from(vec![0u8; 9]),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DepositPhase1Shapes {
    dao_type: DepositPhase1DaoTypeShape,
    lock: DepositPhase1LockShape,
    deposit_data: DepositPhase1DepositDataShape,
}

impl DepositPhase1Shapes {
    const VALID: Self = Self {
        dao_type: DepositPhase1DaoTypeShape::Valid,
        lock: DepositPhase1LockShape::Valid,
        deposit_data: DepositPhase1DepositDataShape::Valid,
    };

    fn new(dao_type: DepositPhase1DaoTypeShape, lock: DepositPhase1LockShape, deposit_data: DepositPhase1DepositDataShape) -> Self {
        Self { dao_type, lock, deposit_data }
    }
}

#[derive(Debug, Clone, Copy)]
enum MintXudtBinding {
    ScriptUnderTest,
    WrongOwnerHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MintReceiptDataMode {
    Valid,
    QuantityZero,
    QuantityTwo,
    ZeroFirstQuantity,
    MixedQuantities,
    LongTrailingData,
    MalformedFirstInput,
    MalformedSecondInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MintHeaderDepMode {
    Present,
    Omitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaoWithdrawalHeaderDepMode {
    Present,
    DepositDataInput,
    MalformedInputData,
    LongInputData,
    MissingWithdrawHeader,
    MissingDepositHeader,
    DepositHeaderIndexOutOfBounds,
    WrongDepositAccumulatedRate,
    WrongWithdrawAccumulatedRate,
    WrongDepositHeaderIndex,
    WrongWithdrawCommittedHeader,
    MissingWitnessInputType,
    EmptyWitnessInputType,
    ShortWitnessInputType,
    LongWitnessInputType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaoTwoInputWithdrawalMode {
    SameDeposit,
    MixedDeposit,
    MixedWithdraw,
    MixedBoth,
    SecondDepositDataInput,
    SecondMalformedInputData,
    SecondLongInputData,
    SecondWitnessMissing,
    SecondWitnessEmpty,
    SecondWitnessShort,
    SecondWitnessLong,
    SecondWitnessWithdrawHeaderIndex,
    SecondWitnessOutOfBoundsIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaoThreeInputWithdrawalMode {
    SameDeposit,
    MixedDepositSecond,
    MixedWithdrawSecond,
    MixedBothSecond,
    MixedDepositSecondWithdrawThird,
    MixedWithdrawSecondDepositThird,
    MixedDepositThird,
    MixedWithdrawThird,
    MixedBothThird,
    SecondWitnessMissing,
    SecondWitnessEmpty,
    SecondWitnessShort,
    SecondWitnessLong,
    SecondWitnessWithdrawHeaderIndex,
    SecondWitnessOutOfBoundsIndex,
    SecondDepositDataInput,
    SecondMalformedInputData,
    SecondLongInputData,
    ThirdWitnessMissing,
    ThirdWitnessEmpty,
    ThirdWitnessShort,
    ThirdWitnessLong,
    ThirdWitnessWithdrawHeaderIndex,
    ThirdWitnessOutOfBoundsIndex,
    ThirdDepositDataInput,
    ThirdMalformedInputData,
    ThirdLongInputData,
}

fn dao_deposit_cell_data() -> Bytes {
    Bytes::from(vec![0u8; 8])
}

fn dao_malformed_cell_data() -> Bytes {
    Bytes::from(vec![0x12, 0x06, 0x00, 0x00])
}

fn dao_withdrawal_request_cell_data() -> Bytes {
    Bytes::from(ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK.to_le_bytes().to_vec())
}

fn dao_long_withdrawal_request_cell_data() -> Bytes {
    let mut data = ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK.to_le_bytes().to_vec();
    data.push(0x99);
    Bytes::from(data)
}

fn dao_two_input_cell_data(mode: DaoTwoInputWithdrawalMode, index: usize) -> Bytes {
    if index == 1 {
        match mode {
            DaoTwoInputWithdrawalMode::SecondDepositDataInput => return dao_deposit_cell_data(),
            DaoTwoInputWithdrawalMode::SecondMalformedInputData => return dao_malformed_cell_data(),
            DaoTwoInputWithdrawalMode::SecondLongInputData => return dao_long_withdrawal_request_cell_data(),
            _ => {}
        }
    }
    dao_withdrawal_request_cell_data()
}

fn dao_three_input_cell_data(mode: DaoThreeInputWithdrawalMode, index: usize) -> Bytes {
    if index == 1 {
        match mode {
            DaoThreeInputWithdrawalMode::SecondDepositDataInput => return dao_deposit_cell_data(),
            DaoThreeInputWithdrawalMode::SecondMalformedInputData => return dao_malformed_cell_data(),
            DaoThreeInputWithdrawalMode::SecondLongInputData => return dao_long_withdrawal_request_cell_data(),
            _ => {}
        }
    }
    if index == 2 {
        match mode {
            DaoThreeInputWithdrawalMode::ThirdDepositDataInput => return dao_deposit_cell_data(),
            DaoThreeInputWithdrawalMode::ThirdMalformedInputData => return dao_malformed_cell_data(),
            DaoThreeInputWithdrawalMode::ThirdLongInputData => return dao_long_withdrawal_request_cell_data(),
            _ => {}
        }
    }
    dao_withdrawal_request_cell_data()
}

fn dao_three_input_witness_header_index(mode: DaoThreeInputWithdrawalMode, index: usize) -> u64 {
    if index == 1 {
        match mode {
            DaoThreeInputWithdrawalMode::MixedDepositSecond
            | DaoThreeInputWithdrawalMode::MixedBothSecond
            | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird => return 2,
            DaoThreeInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex => return 0,
            DaoThreeInputWithdrawalMode::SecondWitnessOutOfBoundsIndex => return 2,
            _ => {}
        }
    }
    if index == 2 {
        match mode {
            DaoThreeInputWithdrawalMode::MixedDepositThird
            | DaoThreeInputWithdrawalMode::MixedBothThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird => return 2,
            DaoThreeInputWithdrawalMode::ThirdWitnessWithdrawHeaderIndex => return 0,
            DaoThreeInputWithdrawalMode::ThirdWitnessOutOfBoundsIndex => return 2,
            _ => {}
        }
    }
    1
}

fn dao_three_input_witness(mode: DaoThreeInputWithdrawalMode, index: usize) -> packed::WitnessArgs {
    if index == 1 {
        match mode {
            DaoThreeInputWithdrawalMode::SecondWitnessMissing => return packed::WitnessArgs::new_builder().build(),
            DaoThreeInputWithdrawalMode::SecondWitnessEmpty => {
                return packed::WitnessArgs::new_builder().input_type(Some(Bytes::default()).pack()).build();
            }
            DaoThreeInputWithdrawalMode::SecondWitnessShort => {
                return packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(vec![1u8])).pack()).build();
            }
            DaoThreeInputWithdrawalMode::SecondWitnessLong => {
                return packed::WitnessArgs::new_builder()
                    .input_type(Some(Bytes::from(vec![1, 0, 0, 0, 0, 0, 0, 0, 0])).pack())
                    .build();
            }
            _ => {}
        }
    }
    if index == 2 {
        match mode {
            DaoThreeInputWithdrawalMode::ThirdWitnessMissing => return packed::WitnessArgs::new_builder().build(),
            DaoThreeInputWithdrawalMode::ThirdWitnessEmpty => {
                return packed::WitnessArgs::new_builder().input_type(Some(Bytes::default()).pack()).build();
            }
            DaoThreeInputWithdrawalMode::ThirdWitnessShort => {
                return packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(vec![1u8])).pack()).build();
            }
            DaoThreeInputWithdrawalMode::ThirdWitnessLong => {
                return packed::WitnessArgs::new_builder()
                    .input_type(Some(Bytes::from(vec![1, 0, 0, 0, 0, 0, 0, 0, 0])).pack())
                    .build();
            }
            _ => {}
        }
    }
    let witness_header_index = dao_three_input_witness_header_index(mode, index);
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(witness_header_index.to_le_bytes().to_vec())).pack()).build()
}

fn dao_three_input_witness_metadata(mode: DaoThreeInputWithdrawalMode, index: usize) -> Value {
    if index == 1 {
        match mode {
            DaoThreeInputWithdrawalMode::SecondWitnessMissing => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": null,
                    "witness_input_type_shape": "missing"
                });
            }
            DaoThreeInputWithdrawalMode::SecondWitnessEmpty => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": null,
                    "witness_input_type_shape": "empty"
                });
            }
            DaoThreeInputWithdrawalMode::SecondWitnessShort => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": null,
                    "witness_input_type_shape": "short_1_byte"
                });
            }
            DaoThreeInputWithdrawalMode::SecondWitnessLong => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": null,
                    "witness_input_type_shape": "long_9_bytes"
                });
            }
            DaoThreeInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": 0,
                    "expected_input_type_header_dep_index_le_u64": 1,
                    "witness_index_role": "withdraw_header_instead_of_deposit_header"
                });
            }
            DaoThreeInputWithdrawalMode::SecondWitnessOutOfBoundsIndex => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": 2,
                    "expected_input_type_header_dep_index_le_u64": 1,
                    "witness_index_role": "out_of_bounds_header_dep_index"
                });
            }
            _ => {}
        }
    }
    if index == 2 {
        match mode {
            DaoThreeInputWithdrawalMode::ThirdWitnessMissing => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": null,
                    "witness_input_type_shape": "missing"
                });
            }
            DaoThreeInputWithdrawalMode::ThirdWitnessEmpty => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": null,
                    "witness_input_type_shape": "empty"
                });
            }
            DaoThreeInputWithdrawalMode::ThirdWitnessShort => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": null,
                    "witness_input_type_shape": "short_1_byte"
                });
            }
            DaoThreeInputWithdrawalMode::ThirdWitnessLong => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": null,
                    "witness_input_type_shape": "long_9_bytes"
                });
            }
            DaoThreeInputWithdrawalMode::ThirdWitnessWithdrawHeaderIndex => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": 0,
                    "expected_input_type_header_dep_index_le_u64": 1,
                    "witness_index_role": "withdraw_header_instead_of_deposit_header"
                });
            }
            DaoThreeInputWithdrawalMode::ThirdWitnessOutOfBoundsIndex => {
                return json!({
                    "index": index,
                    "input_type_header_dep_index_le_u64": 2,
                    "expected_input_type_header_dep_index_le_u64": 1,
                    "witness_index_role": "out_of_bounds_header_dep_index"
                });
            }
            _ => {}
        }
    }
    json!({
        "index": index,
        "input_type_header_dep_index_le_u64": dao_three_input_witness_header_index(mode, index)
    })
}

fn dao_two_input_second_witness_header_index(mode: DaoTwoInputWithdrawalMode) -> u64 {
    match mode {
        DaoTwoInputWithdrawalMode::MixedDeposit | DaoTwoInputWithdrawalMode::MixedBoth => 2,
        DaoTwoInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex => 0,
        DaoTwoInputWithdrawalMode::SecondWitnessOutOfBoundsIndex => 2,
        _ => 1,
    }
}

fn dao_two_input_second_witness(mode: DaoTwoInputWithdrawalMode) -> packed::WitnessArgs {
    match mode {
        DaoTwoInputWithdrawalMode::SecondWitnessMissing => packed::WitnessArgs::new_builder().build(),
        DaoTwoInputWithdrawalMode::SecondWitnessEmpty => {
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::default()).pack()).build()
        }
        DaoTwoInputWithdrawalMode::SecondWitnessShort => {
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(vec![1u8])).pack()).build()
        }
        DaoTwoInputWithdrawalMode::SecondWitnessLong => {
            let mut input_type = 1u64.to_le_bytes().to_vec();
            input_type.push(0x99);
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(input_type)).pack()).build()
        }
        _ => packed::WitnessArgs::new_builder()
            .input_type(Some(Bytes::from(dao_two_input_second_witness_header_index(mode).to_le_bytes().to_vec())).pack())
            .build(),
    }
}

fn dao_two_input_second_witness_metadata(mode: DaoTwoInputWithdrawalMode) -> Value {
    match mode {
        DaoTwoInputWithdrawalMode::SecondWitnessMissing => json!({
            "index": 1,
            "input_type_present": false
        }),
        DaoTwoInputWithdrawalMode::SecondWitnessEmpty => json!({
            "index": 1,
            "input_type_present": true,
            "input_type_bytes": "0x",
            "input_type_length_bytes": 0
        }),
        DaoTwoInputWithdrawalMode::SecondWitnessShort => json!({
            "index": 1,
            "input_type_present": true,
            "input_type_bytes": "0x01",
            "input_type_length_bytes": 1,
            "expected_input_type_length_bytes": 8
        }),
        DaoTwoInputWithdrawalMode::SecondWitnessLong => json!({
            "index": 1,
            "input_type_present": true,
            "input_type_bytes": "0x010000000000000099",
            "input_type_length_bytes": 9,
            "expected_input_type_length_bytes": 8
        }),
        DaoTwoInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex => json!({
            "index": 1,
            "input_type_header_dep_index_le_u64": 0,
            "expected_input_type_header_dep_index_le_u64": 1,
            "witness_index_role": "withdraw_header_instead_of_deposit_header"
        }),
        DaoTwoInputWithdrawalMode::SecondWitnessOutOfBoundsIndex => json!({
            "index": 1,
            "input_type_header_dep_index_le_u64": 2,
            "expected_input_type_header_dep_index_le_u64": 1,
            "witness_index_role": "out_of_bounds_header_dep_index"
        }),
        _ => json!({
            "index": 1,
            "input_type_header_dep_index_le_u64": dao_two_input_second_witness_header_index(mode)
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitOrderAssetBinding {
    SameAuxiliaryType,
    DifferentAuxiliaryType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitOrderMasterBinding {
    Matching,
    WrongTxHash,
    WrongIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitOrderOutputDataMode {
    Match,
    MintAction,
    InvalidAction,
    ShortAction,
    ShortMasterOutPoint,
    LongTrailingData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitOrderInputDataMode {
    Mint,
    MatchAbsolute,
    MatchWrongTxHash,
    MatchWrongIndex,
    InvalidAction,
    ShortAction,
    ShortMasterOutPoint,
    LongTrailingData,
}

impl LimitOrderOutputDataMode {
    fn order_action(self) -> &'static str {
        match self {
            Self::Match | Self::ShortMasterOutPoint | Self::LongTrailingData => "Match",
            Self::MintAction => "Mint",
            Self::InvalidAction => "Invalid",
            Self::ShortAction => "Short",
        }
    }
}

impl LimitOrderInputDataMode {
    fn order_action(self) -> &'static str {
        match self {
            Self::Mint | Self::ShortAction => "Mint",
            Self::MatchAbsolute
            | Self::MatchWrongTxHash
            | Self::MatchWrongIndex
            | Self::ShortMasterOutPoint
            | Self::LongTrailingData => "Match",
            Self::InvalidAction => "Invalid",
        }
    }
}

impl LimitOrderMasterBinding {
    fn failure_mode(self, udt_to_ckb: bool) -> Option<&'static str> {
        match (self, udt_to_ckb) {
            (Self::Matching, _) => None,
            (Self::WrongTxHash, false) => Some("wrong_master_tx_hash"),
            (Self::WrongIndex, false) => Some("wrong_master_index"),
            (Self::WrongTxHash, true) => Some("limit_order_udt_to_ckb_wrong_master_tx_hash"),
            (Self::WrongIndex, true) => Some("limit_order_udt_to_ckb_wrong_master_index"),
        }
    }

    fn master_tx_hash(self) -> &'static [u8; 32] {
        match self {
            Self::Matching | Self::WrongIndex => &LIMIT_ORDER_MASTER_TX_HASH,
            Self::WrongTxHash => &LIMIT_ORDER_WRONG_MASTER_TX_HASH,
        }
    }

    fn master_index(self) -> u32 {
        match self {
            Self::Matching | Self::WrongTxHash => 0,
            Self::WrongIndex => 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LimitOrderBuildParams {
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    master_binding: LimitOrderMasterBinding,
    input_data_mode: LimitOrderInputDataMode,
    output_data_mode: LimitOrderOutputDataMode,
}

#[derive(Debug, Clone, Copy)]
struct LimitOrderScenarioOptions {
    failure_mode: Option<&'static str>,
    asset_binding: LimitOrderAssetBinding,
    pass_scenario: Option<&'static str>,
    master_binding: LimitOrderMasterBinding,
    input_data_mode: LimitOrderInputDataMode,
    output_data_mode: LimitOrderOutputDataMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitOrderCellShape {
    MissingMatchingOutput,
    DuplicateMatchingOutputs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LimitOrderTypeShape {
    MissingInputAuxiliaryType,
    MissingOutputAuxiliaryType,
}

impl LimitOrderCellShape {
    fn scenario(self, udt_to_ckb: bool) -> &'static str {
        match (self, udt_to_ckb) {
            (Self::MissingMatchingOutput, false) => "limit_order_missing_matching_output",
            (Self::DuplicateMatchingOutputs, false) => "limit_order_duplicate_matching_output",
            (Self::MissingMatchingOutput, true) => "limit_order_udt_to_ckb_missing_matching_output",
            (Self::DuplicateMatchingOutputs, true) => "limit_order_udt_to_ckb_duplicate_matching_output",
        }
    }

    fn failure_mode(self, udt_to_ckb: bool) -> &'static str {
        self.scenario(udt_to_ckb)
    }
}

impl LimitOrderTypeShape {
    fn scenario(self, udt_to_ckb: bool) -> &'static str {
        match (self, udt_to_ckb) {
            (Self::MissingInputAuxiliaryType, false) => "limit_order_missing_input_type",
            (Self::MissingOutputAuxiliaryType, false) => "limit_order_missing_output_type",
            (Self::MissingInputAuxiliaryType, true) => "limit_order_udt_to_ckb_missing_input_type",
            (Self::MissingOutputAuxiliaryType, true) => "limit_order_udt_to_ckb_missing_output_type",
        }
    }

    fn failure_mode(self, udt_to_ckb: bool) -> &'static str {
        self.scenario(udt_to_ckb)
    }

    fn input_type_present(self) -> bool {
        !matches!(self, Self::MissingInputAuxiliaryType)
    }

    fn output_type_present(self) -> bool {
        !matches!(self, Self::MissingOutputAuxiliaryType)
    }
}

fn limit_order_options(
    failure_mode: Option<&'static str>,
    asset_binding: LimitOrderAssetBinding,
    pass_scenario: Option<&'static str>,
    master_binding: LimitOrderMasterBinding,
    input_data_mode: LimitOrderInputDataMode,
    output_data_mode: LimitOrderOutputDataMode,
) -> LimitOrderScenarioOptions {
    LimitOrderScenarioOptions { failure_mode, asset_binding, pass_scenario, master_binding, input_data_mode, output_data_mode }
}
