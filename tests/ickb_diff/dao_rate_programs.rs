const DAO_THREE_INPUT_MIXED_DEPOSIT_RATE_CELLSCRIPT_ACTION: &str = "test_dao_three_input_mixed_deposit_rate";
const DAO_THREE_INPUT_MIXED_DEPOSIT_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_mixed_deposit_rate;

action test_dao_three_input_mixed_deposit_rate() -> u64 {

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
        let deposit01_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit01_header_rate == 0 {
            return 41;
        }
        let deposit2_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit2_header_rate == 0 {
            return 44;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit01_header_rate;
        let max0_output_capacity = occupied0_capacity + compensated0_capacity;

        let input1_capacity = ckb::cell_capacity(input1);
        let occupied1_capacity = ckb::cell_occupied_capacity(input1);
        let withdraw1_rate = dao::input_accumulated_rate(input1);
        if withdraw1_rate == 0 {
            return 42;
        }
        let withdrawable1_capacity = input1_capacity - occupied1_capacity;
        let compensated1_capacity = (withdrawable1_capacity * withdraw1_rate) / deposit01_header_rate;
        let max1_output_capacity = occupied1_capacity + compensated1_capacity;

        let input2_capacity = ckb::cell_capacity(input2);
        let occupied2_capacity = ckb::cell_occupied_capacity(input2);
        let withdraw2_rate = dao::input_accumulated_rate(input2);
        if withdraw2_rate == 0 {
            return 43;
        }
        let withdrawable2_capacity = input2_capacity - occupied2_capacity;
        let compensated2_capacity = (withdrawable2_capacity * withdraw2_rate) / deposit2_header_rate;
        let max2_output_capacity = occupied2_capacity + compensated2_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity + max2_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_THREE_INPUT_MIXED_WITHDRAW_RATE_CELLSCRIPT_ACTION: &str = "test_dao_three_input_mixed_withdraw_rate";
const DAO_THREE_INPUT_MIXED_WITHDRAW_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_mixed_withdraw_rate;

action test_dao_three_input_mixed_withdraw_rate() -> u64 {

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
        dao::require_header_dep_for_input(input2, source::header_dep(2));
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
const DAO_THREE_INPUT_MIXED_BOTH_RATE_CELLSCRIPT_ACTION: &str = "test_dao_three_input_mixed_both_rate";
const DAO_THREE_INPUT_MIXED_BOTH_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_mixed_both_rate;

action test_dao_three_input_mixed_both_rate() -> u64 {

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
        dao::require_header_dep_for_input(input2, source::header_dep(3));
        let deposit01_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit01_header_rate == 0 {
            return 41;
        }
        let deposit2_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit2_header_rate == 0 {
            return 44;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit01_header_rate;
        let max0_output_capacity = occupied0_capacity + compensated0_capacity;

        let input1_capacity = ckb::cell_capacity(input1);
        let occupied1_capacity = ckb::cell_occupied_capacity(input1);
        let withdraw1_rate = dao::input_accumulated_rate(input1);
        if withdraw1_rate == 0 {
            return 42;
        }
        let withdrawable1_capacity = input1_capacity - occupied1_capacity;
        let compensated1_capacity = (withdrawable1_capacity * withdraw1_rate) / deposit01_header_rate;
        let max1_output_capacity = occupied1_capacity + compensated1_capacity;

        let input2_capacity = ckb::cell_capacity(input2);
        let occupied2_capacity = ckb::cell_occupied_capacity(input2);
        let withdraw2_rate = dao::input_accumulated_rate(input2);
        if withdraw2_rate == 0 {
            return 43;
        }
        let withdrawable2_capacity = input2_capacity - occupied2_capacity;
        let compensated2_capacity = (withdrawable2_capacity * withdraw2_rate) / deposit2_header_rate;
        let max2_output_capacity = occupied2_capacity + compensated2_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity + max2_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_THREE_INPUT_SECOND_MIXED_DEPOSIT_RATE_CELLSCRIPT_ACTION: &str = "test_dao_three_input_second_mixed_deposit_rate";
const DAO_THREE_INPUT_SECOND_MIXED_DEPOSIT_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_second_mixed_deposit_rate;

action test_dao_three_input_second_mixed_deposit_rate() -> u64 {

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
        let deposit02_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit02_header_rate == 0 {
            return 41;
        }
        let deposit1_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit1_header_rate == 0 {
            return 44;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit02_header_rate;
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

        let input2_capacity = ckb::cell_capacity(input2);
        let occupied2_capacity = ckb::cell_occupied_capacity(input2);
        let withdraw2_rate = dao::input_accumulated_rate(input2);
        if withdraw2_rate == 0 {
            return 43;
        }
        let withdrawable2_capacity = input2_capacity - occupied2_capacity;
        let compensated2_capacity = (withdrawable2_capacity * withdraw2_rate) / deposit02_header_rate;
        let max2_output_capacity = occupied2_capacity + compensated2_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity + max2_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_THREE_INPUT_SECOND_MIXED_WITHDRAW_RATE_CELLSCRIPT_ACTION: &str = "test_dao_three_input_second_mixed_withdraw_rate";
const DAO_THREE_INPUT_SECOND_MIXED_WITHDRAW_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_second_mixed_withdraw_rate;

action test_dao_three_input_second_mixed_withdraw_rate() -> u64 {

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
        dao::require_header_dep_for_input(input1, source::header_dep(2));
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
const DAO_THREE_INPUT_SECOND_MIXED_BOTH_RATE_CELLSCRIPT_ACTION: &str = "test_dao_three_input_second_mixed_both_rate";
const DAO_THREE_INPUT_SECOND_MIXED_BOTH_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_second_mixed_both_rate;

action test_dao_three_input_second_mixed_both_rate() -> u64 {

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
        dao::require_header_dep_for_input(input1, source::header_dep(3));
        dao::require_header_dep_for_input(input2, source::header_dep(0));
        let deposit02_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit02_header_rate == 0 {
            return 41;
        }
        let deposit1_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit1_header_rate == 0 {
            return 44;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit02_header_rate;
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

        let input2_capacity = ckb::cell_capacity(input2);
        let occupied2_capacity = ckb::cell_occupied_capacity(input2);
        let withdraw2_rate = dao::input_accumulated_rate(input2);
        if withdraw2_rate == 0 {
            return 43;
        }
        let withdrawable2_capacity = input2_capacity - occupied2_capacity;
        let compensated2_capacity = (withdrawable2_capacity * withdraw2_rate) / deposit02_header_rate;
        let max2_output_capacity = occupied2_capacity + compensated2_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity + max2_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_THREE_INPUT_SECOND_DEPOSIT_THIRD_WITHDRAW_RATE_CELLSCRIPT_ACTION: &str =
    "test_dao_three_input_second_deposit_third_withdraw_rate";
const DAO_THREE_INPUT_SECOND_DEPOSIT_THIRD_WITHDRAW_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_second_deposit_third_withdraw_rate;

action test_dao_three_input_second_deposit_third_withdraw_rate() -> u64 {

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
        dao::require_header_dep_for_input(input2, source::header_dep(3));
        let deposit02_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit02_header_rate == 0 {
            return 41;
        }
        let deposit1_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit1_header_rate == 0 {
            return 44;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit02_header_rate;
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

        let input2_capacity = ckb::cell_capacity(input2);
        let occupied2_capacity = ckb::cell_occupied_capacity(input2);
        let withdraw2_rate = dao::input_accumulated_rate(input2);
        if withdraw2_rate == 0 {
            return 43;
        }
        let withdrawable2_capacity = input2_capacity - occupied2_capacity;
        let compensated2_capacity = (withdrawable2_capacity * withdraw2_rate) / deposit02_header_rate;
        let max2_output_capacity = occupied2_capacity + compensated2_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity + max2_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
const DAO_THREE_INPUT_SECOND_WITHDRAW_THIRD_DEPOSIT_RATE_CELLSCRIPT_ACTION: &str =
    "test_dao_three_input_second_withdraw_third_deposit_rate";
const DAO_THREE_INPUT_SECOND_WITHDRAW_THIRD_DEPOSIT_RATE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_three_input_second_withdraw_third_deposit_rate;

action test_dao_three_input_second_withdraw_third_deposit_rate() -> u64 {

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
        dao::require_header_dep_for_input(input1, source::header_dep(3));
        dao::require_header_dep_for_input(input2, source::header_dep(0));
        let deposit01_header_rate = dao::accumulated_rate(source::header_dep(1));
        if deposit01_header_rate == 0 {
            return 41;
        }
        let deposit2_header_rate = dao::accumulated_rate(source::header_dep(2));
        if deposit2_header_rate == 0 {
            return 44;
        }

        let input0_capacity = ckb::cell_capacity(input0);
        let occupied0_capacity = ckb::cell_occupied_capacity(input0);
        let withdraw0_rate = dao::input_accumulated_rate(input0);
        if withdraw0_rate == 0 {
            return 40;
        }
        let withdrawable0_capacity = input0_capacity - occupied0_capacity;
        let compensated0_capacity = (withdrawable0_capacity * withdraw0_rate) / deposit01_header_rate;
        let max0_output_capacity = occupied0_capacity + compensated0_capacity;

        let input1_capacity = ckb::cell_capacity(input1);
        let occupied1_capacity = ckb::cell_occupied_capacity(input1);
        let withdraw1_rate = dao::input_accumulated_rate(input1);
        if withdraw1_rate == 0 {
            return 42;
        }
        let withdrawable1_capacity = input1_capacity - occupied1_capacity;
        let compensated1_capacity = (withdrawable1_capacity * withdraw1_rate) / deposit01_header_rate;
        let max1_output_capacity = occupied1_capacity + compensated1_capacity;

        let input2_capacity = ckb::cell_capacity(input2);
        let occupied2_capacity = ckb::cell_occupied_capacity(input2);
        let withdraw2_rate = dao::input_accumulated_rate(input2);
        if withdraw2_rate == 0 {
            return 43;
        }
        let withdrawable2_capacity = input2_capacity - occupied2_capacity;
        let compensated2_capacity = (withdrawable2_capacity * withdraw2_rate) / deposit2_header_rate;
        let max2_output_capacity = occupied2_capacity + compensated2_capacity;

        let max_output_capacity = max0_output_capacity + max1_output_capacity + max2_output_capacity;
        let output_capacity = ckb::cell_capacity(source::output(0));
        if output_capacity > max_output_capacity {
            return 48;
        }
        return 0;
}
"#;
