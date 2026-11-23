const LIMIT_ORDER_CELLSCRIPT_ACTION: &str = "test_limit_order_value";
const LIMIT_ORDER_CELLSCRIPT_PROGRAM: &str = r#"
module differential_limit_order;

action test_limit_order_value() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let output = source::output(0);
        ckb::require_lock_match_master_out_point_pairs_from_data(input, output, 16, 20, 52);
        let input_ckb = ckb::cell_capacity(input);
        let output_ckb = ckb::cell_capacity(output);
        let input_type_code_hash: Hash = ckb::cell_type_code_hash(input);
        let input_type_hash_type = ckb::cell_type_hash_type(input);
        let expected_type = script::new(input_type_code_hash, input_type_hash_type, script::args_empty());
        script::require_cell_type_matches(input, expected_type);
        script::require_cell_type_matches(output, expected_type);
        let input_udt = xudt::amount_low(input);
        let output_udt = xudt::amount_low(output);
        if output_ckb >= input_ckb {
            return 44;
        }
        if output_udt < input_udt {
            return 45;
        }
        if output_ckb + output_udt < input_ckb + input_udt {
            return 41;
        }
        if output_ckb > 0 {
            if input_ckb < output_ckb + 64 {
                return 43;
            }
        }
        return 0;
}
"#;
const LIMIT_ORDER_UDT_TO_CKB_CELLSCRIPT_ACTION: &str = "test_limit_order_udt_to_ckb_value";
const LIMIT_ORDER_UDT_TO_CKB_CELLSCRIPT_PROGRAM: &str = r#"
module differential_limit_order_udt_to_ckb;

action test_limit_order_udt_to_ckb_value() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let output = source::output(0);
        ckb::require_lock_match_master_out_point_pairs_from_data(input, output, 16, 20, 52);
        let input_ckb = ckb::cell_capacity(input);
        let output_ckb = ckb::cell_capacity(output);
        let input_type_code_hash: Hash = ckb::cell_type_code_hash(input);
        let input_type_hash_type = ckb::cell_type_hash_type(input);
        let expected_type = script::new(input_type_code_hash, input_type_hash_type, script::args_empty());
        script::require_cell_type_matches(input, expected_type);
        script::require_cell_type_matches(output, expected_type);
        let input_udt = xudt::amount_low(input);
        let output_udt = xudt::amount_low(output);
        if output_udt >= input_udt {
            return 54;
        }
        if output_ckb < input_ckb {
            return 55;
        }
        if output_ckb + output_udt < input_ckb + input_udt {
            return 51;
        }
        if output_udt > 0 {
            if input_udt < output_udt + 64 {
                return 53;
            }
        }
        return 0;
}
"#;
const OWNED_OWNER_CELLSCRIPT_ACTION: &str = "test_owned_owner_pairing";
const OWNED_OWNER_CELLSCRIPT_PROGRAM: &str = r#"
module differential_owned_owner;

action test_owned_owner_pairing() -> u64 {

        ckb::require_current_script_args_empty();
        ckb::require_type_lock_metapoint_pairs_from_i32_data(source::input(0), 0);
        return 0;
}
"#;
const OWNED_OWNER_OUTPUT_CELLSCRIPT_ACTION: &str = "test_owned_owner_output_pairing";
const OWNED_OWNER_OUTPUT_CELLSCRIPT_PROGRAM: &str = r#"
module differential_owned_owner_output;

action test_owned_owner_output_pairing() -> u64 {

        ckb::require_current_script_args_empty();
        ckb::require_type_lock_metapoint_pairs_from_i32_data(source::output(0), 0);
        return 0;
}
"#;
const OWNED_OWNER_SCRIPT_MISUSE_CELLSCRIPT_ACTION: &str = "test_owned_owner_script_misuse";
const OWNED_OWNER_SCRIPT_MISUSE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_owned_owner_script_misuse;

action test_owned_owner_script_misuse() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::input(0);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(input, current_script_hash);
        ckb::require_cell_type_hash(input, current_script_hash);
        return 7;
}
"#;
const OWNED_OWNER_OUTPUT_SCRIPT_MISUSE_CELLSCRIPT_ACTION: &str = "test_owned_owner_output_script_misuse";
const OWNED_OWNER_OUTPUT_SCRIPT_MISUSE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_owned_owner_output_script_misuse;

action test_owned_owner_output_script_misuse() -> u64 {

        ckb::require_current_script_args_empty();
        let output = source::output(0);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(output, current_script_hash);
        ckb::require_cell_type_hash(output, current_script_hash);
        return 7;
}
"#;
const OWNED_OWNER_OUTPUT_NOT_WITHDRAWAL_CELLSCRIPT_ACTION: &str = "test_owned_owner_output_not_withdrawal";
const OWNED_OWNER_OUTPUT_NOT_WITHDRAWAL_CELLSCRIPT_PROGRAM: &str = r#"
module differential_owned_owner_output_not_withdrawal;

action test_owned_owner_output_not_withdrawal() -> u64 {

        ckb::require_current_script_args_empty();
        let output = source::output(0);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(output, current_script_hash);
        let has_dao = dao::has_dao_type(output);
        if !has_dao {
            return 6;
        }
        let is_withdrawal = dao::is_withdrawal_request_data(output);
        if !is_withdrawal {
            return 6;
        }
        return 0;
}
"#;
const OWNED_OWNER_NOT_WITHDRAWAL_CELLSCRIPT_ACTION: &str = "test_owned_owner_not_withdrawal";
const OWNED_OWNER_NOT_WITHDRAWAL_CELLSCRIPT_PROGRAM: &str = r#"
module differential_owned_owner_not_withdrawal;

action test_owned_owner_not_withdrawal() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::input(0);
        let has_dao = dao::has_dao_type(input);
        if !has_dao {
            return 6;
        }
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 6;
        }
        return 0;
}
"#;

const OWNED_OWNER_RELATED_TYPE_HASH_MISMATCH_CELLSCRIPT_ACTION: &str = "test_owned_owner_related_type_hash_mismatch";

fn owned_owner_related_type_hash_mismatch_cellscript_program(expected_related_type_script: &packed::Script) -> String {
    let expected_related_type_script = cellscript_script_value_expr(expected_related_type_script);
    format!(
        r#"
module differential_owned_owner_related_type_hash_mismatch;

action test_owned_owner_related_type_hash_mismatch() -> u64 {{

        ckb::require_current_script_args_empty();
        let owned = source::input(0);
        let owner = source::input(1);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(owned, current_script_hash);
        ckb::require_cell_type_hash(owner, current_script_hash);
        let expected_related_type = {expected_related_type_script};
        script::require_cell_type_matches(owned, expected_related_type);
        ckb::require_type_lock_metapoint_pairs_from_i32_data(source::input(0), 0);
        return 0;
}}
"#
    )
}

const OWNED_OWNER_OUTPUT_RELATED_TYPE_HASH_MISMATCH_CELLSCRIPT_ACTION: &str = "test_owned_owner_output_related_type_hash_mismatch";

fn owned_owner_output_related_type_hash_mismatch_cellscript_program(expected_related_type_script: &packed::Script) -> String {
    let expected_related_type_script = cellscript_script_value_expr(expected_related_type_script);
    format!(
        r#"
module differential_owned_owner_output_related_type_hash_mismatch;

action test_owned_owner_output_related_type_hash_mismatch() -> u64 {{

        ckb::require_current_script_args_empty();
        let owned = source::output(0);
        let owner = source::output(1);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(owned, current_script_hash);
        ckb::require_cell_type_hash(owner, current_script_hash);
        let expected_related_type = {expected_related_type_script};
        script::require_cell_type_matches(owned, expected_related_type);
        ckb::require_type_lock_metapoint_pairs_from_i32_data(source::output(0), 0);
        return 0;
}}
"#
    )
}

const OWNED_OWNER_OUTPUT_RELATED_DATA_RULE_MISMATCH_CELLSCRIPT_ACTION: &str = "test_owned_owner_output_related_data_rule_mismatch";

fn owned_owner_output_related_data_rule_mismatch_cellscript_program(expected_related_type_script: &packed::Script) -> String {
    let expected_related_type_script = cellscript_script_value_expr(expected_related_type_script);
    format!(
        r#"
module differential_owned_owner_output_related_data_rule_mismatch;

action test_owned_owner_output_related_data_rule_mismatch() -> u64 {{

        ckb::require_current_script_args_empty();
        let owned = source::output(0);
        let owner = source::output(1);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(owned, current_script_hash);
        ckb::require_cell_type_hash(owner, current_script_hash);
        let expected_related_type = {expected_related_type_script};
        script::require_cell_type_matches(owned, expected_related_type);
        let is_withdrawal = dao::is_withdrawal_request_data(owned);
        if !is_withdrawal {{
            return 47;
        }}
        ckb::require_type_lock_metapoint_pairs_from_i32_data(source::output(0), 0);
        return 0;
}}
"#
    )
}

const OWNED_OWNER_RELATED_DATA_RULE_MISMATCH_CELLSCRIPT_ACTION: &str = "test_owned_owner_related_data_rule_mismatch";

fn owned_owner_related_data_rule_mismatch_cellscript_program(expected_related_type_script: &packed::Script) -> String {
    let expected_related_type_script = cellscript_script_value_expr(expected_related_type_script);
    format!(
        r#"
module differential_owned_owner_related_data_rule_mismatch;

action test_owned_owner_related_data_rule_mismatch() -> u64 {{

        ckb::require_current_script_args_empty();
        let owned = source::input(0);
        let owner = source::input(1);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(owned, current_script_hash);
        ckb::require_cell_type_hash(owner, current_script_hash);
        let expected_related_type = {expected_related_type_script};
        script::require_cell_type_matches(owned, expected_related_type);
        let is_withdrawal = dao::is_withdrawal_request_data(owned);
        if !is_withdrawal {{
            return 47;
        }}
        ckb::require_type_lock_metapoint_pairs_from_i32_data(source::input(0), 0);
        return 0;
}}
"#
    )
}
