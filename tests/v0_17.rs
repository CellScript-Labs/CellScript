use camino::Utf8PathBuf;
use cellscript::runtime_errors::CellScriptRuntimeError;
use cellscript::{compile, compile_file, CompileOptions};

const CKB_SOURCE_PROGRAM: &str = r#"
module v017::ckb_source

resource Token has store, create, consume {
    amount: u64
}

action inspect(
    expected_lock_hash: Hash,
    expected_type_hash: Hash,
    relative_distance: i32
) -> u64 {
    verification
        let input = source::group_input(0)
        let output = source::output(0)
        let owner_output = source::output(1)
        let header = source::header_dep(0)
        let capacity = ckb::cell_capacity(input)
        let occupied = ckb::cell_occupied_capacity(input)
        let unoccupied = ckb::cell_unoccupied_capacity(input)
        let data_size = ckb::cell_data_size(input)
        let output_index = ckb::cell_output_index(source::group_output(0))
        let out_point_index = ckb::input_out_point_index(input)
        let out_point_hash_low = ckb::input_out_point_tx_hash_low(input)
        let lock_hash_low = ckb::cell_lock_hash_low(input)
        let type_hash_low = ckb::cell_type_hash_low(input)
        ckb::require_cell_lock_args_empty(input)
        ckb::require_cell_type_args_empty(input)
        ckb::require_current_script_args_empty()
        ckb::require_cell_lock_args_hash(output, expected_lock_hash)
        ckb::require_cell_type_args_hash(output, expected_type_hash)
        ckb::require_cell_lock_script_hash_type(output, expected_lock_hash, 1)
        ckb::require_cell_type_script_hash_type(output, expected_type_hash, 1)
        dao::require_header_dep_for_input(input, header)
        let absolute_since = ckb::since_epoch_absolute(42, 0, 1)
        let relative_since = ckb::since_epoch_relative(42, 0, 1)
        dao::require_input_since_at_least(input, relative_since)
        dao::require_input_relative_epoch_since_at_least(input, 42, 0, 1)
        let rate = dao::accumulated_rate(header)
        let input_rate = dao::input_accumulated_rate(input)
        let has_dao_type = dao::has_dao_type(input)
        let is_deposit_data = dao::is_deposit_data(input)
        let is_withdrawal_request_data = dao::is_withdrawal_request_data(input)
        let low = xudt::amount_low(input)
        let high = xudt::amount_high(input)
        ckb::require_cell_lock_hash(input, expected_lock_hash)
        ckb::require_cell_type_hash(input, expected_type_hash)
        ckb::require_input_out_point_tx_hash(input, expected_lock_hash)
        ckb::require_input_out_point(input, expected_lock_hash, out_point_index)
        ckb::require_metapoint_relative(owner_output, output, relative_distance)
        ckb::require_lock_type_metapoint_pairs(source::output(0), relative_distance)
        ckb::require_type_lock_metapoint_pairs(source::input(0), relative_distance)
        ckb::require_type_lock_metapoint_pairs_from_i32_data(source::input(0), 0)
        ckb::require_lock_type_metapoint_pairs_from_i32_data(source::output(0), 52)
        ckb::require_type_lock_metapoint_pairs_from_i32_data_filtered(source::input(0), 0, expected_type_hash, 2)
        ckb::require_lock_type_metapoint_pairs_from_i32_data_filtered(source::output(0), 52, expected_type_hash, 1)
        ckb::require_lock_match_master_out_point_pairs_from_data(source::input(0), source::output(0), 16, 20, 52)
        let current_script_hash: Hash = ckb::current_script_hash()
        ckb::require_cell_type_hash(input, current_script_hash)
        xudt::require_owner_mode_input_type(input, expected_type_hash)
        xudt::require_owner_mode_type_args(input, expected_lock_hash, 2147483648)
        xudt::require_owner_mode_type_args(input, current_script_hash, 2147483648)
        xudt::require_owner_mode_type_args_current_script(input, 2147483648)
        xudt::require_group_amount_conserved()
        require ckb::current_role() == 2, "action role must be type"
        let deposit_flag = if is_deposit_data { 1 } else { 0 }
        let withdrawal_flag = if is_withdrawal_request_data { 1 } else { 0 }
        let dao_type_flag = if has_dao_type { 1 } else { 0 }
        return capacity + occupied + unoccupied + data_size + output_index + out_point_index + out_point_hash_low + lock_hash_low + type_hash_low + rate + input_rate + dao_type_flag + deposit_flag + withdrawal_flag + low + high + absolute_since + relative_since

}
action verify_xudt_mint_delta(delta: u128) {
    verification
        xudt::require_group_amount_minted(delta)

}
action verify_xudt_burn_delta(delta: u128) {
    verification
        xudt::require_group_amount_burned(delta)

}
lock guard(token: protected Token) -> bool {
    verification
        require ckb::current_role() == 1
        require token.amount > 0
}
"#;

const METADATA_ONLY_INVARIANT: &str = r#"
module v017::gap

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
}

resource Token has store, create, consume {
    amount: u64
}

action noop() -> u64 {
    verification
        return 0
}
"#;

const XUDT_GROUP_AMOUNT_BRIDGE: &str = r#"
module v017::xudt_aggregate_bridge

invariant xudt_transfer_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<IckbToken>.amount, group_outputs<IckbToken>.amount
    assert_sum(group_outputs<IckbToken>.amount) == assert_sum(group_inputs<IckbToken>.amount)
}

resource IckbToken has store, create, consume {
    amount: u128
}

action verify() {
    verification
        xudt::require_group_amount_conserved()
}
"#;

const XUDT_GROUP_AMOUNT_BRIDGE_MISSING_HELPER: &str = r#"
module v017::xudt_aggregate_bridge_gap

invariant xudt_transfer_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<IckbToken>.amount, group_outputs<IckbToken>.amount
    assert_sum(group_outputs<IckbToken>.amount) == assert_sum(group_inputs<IckbToken>.amount)
}

resource IckbToken has store, create, consume {
    amount: u128
}

action verify() {
    verification
        require true
}
"#;

const XUDT_GROUP_AMOUNT_DELTA_BRIDGE: &str = r#"
module v017::xudt_delta_bridge

invariant xudt_mint_delta {
    trigger: type_group
    scope: group
    reads: group_inputs<IckbToken>.amount, group_outputs<IckbToken>.amount
    assert_delta(group_outputs<IckbToken>.amount, minted, scope = group)
}

invariant xudt_burn_delta {
    trigger: type_group
    scope: group
    reads: group_inputs<IckbToken>.amount, group_outputs<IckbToken>.amount
    assert_delta(group_inputs<IckbToken>.amount, burned, scope = group)
}

resource IckbToken has store, create, consume {
    amount: u128
}

action verify_mint(minted: u128) {
    verification
        xudt::require_group_amount_minted(minted)

}
action verify_burn(burned: u128) {
    verification
        xudt::require_group_amount_burned(burned)
}
"#;

const XUDT_GROUP_AMOUNT_DELTA_MISSING_HELPER: &str = r#"
module v017::xudt_delta_bridge_gap

invariant xudt_mint_delta {
    trigger: type_group
    scope: group
    reads: group_inputs<IckbToken>.amount, group_outputs<IckbToken>.amount
    assert_delta(group_outputs<IckbToken>.amount, minted, scope = group)
}

resource IckbToken has store, create, consume {
    amount: u128
}

action verify() {
    verification
        require true
}
"#;

const C256_PROGRAM: &str = r#"
module v017::c256_math

action check_limit_order_product(
    left_amount: u128,
    left_multiplier: u128,
    right_amount: u128,
    right_multiplier: u128,
    fee_amount: u128,
    fee_multiplier: u128
) {
    verification
        c256::require_product_lte(left_amount, left_multiplier, right_amount, right_multiplier)
        c256::require_product_eq(left_amount, left_multiplier, left_amount, left_multiplier)
        c256::require_sum2_products_lte(
            left_amount, left_multiplier,
            fee_amount, fee_multiplier,
            right_amount, right_multiplier,
            fee_amount, fee_multiplier
        )
        c256::require_sum2_products_eq(
            left_amount, left_multiplier,
            fee_amount, fee_multiplier,
            left_amount, left_multiplier,
            fee_amount, fee_multiplier
        )
}
"#;

const U128_LOCAL_PROGRAM: &str = r#"
module v017::u128_local_values

fn add_u128(left: u128, right: u128) -> u128 {
    return left + right
}

action verify_computed_mint_delta(left: u128, right: u128) {
    verification
        let base: u128 = add_u128(left, right)
        let reduced: u128 = base - 1
        let product: u128 = reduced * 2
        let quotient: u128 = product / 2
        require quotient >= left, "u128 comparison must use high limb"
        xudt::require_group_amount_minted(quotient)
}
"#;

const SIGNED_I32_PROGRAM: &str = r#"
module v017::signed_index

struct MetaPoint {
    anchor: u64,
    relative_index: i32
}

action signed_relative_order(left: i32, right: i32) -> bool {
    verification
        return left < right

}
action signed_field_order(point: MetaPoint, right: i32) -> bool {
    verification
        return point.relative_index < right
}
"#;

#[test]
fn ckb_source_primitives_lower_to_runtime_helpers() {
    let result = compile(
        CKB_SOURCE_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("0.17 CKB source program should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    for helper in [
        "__ckb_source_group_input",
        "__ckb_source_output",
        "__ckb_source_header_dep",
        "__ckb_source_group_output",
        "__ckb_current_script_hash",
        "__ckb_cell_capacity",
        "__ckb_cell_occupied_capacity",
        "__ckb_cell_unoccupied_capacity",
        "__ckb_cell_output_index",
        "__ckb_input_out_point_index",
        "__ckb_input_out_point_tx_hash_low",
        "__ckb_require_input_out_point_tx_hash",
        "__ckb_require_input_out_point",
        "__ckb_require_metapoint_relative",
        "__ckb_require_lock_type_metapoint_pairs",
        "__ckb_require_type_lock_metapoint_pairs",
        "__ckb_require_lock_type_metapoint_pairs_from_i32_data",
        "__ckb_require_type_lock_metapoint_pairs_from_i32_data",
        "__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered",
        "__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered",
        "__ckb_require_lock_match_master_out_point_pairs_from_data",
        "__ckb_cell_lock_hash_low",
        "__ckb_cell_type_hash_low",
        "__ckb_require_cell_lock_hash",
        "__ckb_require_cell_type_hash",
        "__ckb_require_current_script_args_empty",
        "__ckb_require_cell_lock_args_empty",
        "__ckb_require_cell_type_args_empty",
        "__ckb_require_cell_lock_args_hash",
        "__ckb_require_cell_type_args_hash",
        "__ckb_require_cell_lock_script_hash_type",
        "__ckb_require_cell_type_script_hash_type",
        "__ckb_cell_data_size",
        "__ckb_since_epoch_absolute",
        "__ckb_since_epoch_relative",
        "__dao_accumulated_rate",
        "__dao_input_accumulated_rate",
        "__dao_has_dao_type",
        "__dao_is_deposit_data",
        "__dao_is_withdrawal_request_data",
        "__dao_require_header_dep_for_input",
        "__dao_require_input_since_at_least",
        "__dao_require_input_relative_epoch_since_at_least",
        "__xudt_amount_low",
        "__xudt_amount_high",
        "__xudt_require_owner_mode_input_type",
        "__xudt_require_owner_mode_type_args",
        "__xudt_require_owner_mode_type_args_current_script",
        "__xudt_require_group_amount_conserved",
        "__xudt_require_group_amount_minted",
        "__xudt_require_group_amount_burned",
    ] {
        assert!(assembly.contains(&format!(".global {helper}")), "missing helper {helper}:\n{assembly}");
    }
    assert!(assembly.contains("CKB SourceView helper"), "{assembly}");
    assert!(
        assembly.contains("current script Hash via LOAD_SCRIPT_HASH")
            && assembly.contains("load current script hash into addressable Hash"),
        "current script hash must be loaded as an addressable 32-byte Hash:\n{assembly}"
    );
    assert!(assembly.contains("DAO accumulated-rate HeaderDep SourceView helper"), "{assembly}");
    assert!(assembly.contains("DAO accumulated-rate from Input/GroupInput committed header via LOAD_HEADER"), "{assembly}");
    assert!(
        assembly.contains("call __dao_input_accumulated_rate\n    # cellscript abi: scalar runtime helper status check (a1 == 0)"),
        "dao::input_accumulated_rate must fail closed on helper status a1:\n{assembly}"
    );
    assert!(assembly.contains("NervosDAO type-hash classifier"), "{assembly}");
    assert!(assembly.contains("DAO deposit data classifier via LOAD_CELL_DATA exact 8-byte DAO data"), "{assembly}");
    assert!(assembly.contains("DAO withdrawal request data classifier via LOAD_CELL_DATA exact 8-byte DAO data"), "{assembly}");
    assert!(assembly.contains("DAO input since lower-bound requirement"), "{assembly}");
    assert!(assembly.contains("CKB RFC0017 relative epoch since encoder"), "{assembly}");
    assert!(assembly.contains("DAO relative epoch since maturity requirement"), "{assembly}");
    assert!(
        assembly.contains("absolute header offset 160+8"),
        "DAO accumulated-rate helper must read the accumulated-rate bytes:\n{assembly}"
    );
    assert!(assembly.contains("DAO input header to HeaderDep lineage requirement"), "{assembly}");
    assert!(
        assembly.contains("# cellscript abi: scalar runtime helper status check (a1 == 0)"),
        "0.17 scalar runtime helpers must be fail-closed at call sites:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code())),
        "DAO helper must return a stable malformed-field status on syscall/field failure:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::DaoHeaderLineageMismatch.code())),
        "DAO header lineage helper must return a stable mismatch status:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::DaoMaturityViolation.code())),
        "DAO since helper must return a stable maturity violation status:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::CkbSinceMalformed.code()))
            || assembly.contains(&format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code())),
        "epoch since helpers must return a stable malformed-since status:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code())),
        "xUDT helper must return a stable mismatch status on malformed token data:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code())),
        "OutPoint tx-hash requirement must return a stable mismatch status:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code())),
        "xUDT type-args helper must return a stable malformed-script status:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code())),
        "script args helpers must return a stable args mismatch status:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::ScriptIdentityMismatch.code())),
        "script identity helpers must return a stable code_hash/hash_type mismatch status:\n{assembly}"
    );
    assert!(
        assembly.contains("full-hash requirement"),
        "lock/type/xUDT hash requirements must compare full 32-byte hashes:\n{assembly}"
    );
    assert!(assembly.contains("Script empty-args requirement"), "{assembly}");
    assert!(assembly.contains("Script code_hash/hash_type requirement"), "{assembly}");
    assert!(
        assembly.contains("current-script empty-args requirement via LOAD_SCRIPT plus output lock scan")
            && assembly.contains("require matching output lock scripts to keep empty args"),
        "current-script args helper must also scan same-code/hash-type output locks:\n{assembly}"
    );
    assert!(assembly.contains("Script 32-byte args requirement"), "{assembly}");
    assert!(assembly.contains("sum(outputs.amount) == sum(inputs.amount) + delta"), "{assembly}");
    assert!(assembly.contains("sum(inputs.amount) == sum(outputs.amount) + delta"), "{assembly}");
    assert!(
        assembly.contains("args a0=SourceView, a1=expected_hash_ptr, a2=expected_hash_len"),
        "full-hash runtime helpers must use pointer+length expected hash ABI:\n{assembly}"
    );
    assert!(
        assembly.contains("OutPoint full tx-hash + index requirement")
            && assembly.contains("a3=expected_index")
            && assembly.contains("call __ckb_require_input_out_point"),
        "OutPoint helper must bind tx-hash and index through a single fail-closed runtime call:\n{assembly}"
    );
    assert!(
        assembly.contains("MetaPoint relative-distance requirement")
            && assembly.contains("output MetaPoint compare base_output_index + distance == related_output_index")
            && assembly.contains("input MetaPoint compare OutPoint tx_hash and base_out_index + distance"),
        "MetaPoint helper must cover both output-index and input-OutPoint semantics:\n{assembly}"
    );
    assert!(
        assembly.contains("scans current-script lock-only/type-only cells and requires one-to-one MetaPoint pairing")
            && assembly.contains("current-script lock-only to type-only MetaPoint pair cardinality check")
            && assembly.contains("current-script type-only to lock-only MetaPoint pair cardinality check"),
        "MetaPoint pair cardinality helpers must lower current-script group scans:\n{assembly}"
    );
    assert!(
        assembly.contains("using signed i32 distance loaded from base cell data")
            && assembly.contains("load signed i32 MetaPoint distance from the base cell data"),
        "MetaPoint data-driven pair cardinality helpers must load signed i32 distances from cell data:\n{assembly}"
    );
    assert!(
        assembly.contains("filtered MetaPoint related cell type hash and data-rule check")
            && assembly.contains("filtered data rules: 0=no data check, 1=exact 8-byte zero u64, 2=exact 8-byte nonzero u64"),
        "filtered MetaPoint aggregates must lower generic related-cell type/data filters:\n{assembly}"
    );
    assert!(
        assembly.contains("Limit-Order-style lock-only match order master OutPoint pairing")
            && assembly.contains("input orders may encode master as Mint(relative i32) or Match(absolute OutPoint)")
            && assembly.contains("output orders must encode master as Match(absolute OutPoint)"),
        "Limit Order Match helper must encode absolute-master OutPoint pairing semantics:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::MetaPointMismatch.code())),
        "MetaPoint helper must return a stable mismatch status:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::MetaPointCardinalityMismatch.code())),
        "MetaPoint pair scan must return a stable cardinality mismatch status:\n{assembly}"
    );
    assert!(
        assembly.contains("owner hash is LOAD_SCRIPT_HASH(current script)")
            && assembly.contains("call __xudt_require_owner_mode_type_args"),
        "xUDT current-script helper must bind type args to the current script hash:\n{assembly}"
    );
    assert!(
        assembly.contains("CKB occupied capacity via LOAD_CELL_BY_FIELD CellField::OccupiedCapacity")
            && assembly.contains("SourceView unoccupied capacity = capacity - occupied_capacity"),
        "occupied/unoccupied capacity helpers must use the canonical CKB occupied-capacity field:\n{assembly}"
    );

    let features = &result.metadata.runtime.ckb_runtime_features;
    assert!(features.contains(&"ckb-source-view".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-source-cell-fields".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-source-input-out-point".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-metapoint-relative".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-metapoint-pair-cardinality".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-metapoint-data-driven-cardinality".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-metapoint-filtered-cardinality".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-metapoint-master-outpoint-data-cardinality".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-script-args-requirements".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-script-identity-requirements".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-dao-header-accumulated-rate".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-dao-input-header-accumulated-rate".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-dao-cell-classification".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-dao-header-lineage".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-dao-input-since-maturity".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-dao-relative-epoch-maturity".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-input-since".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-since-epoch-encoding".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-xudt-layout-binding".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-xudt-group-amount-conservation".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-xudt-group-amount-delta".to_string()), "{features:?}");

    let accesses = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .map(|access| (access.operation.as_str(), access.syscall.as_str(), access.source.as_str()))
        .collect::<Vec<_>>();
    assert!(accesses.contains(&("dao-accumulated-rate", "LOAD_HEADER", "HeaderDep")), "{accesses:?}");
    assert!(accesses.contains(&("dao-input-accumulated-rate", "LOAD_HEADER", "Input/GroupInput")), "{accesses:?}");
    assert!(accesses.contains(&("dao-type-hash-classifier", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("dao-deposit-data-classifier", "LOAD_CELL_DATA", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("dao-withdrawal-request-data-classifier", "LOAD_CELL_DATA", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("dao-header-dep-input-lineage", "LOAD_HEADER", "Input/HeaderDep")), "{accesses:?}");
    assert!(accesses.contains(&("dao-input-since-maturity", "LOAD_INPUT_BY_FIELD", "Input/GroupInput")), "{accesses:?}");
    assert!(
        accesses.contains(&("dao-input-relative-epoch-since-maturity", "LOAD_INPUT_BY_FIELD", "Input/GroupInput")),
        "{accesses:?}"
    );
    assert!(accesses.contains(&("since-epoch-absolute", "CKB_SINCE_ENCODING", "Expression")), "{accesses:?}");
    assert!(accesses.contains(&("since-epoch-relative", "CKB_SINCE_ENCODING", "Expression")), "{accesses:?}");
    assert!(accesses.contains(&("cell-capacity", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-occupied-capacity", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-unoccupied-capacity", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-lock-hash-low", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-type-hash-low", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-lock-hash-require", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-type-hash-require", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-lock-script-empty-args-require", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-type-script-empty-args-require", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(
        accesses.contains(&("current-script-empty-args-require", "LOAD_SCRIPT+LOAD_CELL_BY_FIELD", "CurrentScript/Output")),
        "{accesses:?}"
    );
    assert!(accesses.contains(&("cell-lock-script-hash-args-require", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-type-script-hash-args-require", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-lock-script-identity-require", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-type-script-identity-require", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("current-script-hash", "LOAD_SCRIPT_HASH", "CurrentScript")), "{accesses:?}");
    assert!(accesses.contains(&("input-out-point-index", "LOAD_INPUT_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("input-out-point-tx-hash-require", "LOAD_INPUT_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("input-out-point-require", "LOAD_INPUT_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("metapoint-relative-require", "LOAD_INPUT_BY_FIELD/SOURCE_VIEW", "Input/Output")), "{accesses:?}");
    assert!(
        accesses.contains(&(
            "metapoint-lock-type-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output"
        )),
        "{accesses:?}"
    );
    assert!(
        accesses.contains(&(
            "metapoint-type-lock-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output"
        )),
        "{accesses:?}"
    );
    assert!(
        accesses.contains(&(
            "metapoint-lock-type-data-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output"
        )),
        "{accesses:?}"
    );
    assert!(
        accesses.contains(&(
            "metapoint-type-lock-data-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output"
        )),
        "{accesses:?}"
    );
    assert!(
        accesses.contains(&(
            "metapoint-lock-type-filtered-data-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output"
        )),
        "{accesses:?}"
    );
    assert!(
        accesses.contains(&(
            "metapoint-type-lock-filtered-data-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output"
        )),
        "{accesses:?}"
    );
    assert!(
        accesses.contains(&(
            "metapoint-lock-match-master-out-point-data-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD",
            "Input/Output"
        )),
        "{accesses:?}"
    );
    assert!(accesses.contains(&("xudt-require-owner-mode-type-args", "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(
        accesses.contains(&(
            "xudt-require-owner-mode-type-args-current-script",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD",
            "CurrentScript/SourceView"
        )),
        "{accesses:?}"
    );
    assert!(accesses.contains(&("xudt-group-amount-conservation", "LOAD_CELL_DATA", "GroupInput/GroupOutput")), "{accesses:?}");
    assert!(accesses.contains(&("xudt-group-amount-minted-delta", "LOAD_CELL_DATA", "GroupInput/GroupOutput")), "{accesses:?}");
    assert!(accesses.contains(&("xudt-group-amount-burned-delta", "LOAD_CELL_DATA", "GroupInput/GroupOutput")), "{accesses:?}");

    let source_view_cell_access = result
        .metadata
        .runtime
        .proof_plan
        .iter()
        .filter(|plan| plan.category == "cell-access" && plan.feature.contains(":SourceView#"))
        .collect::<Vec<_>>();
    assert!(!source_view_cell_access.is_empty(), "expected SourceView cell-access proof records");
    assert!(
        source_view_cell_access.iter().all(|plan| plan.reads.iter().any(|read| read == "source_view")),
        "SourceView cell-access proof records must declare source_view reads: {source_view_cell_access:#?}"
    );
    let dao_lineage_plan = result
        .metadata
        .runtime
        .proof_plan
        .iter()
        .find(|plan| plan.category == "cell-access" && plan.feature.contains("dao-header-dep-input-lineage:Input/HeaderDep#"))
        .expect("DAO header lineage ProofPlan record");
    assert!(dao_lineage_plan.reads.iter().any(|read| read == "input"), "{dao_lineage_plan:#?}");
    assert!(dao_lineage_plan.reads.iter().any(|read| read == "header_dep"), "{dao_lineage_plan:#?}");
    let dao_since_plan = result
        .metadata
        .runtime
        .proof_plan
        .iter()
        .find(|plan| plan.category == "cell-access" && plan.feature.contains("dao-input-since-maturity:Input/GroupInput#"))
        .expect("DAO input since ProofPlan record");
    assert!(dao_since_plan.reads.iter().any(|read| read == "input"), "{dao_since_plan:#?}");
    let dao_epoch_since_plan = result
        .metadata
        .runtime
        .proof_plan
        .iter()
        .find(|plan| {
            plan.category == "cell-access" && plan.feature.contains("dao-input-relative-epoch-since-maturity:Input/GroupInput#")
        })
        .expect("DAO relative epoch since ProofPlan record");
    assert!(dao_epoch_since_plan.reads.iter().any(|read| read == "input"), "{dao_epoch_since_plan:#?}");
    assert!(dao_epoch_since_plan.reads.iter().any(|read| read == "group_input"), "{dao_epoch_since_plan:#?}");

    let elf = compile(
        CKB_SOURCE_PROGRAM,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("0.17 CKB source runtime helpers should assemble to ELF");
    assert!(elf.artifact_bytes.starts_with(b"\x7fELF"));
}

#[test]
fn strict_0_17_rejects_metadata_only_aggregate_invariants() {
    let err = compile(
        METADATA_ONLY_INVARIANT,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("v0.17 strict mode must reject metadata-only aggregate invariants");

    assert!(err.message.contains("0.17 CKB source strict check failed"), "unexpected error: {}", err.message);
    assert!(err.message.contains("PP0170"), "unexpected error: {}", err.message);
}

#[test]
fn strict_0_17_accepts_xudt_group_amount_aggregate_when_runtime_helper_is_called() {
    let result = compile(
        XUDT_GROUP_AMOUNT_BRIDGE,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("xUDT group amount aggregate should be discharged by explicit runtime helper");

    let aggregate_plan = result
        .metadata
        .runtime
        .proof_plan
        .iter()
        .find(|plan| plan.category == "aggregate-invariant")
        .expect("aggregate invariant ProofPlan record");
    assert_eq!(aggregate_plan.codegen_coverage_status, "gap:runtime-helper-required");
    assert!(
        aggregate_plan.coverage.iter().any(|coverage| coverage == "runtime_helper:xudt::require_group_amount_conserved"),
        "{aggregate_plan:#?}"
    );
    assert!(
        aggregate_plan
            .input_output_relation_checks
            .iter()
            .any(|check| check.contains("runtime-helper-required:xudt::require_group_amount_conserved")),
        "{aggregate_plan:#?}"
    );
    assert!(
        result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
            access.operation == "xudt-group-amount-conservation"
                && access.syscall == "LOAD_CELL_DATA"
                && access.source == "GroupInput/GroupOutput"
        }),
        "{:#?}",
        result.metadata.runtime.ckb_runtime_accesses
    );

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    assert!(assembly.contains(".global __xudt_require_group_amount_conserved"), "missing xUDT conservation helper:\n{assembly}");
}

#[test]
fn strict_0_17_rejects_xudt_group_amount_aggregate_without_runtime_helper_call() {
    let err = compile(
        XUDT_GROUP_AMOUNT_BRIDGE_MISSING_HELPER,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("strict mode must reject helper-backed aggregate declarations when no entry calls the helper");

    assert!(err.message.contains("0.17 CKB source strict check failed"), "unexpected error: {}", err.message);
    assert!(err.message.contains("PP0170"), "unexpected error: {}", err.message);
    assert!(err.message.contains("runtime-helper-required:xudt::require_group_amount_conserved"), "unexpected error: {}", err.message);
}

#[test]
fn strict_0_17_accepts_xudt_group_amount_delta_aggregates_when_runtime_helpers_are_called() {
    let result = compile(
        XUDT_GROUP_AMOUNT_DELTA_BRIDGE,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("xUDT group amount delta aggregates should be discharged by explicit minted/burned runtime helpers");

    let proof_plan = &result.metadata.runtime.proof_plan;
    let mint_delta = proof_plan
        .iter()
        .find(|plan| plan.feature == "assert_delta:group_outputs<IckbToken>.amount:minted")
        .expect("mint delta aggregate ProofPlan record");
    assert_eq!(mint_delta.codegen_coverage_status, "gap:runtime-helper-required");
    assert!(
        mint_delta.coverage.iter().any(|coverage| coverage == "runtime_helper:xudt::require_group_amount_minted"),
        "{mint_delta:#?}"
    );
    assert!(
        mint_delta
            .input_output_relation_checks
            .iter()
            .any(|check| check.contains("runtime-helper-required:xudt::require_group_amount_minted")),
        "{mint_delta:#?}"
    );

    let burn_delta = proof_plan
        .iter()
        .find(|plan| plan.feature == "assert_delta:group_inputs<IckbToken>.amount:burned")
        .expect("burn delta aggregate ProofPlan record");
    assert_eq!(burn_delta.codegen_coverage_status, "gap:runtime-helper-required");
    assert!(
        burn_delta.coverage.iter().any(|coverage| coverage == "runtime_helper:xudt::require_group_amount_burned"),
        "{burn_delta:#?}"
    );
    assert!(
        burn_delta
            .input_output_relation_checks
            .iter()
            .any(|check| check.contains("runtime-helper-required:xudt::require_group_amount_burned")),
        "{burn_delta:#?}"
    );

    let accesses = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .map(|access| (access.operation.as_str(), access.syscall.as_str(), access.source.as_str()))
        .collect::<Vec<_>>();
    assert!(accesses.contains(&("xudt-group-amount-minted-delta", "LOAD_CELL_DATA", "GroupInput/GroupOutput")), "{accesses:?}");
    assert!(accesses.contains(&("xudt-group-amount-burned-delta", "LOAD_CELL_DATA", "GroupInput/GroupOutput")), "{accesses:?}");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    assert!(assembly.contains(".global __xudt_require_group_amount_minted"), "missing xUDT minted delta helper:\n{assembly}");
    assert!(assembly.contains(".global __xudt_require_group_amount_burned"), "missing xUDT burned delta helper:\n{assembly}");
}

#[test]
fn strict_0_17_rejects_xudt_group_amount_delta_without_runtime_helper_call() {
    let err = compile(
        XUDT_GROUP_AMOUNT_DELTA_MISSING_HELPER,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("strict mode must reject helper-backed delta declarations when no entry calls the matching helper");

    assert!(err.message.contains("0.17 CKB source strict check failed"), "unexpected error: {}", err.message);
    assert!(err.message.contains("PP0170"), "unexpected error: {}", err.message);
    assert!(err.message.contains("runtime-helper-required:xudt::require_group_amount_minted"), "unexpected error: {}", err.message);
}

#[test]
fn u128_local_values_are_materialized_for_runtime_delta_helpers() {
    let result = compile(
        U128_LOCAL_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("computed u128 local values should compile into addressable runtime helper inputs");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    assert!(assembly.contains("# cellscript abi: u128 add with carry"), "u128 add must be explicit 128-bit lowering:\n{assembly}");
    assert!(assembly.contains("# cellscript abi: u128 sub with borrow"), "u128 sub must be explicit 128-bit lowering:\n{assembly}");
    assert!(
        assembly.contains("# cellscript abi: checked u128 multiplication"),
        "u128 multiplication must use checked 128-bit lowering:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: checked u128 division by restoring long division"),
        "u128 division must use checked 128-bit lowering:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: u128 compare high limb first"),
        "u128 compare must inspect the high limb:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: return u128 via a0(low)/a1(high)"),
        "u128 helper returns must not return callee stack pointers:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: receive u128 return from a0(low)/a1(high)"),
        "u128 call destinations must materialize the returned limbs:\n{assembly}"
    );
    assert!(
        !assembly.contains("xUDT group amount delta is unavailable"),
        "computed u128 delta must be addressable by xUDT delta helper:\n{assembly}"
    );

    let elf = compile(
        U128_LOCAL_PROGRAM,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("computed u128 local lowering should assemble to ELF");
    assert!(elf.artifact_bytes.starts_with(b"\x7fELF"), "u128 local program should emit a RISC-V ELF artifact");
}

#[test]
fn c256_product_requirements_lower_to_executable_u128_mulhu_helpers() {
    let result = compile(
        C256_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("C256 u128 product requirements should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    for helper in [
        "__c256_require_u128_product_lte",
        "__c256_require_u128_product_eq",
        "__c256_require_u128_sum2_products_lte",
        "__c256_require_u128_sum2_products_eq",
        "__cellscript_mul_u128_to_u256",
        "__cellscript_add_u256",
        "__cellscript_load_u64_le",
    ] {
        assert!(assembly.contains(&format!(".global {helper}")), "missing helper {helper}:\n{assembly}");
    }
    assert!(assembly.contains("mulhu"), "C256 helper must use high-word multiply, not wrapping u64 math:\n{assembly}");
    assert!(assembly.contains("u128*u128 -> u256 limbs"), "C256 helper should expose auditable u256 limb lowering:\n{assembly}");
    assert!(
        assembly.contains("checked u256 addition"),
        "C256 sum helper should use checked u256 addition, not wrapping arithmetic:\n{assembly}"
    );
    assert!(
        assembly.contains(&format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code())),
        "C256 comparison failure must use stable AggregateAmountMismatch code:\n{assembly}"
    );
    assert!(
        result.metadata.runtime.ckb_runtime_features.contains(&"ckb-c256-product-arithmetic".to_string()),
        "{:?}",
        result.metadata.runtime.ckb_runtime_features
    );

    let elf = compile(
        C256_PROGRAM,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("internal ELF assembler should support C256 mulhu lowering");
    assert!(elf.artifact_bytes.starts_with(b"\x7fELF"), "C256 ELF output should be a RISC-V ELF artifact");
}

#[test]
fn signed_i32_lowers_to_fixed_width_signed_abi_values() {
    let result = compile(
        SIGNED_I32_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("signed i32 program should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    assert!(assembly.contains("scalar param left"), "i32 action param should use scalar entry ABI:\n{assembly}");
    assert!(
        assembly.contains("# cellscript abi: sign-extend i32"),
        "i32 values loaded from witness/aggregate bytes must be sign-extended before signed comparisons:\n{assembly}"
    );
    assert!(assembly.contains("srai"), "i32 sign extension should lower to an arithmetic shift:\n{assembly}");
    assert!(assembly.contains("field access .relative_index"), "i32 aggregate field access should compile:\n{assembly}");

    let action =
        result.metadata.actions.iter().find(|action| action.name == "signed_relative_order").expect("signed_relative_order metadata");
    assert_eq!(action.params[0].ty, "i32");
    assert_eq!(action.params[1].ty, "i32");
    let witness = action
        .entry_witness_args(&[cellscript::EntryWitnessArg::I32(-1), cellscript::EntryWitnessArg::I32(2)])
        .expect("i32 witness encoding");
    assert!(witness.ends_with(&[0xff, 0xff, 0xff, 0xff, 0x02, 0x00, 0x00, 0x00]), "{witness:02x?}");

    let elf = compile(
        SIGNED_I32_PROGRAM,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("internal ELF assembler should support i32 sign-extension lowering");
    assert!(elf.artifact_bytes.starts_with(b"\x7fELF"), "i32 ELF output should be a RISC-V ELF artifact");
}

#[test]
fn ickb_benchmark_specs_compile_under_0_17_strict_source_mode() {
    for file in ["ickb_logic.cell", "limit_order.cell", "owned_owner.cell"] {
        let result = compile_file(
            ickb_spec_path(file),
            CompileOptions {
                target: Some("riscv64-asm".to_string()),
                target_profile: Some("ckb".to_string()),
                primitive_compat: Some("0.17".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("{file} should compile under 0.17 strict mode: {}", err.message));

        let proof_status = &result.metadata.runtime.proof_plan_soundness.status;
        assert_eq!(proof_status, "passed", "{file} proof plan soundness failed");
    }
}

fn ickb_spec_path(file: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks").join("ickb_specs").join(file)
}

const WITNESS_SIZE_AND_REQUIRE_SOURCE: &str = r#"
module v017::witness_size_and_require

resource Wallet has store, create, consume {
    owner: Hash,
}

lock witness_size_lock(wallet: protected Wallet, claimed: witness Hash, min_size: u64) -> bool {
    verification
        let view = source::group_input(0)
        let sz = witness::size(view)
        ckb::require_witness_size_at_least(view, min_size)
        let raw = witness::raw(view)
        let lock_field = witness::lock(view)
        require raw == claimed
        require lock_field == claimed
}
"#;

#[test]
fn v0_17_witness_size_and_require_witness_size_at_least_compile_and_emit_metadata() {
    let result = compile(
        WITNESS_SIZE_AND_REQUIRE_SOURCE,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap_or_else(|err| panic!("witness size test should compile: {}", err.message));

    let features = &result.metadata.runtime.ckb_runtime_features;
    assert!(features.iter().any(|f| f == "ckb-witness-args"), "missing ckb-witness-args feature: {features:?}");

    let accesses = &result.metadata.runtime.ckb_runtime_accesses;
    assert!(
        accesses.iter().any(|a| {
            a.operation == "witness-size" && a.syscall == "LOAD_WITNESS" && a.source == "Witness" && a.binding == "witness::size"
        }),
        "missing witness-size access: {accesses:#?}"
    );
    assert!(
        accesses.iter().any(|a| {
            a.operation == "require-witness-size-at-least"
                && a.syscall == "LOAD_WITNESS"
                && a.source == "Witness"
                && a.binding == "ckb::require_witness_size_at_least"
        }),
        "missing require-witness-size-at-least access: {accesses:#?}"
    );
    assert!(
        accesses.iter().any(|a| {
            a.operation == "witness-raw" && a.syscall == "LOAD_WITNESS" && a.source == "Witness" && a.binding == "witness::raw"
        }),
        "missing witness-raw access: {accesses:#?}"
    );
    assert!(
        accesses.iter().any(|a| {
            a.operation == "witness-lock"
                && a.syscall == "LOAD_WITNESS_ARGS_LOCK"
                && a.source == "GroupInput"
                && a.binding == "witness::lock"
        }),
        "missing witness-lock access: {accesses:#?}"
    );

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    assert!(assembly.contains(".global __ckb_witness_size"), "missing __ckb_witness_size helper:\n{assembly}");
    assert!(
        assembly.contains(".global __ckb_require_witness_size_at_least"),
        "missing __ckb_require_witness_size_at_least helper:\n{assembly}"
    );
    assert!(assembly.contains(".global __ckb_witness_raw"), "missing __ckb_witness_raw helper:\n{assembly}");
    assert!(assembly.contains(".global __ckb_witness_lock"), "missing __ckb_witness_lock helper:\n{assembly}");
    assert!(
        assembly.contains("# cellscript abi: preserve min_size before LOAD_WITNESS size probe"),
        "require_witness_size_at_least does not preserve min_size:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: LOAD_WITNESS raw first 32 bytes into caller buffer"),
        "witness::raw helper does not use the corrected LOAD_WITNESS caller-buffer ABI:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: zero-fill extracted WitnessArgs Hash buffer before parsing"),
        "WitnessArgs field helper does not pre-zero short/empty Hash output:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: WitnessArgs total_size must match loaded witness size"),
        "WitnessArgs field helper does not validate table total_size:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: BytesOpt None leaves pre-zeroed Hash buffer"),
        "WitnessArgs field helper does not preserve BytesOpt None as an empty field:\n{assembly}"
    );

    let proof_status = &result.metadata.runtime.proof_plan_soundness.status;
    assert_eq!(proof_status, "passed", "proof plan soundness failed");
}
