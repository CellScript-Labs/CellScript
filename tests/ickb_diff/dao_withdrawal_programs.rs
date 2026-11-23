const DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION: &str = "test_dao_withdrawal_capacity";
const DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_withdrawal_capacity;

action test_dao_withdrawal_capacity() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        dao::require_input_since_at_least(input, 2306942530136048371);
        dao::require_header_dep_for_input(input, source::header_dep(0));
        let input_capacity = ckb::cell_capacity(input);
        let occupied_capacity = ckb::cell_occupied_capacity(input);
        let withdraw_rate = dao::input_accumulated_rate(input);
        if withdraw_rate == 0 {
            return 40;
        }
        let deposit_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit_header_rate == 0 {
            return 41;
        }
        let withdrawable_capacity = input_capacity - occupied_capacity;
        let compensated_capacity = (withdrawable_capacity * withdraw_rate) / deposit_header_rate;
        let max_output_capacity = occupied_capacity + compensated_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_TWO_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION: &str = "test_dao_two_input_withdrawal_capacity";
const DAO_TWO_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_two_input_withdrawal_capacity;

action test_dao_two_input_withdrawal_capacity() -> u64 {

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
const DAO_THREE_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION: &str = "test_dao_three_input_withdrawal_capacity";
const DAO_THREE_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_withdrawal_capacity;

action test_dao_three_input_withdrawal_capacity() -> u64 {

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

        let input2_capacity = ckb::cell_capacity(input2);
        let occupied2_capacity = ckb::cell_occupied_capacity(input2);
        let withdraw2_rate = dao::input_accumulated_rate(input2);
        if withdraw2_rate == 0 {
            return 43;
        }
        let withdrawable2_capacity = input2_capacity - occupied2_capacity;
        let compensated2_capacity = (withdrawable2_capacity * withdraw2_rate) / deposit_header_rate;
        let max2_output_capacity = occupied2_capacity + compensated2_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity + max2_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
