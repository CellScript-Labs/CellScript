const DAO_THREE_INPUT_WITNESS_SHAPE_CELLSCRIPT_ACTION: &str = "test_dao_three_input_witness_shape";
const DAO_THREE_INPUT_WITNESS_SHAPE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_witness_shape;

action test_dao_three_input_witness_shape() -> u64 {

        ckb::require_current_script_args_empty();
        let input0 = source::group_input(0);
        let input1 = source::group_input(1);
        let input2 = source::group_input(2);
        let is_withdrawal0 = dao::is_withdrawal_request_data(input0);
        if !is_withdrawal0 {
            return 34;
        }
        let is_withdrawal1 = dao::is_withdrawal_request_data(input1);
        if !is_withdrawal1 {
            return 35;
        }
        let is_withdrawal2 = dao::is_withdrawal_request_data(input2);
        if !is_withdrawal2 {
            return 36;
        }
        dao::require_input_since_at_least(input0, 2306942530136048371);
        dao::require_input_since_at_least(input1, 2306942530136048371);
        dao::require_input_since_at_least(input2, 2306942530136048371);
        dao::require_header_dep_for_input(input0, source::header_dep(0));
        dao::require_header_dep_for_input(input1, source::header_dep(0));
        dao::require_header_dep_for_input(input2, source::header_dep(0));
        let witness0_bytes = witness::size(input0);
        if witness0_bytes != 28 {
            return 43;
        }
        let witness1_bytes = witness::size(input1);
        if witness1_bytes != 28 {
            return 44;
        }
        let witness2_bytes = witness::size(input2);
        if witness2_bytes != 28 {
            return 45;
        }
        let witness0_input_type = witness::u64_le(witness::bounded_entry(input0, 8), 0);
        if witness0_input_type == 0 {
            return 46;
        }
        let witness1_input_type = witness::u64_le(witness::bounded_entry(input1, 8), 0);
        if witness1_input_type == 0 {
            return 47;
        }
        let witness2_input_type = witness::u64_le(witness::bounded_entry(input2, 8), 0);
        if witness2_input_type == 0 {
            return 49;
        }
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const DAO_THREE_INPUT_WITNESS_INDEX_CELLSCRIPT_ACTION: &str = "test_dao_three_input_witness_index";
const DAO_THREE_INPUT_WITNESS_INDEX_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_witness_index;

action test_dao_three_input_witness_index() -> u64 {

        ckb::require_current_script_args_empty();
        let input0 = source::group_input(0);
        let input1 = source::group_input(1);
        let input2 = source::group_input(2);
        let is_withdrawal0 = dao::is_withdrawal_request_data(input0);
        if !is_withdrawal0 {
            return 34;
        }
        let is_withdrawal1 = dao::is_withdrawal_request_data(input1);
        if !is_withdrawal1 {
            return 35;
        }
        let is_withdrawal2 = dao::is_withdrawal_request_data(input2);
        if !is_withdrawal2 {
            return 36;
        }
        dao::require_input_since_at_least(input0, 2306942530136048371);
        dao::require_input_since_at_least(input1, 2306942530136048371);
        dao::require_input_since_at_least(input2, 2306942530136048371);
        dao::require_header_dep_for_input(input0, source::header_dep(0));
        dao::require_header_dep_for_input(input1, source::header_dep(0));
        dao::require_header_dep_for_input(input2, source::header_dep(0));
        let witness0_bytes = witness::size(input0);
        if witness0_bytes != 28 {
            return 43;
        }
        let witness1_bytes = witness::size(input1);
        if witness1_bytes != 28 {
            return 44;
        }
        let witness2_bytes = witness::size(input2);
        if witness2_bytes != 28 {
            return 45;
        }
        let expected_deposit_header_index = 1;
        let witness1_input_type = witness::u64_le(witness::bounded_entry(input1, 8), 0);
        if witness1_input_type != expected_deposit_header_index {
            return 48;
        }
        let witness2_input_type = witness::u64_le(witness::bounded_entry(input2, 8), 0);
        if witness2_input_type != expected_deposit_header_index {
            return 46;
        }
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const DAO_TWO_INPUT_WITNESS_SHAPE_CELLSCRIPT_ACTION: &str = "test_dao_two_input_witness_shape";
const DAO_TWO_INPUT_WITNESS_SHAPE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_two_input_witness_shape;

action test_dao_two_input_witness_shape() -> u64 {

        ckb::require_current_script_args_empty();
        let input0 = source::group_input(0);
        let input1 = source::group_input(1);
        let is_withdrawal0 = dao::is_withdrawal_request_data(input0);
        if !is_withdrawal0 {
            return 34;
        }
        let is_withdrawal1 = dao::is_withdrawal_request_data(input1);
        if !is_withdrawal1 {
            return 35;
        }
        dao::require_input_since_at_least(input0, 2306942530136048371);
        dao::require_input_since_at_least(input1, 2306942530136048371);
        dao::require_header_dep_for_input(input0, source::header_dep(0));
        dao::require_header_dep_for_input(input1, source::header_dep(0));
        let witness0_bytes = witness::size(input0);
        if witness0_bytes != 28 {
            return 43;
        }
        let witness1_bytes = witness::size(input1);
        if witness1_bytes != 28 {
            return 44;
        }
        let witness0_input_type = witness::u64_le(witness::bounded_entry(input0, 8), 0);
        if witness0_input_type == 0 {
            return 42;
        }
        let witness1_input_type = witness::u64_le(witness::bounded_entry(input1, 8), 0);
        if witness1_input_type == 0 {
            return 45;
        }
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const DAO_TWO_INPUT_WITNESS_INDEX_CELLSCRIPT_ACTION: &str = "test_dao_two_input_witness_index";
const DAO_TWO_INPUT_WITNESS_INDEX_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_two_input_witness_index;

action test_dao_two_input_witness_index() -> u64 {

        ckb::require_current_script_args_empty();
        let input0 = source::group_input(0);
        let input1 = source::group_input(1);
        let is_withdrawal0 = dao::is_withdrawal_request_data(input0);
        if !is_withdrawal0 {
            return 34;
        }
        let is_withdrawal1 = dao::is_withdrawal_request_data(input1);
        if !is_withdrawal1 {
            return 35;
        }
        dao::require_input_since_at_least(input0, 2306942530136048371);
        dao::require_input_since_at_least(input1, 2306942530136048371);
        dao::require_header_dep_for_input(input0, source::header_dep(0));
        dao::require_header_dep_for_input(input1, source::header_dep(0));
        let witness0_bytes = witness::size(input0);
        if witness0_bytes != 28 {
            return 43;
        }
        let witness1_bytes = witness::size(input1);
        if witness1_bytes != 28 {
            return 44;
        }
        let expected_deposit_header_index = 1;
        let witness1_input_type = witness::u64_le(witness::bounded_entry(input1, 8), 0);
        if witness1_input_type != expected_deposit_header_index {
            return 46;
        }
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const DAO_TWO_INPUT_MIXED_DEPOSIT_RATE_CELLSCRIPT_ACTION: &str = "test_dao_two_input_mixed_deposit_rate";
const DAO_TWO_INPUT_MIXED_DEPOSIT_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_two_input_mixed_deposit_rate;

action test_dao_two_input_mixed_deposit_rate() -> u64 {

        ckb::require_current_script_args_empty();
        let input0 = source::group_input(0);
        let input1 = source::group_input(1);
        let is_withdrawal0 = dao::is_withdrawal_request_data(input0);
        if !is_withdrawal0 {
            return 34;
        }
        let is_withdrawal1 = dao::is_withdrawal_request_data(input1);
        if !is_withdrawal1 {
            return 35;
        }
        dao::require_input_since_at_least(input0, 2306942530136048371);
        dao::require_input_since_at_least(input1, 2306942530136048371);
        dao::require_header_dep_for_input(input0, source::header_dep(0));
        dao::require_header_dep_for_input(input1, source::header_dep(0));

        let deposit0_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit0_header_rate == 0 {
            return 41;
        }
        let deposit1_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit1_header_rate == 0 {
            return 43;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit0_header_rate;
        let max0_output_capacity = occupied0_capacity + compensated0_capacity;

        let input1_capacity = ckb::cell_capacity(input1);
        let occupied1_capacity = ckb::cell_occupied_capacity(input1);
        let withdraw1_rate = dao::input_accumulated_rate(input1);
        if withdraw1_rate == 0 {
            return 42;
        }
        let withdrawable1_capacity = input1_capacity - occupied1_capacity;
        let compensated1_capacity = (withdrawable1_capacity * withdraw1_rate) / deposit1_header_rate;
        let max1_output_capacity = occupied1_capacity + compensated1_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_TWO_INPUT_MIXED_WITHDRAW_RATE_CELLSCRIPT_ACTION: &str = "test_dao_two_input_mixed_withdraw_rate";
const DAO_TWO_INPUT_MIXED_WITHDRAW_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_two_input_mixed_withdraw_rate;

action test_dao_two_input_mixed_withdraw_rate() -> u64 {

        ckb::require_current_script_args_empty();
        let input0 = source::group_input(0);
        let input1 = source::group_input(1);
        let is_withdrawal0 = dao::is_withdrawal_request_data(input0);
        if !is_withdrawal0 {
            return 34;
        }
        let is_withdrawal1 = dao::is_withdrawal_request_data(input1);
        if !is_withdrawal1 {
            return 35;
        }
        dao::require_input_since_at_least(input0, 2306942530136048371);
        dao::require_input_since_at_least(input1, 2306942530136048371);
        dao::require_header_dep_for_input(input0, source::header_dep(0));
        dao::require_header_dep_for_input(input1, source::header_dep(2));

        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate == 0 {
            return 41;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit_header_rate;
        let max0_output_capacity = occupied0_capacity + compensated0_capacity;

        let input1_capacity = ckb::cell_capacity(input1);
        let occupied1_capacity = ckb::cell_occupied_capacity(input1);
        let withdraw1_rate = dao::input_accumulated_rate(input1);
        if withdraw1_rate == 0 {
            return 42;
        }
        let withdrawable1_capacity = input1_capacity - occupied1_capacity;
        let compensated1_capacity = (withdrawable1_capacity * withdraw1_rate) / deposit_header_rate;
        let max1_output_capacity = occupied1_capacity + compensated1_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_TWO_INPUT_MIXED_BOTH_RATE_CELLSCRIPT_ACTION: &str = "test_dao_two_input_mixed_both_rate";
const DAO_TWO_INPUT_MIXED_BOTH_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_two_input_mixed_both_rate;

action test_dao_two_input_mixed_both_rate() -> u64 {

        ckb::require_current_script_args_empty();
        let input0 = source::group_input(0);
        let input1 = source::group_input(1);
        let is_withdrawal0 = dao::is_withdrawal_request_data(input0);
        if !is_withdrawal0 {
            return 34;
        }
        let is_withdrawal1 = dao::is_withdrawal_request_data(input1);
        if !is_withdrawal1 {
            return 35;
        }
        dao::require_input_since_at_least(input0, 2306942530136048371);
        dao::require_input_since_at_least(input1, 2306942530136048371);
        dao::require_header_dep_for_input(input0, source::header_dep(0));
        dao::require_header_dep_for_input(input1, source::header_dep(3));

        let deposit0_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit0_header_rate == 0 {
            return 41;
        }
        let deposit1_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit1_header_rate == 0 {
            return 43;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit0_header_rate;
        let max0_output_capacity = occupied0_capacity + compensated0_capacity;

        let input1_capacity = ckb::cell_capacity(input1);
        let occupied1_capacity = ckb::cell_occupied_capacity(input1);
        let withdraw1_rate = dao::input_accumulated_rate(input1);
        if withdraw1_rate == 0 {
            return 42;
        }
        let withdrawable1_capacity = input1_capacity - occupied1_capacity;
        let compensated1_capacity = (withdrawable1_capacity * withdraw1_rate) / deposit1_header_rate;
        let max1_output_capacity = occupied1_capacity + compensated1_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_WITHDRAWAL_HEADER_LINEAGE_CELLSCRIPT_ACTION: &str = "test_dao_withdrawal_header_lineage";
const DAO_WITHDRAWAL_HEADER_LINEAGE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_withdrawal_header_lineage;

action test_dao_withdrawal_header_lineage() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        dao::require_input_since_at_least(input, 2306942530136048371);
        let withdraw_rate = dao::input_accumulated_rate(input);
        if withdraw_rate != 10001000 {
            return 40;
        }
        return 0;
}
"#;
const DAO_WITHDRAWAL_DEPOSIT_HEADER_WITNESS_CELLSCRIPT_ACTION: &str = "test_dao_withdrawal_deposit_header_witness";
const DAO_WITHDRAWAL_DEPOSIT_HEADER_WITNESS_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_withdrawal_deposit_header_witness;

action test_dao_withdrawal_deposit_header_witness() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        dao::require_input_since_at_least(input, 2306942530136048371);
        dao::require_header_dep_for_input(input, source::header_dep(0));
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(0));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_CELLSCRIPT_ACTION: &str = "test_dao_withdrawal_witness_input_type";
const DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_withdrawal_witness_input_type;

action test_dao_withdrawal_witness_input_type() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        dao::require_input_since_at_least(input, 2306942530136048371);
        dao::require_header_dep_for_input(input, source::header_dep(0));
        let deposit_header_index = witness::u64_le(witness::bounded_entry(input, 8), 0);
        if deposit_header_index == 0 {
            return 42;
        }
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_WIDTH_CELLSCRIPT_ACTION: &str = "test_dao_withdrawal_witness_input_type_width";
const DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_WIDTH_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_withdrawal_witness_input_type_width;

action test_dao_withdrawal_witness_input_type_width() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        dao::require_input_since_at_least(input, 2306942530136048371);
        dao::require_header_dep_for_input(input, source::header_dep(0));
        let witness_bytes = witness::size(input);
        if witness_bytes != 28 {
            return 43;
        }
        let deposit_header_index = witness::u64_le(witness::bounded_entry(input, 8), 0);
        if deposit_header_index == 0 {
            return 42;
        }
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const DAO_WITHDRAWAL_DEPOSIT_HEADER_CELLSCRIPT_ACTION: &str = "test_dao_withdrawal_deposit_header";
const DAO_WITHDRAWAL_DEPOSIT_HEADER_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_withdrawal_deposit_header;

action test_dao_withdrawal_deposit_header() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        dao::require_input_since_at_least(input, 2306942530136048371);
        dao::require_header_dep_for_input(input, source::header_dep(0));
        let withdraw_rate = dao::input_accumulated_rate(input);
        if withdraw_rate != 10001000 {
            return 40;
        }
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const DAO_WITHDRAWAL_DEPOSIT_HEADER_OOB_CELLSCRIPT_ACTION: &str = "test_dao_withdrawal_deposit_header_oob";
const DAO_WITHDRAWAL_DEPOSIT_HEADER_OOB_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_withdrawal_deposit_header_oob;

action test_dao_withdrawal_deposit_header_oob() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        dao::require_input_since_at_least(input, 2306942530136048371);
        dao::require_header_dep_for_input(input, source::header_dep(0));
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit_header_rate != 10000000 {
            return 41;
        }
        return 0;
}
"#;
const IMMATURE_REDEEM_CELLSCRIPT_PROGRAM: &str = r#"
module ckb_vm_immature_redeem;

action test_immature_redeem_since() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        let required_since = ckb::since_epoch_relative(360, 0, 1);
        dao::require_input_since_at_least(input, required_since);
        dao::require_input_relative_epoch_since_at_least(input, 360, 0, 1);
        return 0;
}
"#;
