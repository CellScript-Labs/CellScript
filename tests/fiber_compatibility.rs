use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
use ckb_testtool::context::Context;

const MAX_CYCLES: u64 = 20_000_000;
const XUDT_COMPATIBLE_WITNESS: [u8; 16] = [16, 0, 0, 0, 16, 0, 0, 0, 16, 0, 0, 0, 16, 0, 0, 0];

const FUNGIBLE_SOURCE: &str = r#"
module fiber_fungible

invariant quantity_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<ChannelAsset>.quantity, group_outputs<ChannelAsset>.quantity
    assert_sum(group_outputs<ChannelAsset>.quantity) == assert_sum(group_inputs<ChannelAsset>.quantity)
}

resource ChannelAsset {
    quantity: u128,
}

action application_only(asset: ChannelAsset, to: Address) -> next: ChannelAsset {
    verification
    consume asset
    create next = ChannelAsset { quantity: asset.quantity } with_lock(to)
}
"#;

const ALWAYS_SUCCESS_SOURCE: &str = r#"
module fiber_always_success

action always_success() -> u64 {
    verification
    return 0
}
"#;

#[test]
fn fungible_type_group_entry_accepts_real_n_to_m_shapes_and_fiber_witness() {
    for (inputs, outputs) in [(vec![100], vec![30, 70]), (vec![30, 70], vec![100]), (vec![50, 60], vec![20, 30, 60])] {
        let result = execute_amounts(
            inputs.into_iter().map(amount_data).collect(),
            outputs.into_iter().map(amount_data).collect(),
            true,
            false,
        );
        assert!(result.is_ok(), "valid fungible group shape rejected: {result:#?}");
    }
}

#[test]
fn fungible_type_group_entry_accepts_owner_authorized_issuance_and_destruction() {
    for (inputs, outputs, name) in [
        (Vec::new(), vec![amount_data(100)], "owner-authorized issuance"),
        (vec![amount_data(100)], Vec::new(), "owner-authorized destruction"),
    ] {
        let result = execute_amounts(inputs, outputs, true, true);
        assert!(result.is_ok(), "{name} rejected: {result:#?}");
    }
}

#[test]
fn fungible_type_group_entry_accepts_tagged_policy_type_authority() {
    for (inputs, outputs, name) in [
        (Vec::new(), vec![amount_data(100)], "policy-authorized issuance"),
        (vec![amount_data(100)], Vec::new(), "policy-authorized destruction"),
    ] {
        let result = execute_amounts_with_type_authority(inputs, outputs, true);
        assert!(result.is_ok(), "{name} rejected: {result:#?}");
    }
    let result = execute_amounts_with_type_authority(Vec::new(), vec![amount_data(100)], false);
    assert!(result.is_err(), "tagged Type Script authority passed without a matching input");
}

#[test]
fn fungible_type_group_entry_keeps_codec_and_overflow_checks_in_owner_mode() {
    let cases = [
        (Vec::new(), vec![Bytes::from(vec![0u8; 15])], "owner issuance with short data"),
        (vec![Bytes::from(vec![0u8; 17])], Vec::new(), "owner destruction with long data"),
        (Vec::new(), vec![amount_data(u128::MAX), amount_data(1)], "owner issuance with checked output sum overflow"),
    ];
    for (inputs, outputs, name) in cases {
        let result = execute_amounts(inputs, outputs, false, true);
        assert!(result.is_err(), "{name} unexpectedly passed");
    }
}

#[test]
fn fungible_type_group_entry_rejects_mismatch_malformed_unauthorized_mint_burn_and_overflow() {
    let cases = [
        (vec![amount_data(100)], vec![amount_data(99)], "amount mismatch"),
        (vec![Bytes::from(vec![0u8; 15])], vec![amount_data(0)], "short data"),
        (vec![Bytes::from(vec![0u8; 17])], vec![amount_data(0)], "long data"),
        (Vec::new(), vec![amount_data(0)], "mint with empty input group"),
        (vec![amount_data(0)], Vec::new(), "burn with empty output group"),
        (vec![amount_data(u128::MAX), amount_data(1)], vec![amount_data(u128::MAX), amount_data(1)], "checked input sum overflow"),
    ];
    for (inputs, outputs, name) in cases {
        let result = execute_amounts(inputs, outputs, false, false);
        assert!(result.is_err(), "{name} unexpectedly passed");
    }

    for owner_args_len in [31usize, 33] {
        let result = execute_amounts_with_owner_args(
            vec![amount_data(1)],
            vec![amount_data(1)],
            false,
            false,
            Bytes::from(vec![0x42; owner_args_len]),
        );
        assert!(result.is_err(), "{owner_args_len}-byte owner args unexpectedly passed");
    }
}

fn execute_amounts(
    input_data: Vec<Bytes>,
    output_data: Vec<Bytes>,
    fiber_witness: bool,
    owner_authorized: bool,
) -> Result<u64, ckb_testtool::ckb_error::Error> {
    execute_amounts_with_optional_owner_args(input_data, output_data, fiber_witness, owner_authorized, None, false, false)
}

fn execute_amounts_with_owner_args(
    input_data: Vec<Bytes>,
    output_data: Vec<Bytes>,
    fiber_witness: bool,
    owner_authorized: bool,
    owner_args: Bytes,
) -> Result<u64, ckb_testtool::ckb_error::Error> {
    execute_amounts_with_optional_owner_args(input_data, output_data, fiber_witness, owner_authorized, Some(owner_args), false, false)
}

fn execute_amounts_with_type_authority(
    input_data: Vec<Bytes>,
    output_data: Vec<Bytes>,
    policy_authorized: bool,
) -> Result<u64, ckb_testtool::ckb_error::Error> {
    execute_amounts_with_optional_owner_args(input_data, output_data, false, false, None, true, policy_authorized)
}

fn execute_amounts_with_optional_owner_args(
    input_data: Vec<Bytes>,
    output_data: Vec<Bytes>,
    fiber_witness: bool,
    owner_authorized: bool,
    owner_args: Option<Bytes>,
    tagged_type_authority: bool,
    policy_authorized: bool,
) -> Result<u64, ckb_testtool::ckb_error::Error> {
    let mut context = Context::new_with_deterministic_rng();
    let fungible_out_point = context.deploy_cell(Bytes::from(compile_fungible_elf()));
    let lock_out_point = context.deploy_cell(Bytes::from(compile_always_success_elf()));
    let normal_lock = context.build_script(&lock_out_point, Bytes::from(vec![0x01])).expect("normal always-success lock");
    let owner_lock = context.build_script(&lock_out_point, Bytes::from(vec![0x02])).expect("owner always-success lock");
    let owner_lock_hash = Bytes::copy_from_slice(owner_lock.calc_script_hash().as_slice());
    let policy_script = context.build_script(&lock_out_point, Bytes::from(vec![0x03])).expect("policy type script");
    let authority_args = if tagged_type_authority {
        let mut args = vec![1u8];
        args.extend_from_slice(policy_script.calc_script_hash().as_slice());
        Bytes::from(args)
    } else {
        owner_args.unwrap_or(owner_lock_hash)
    };
    let fungible_script = context.build_script(&fungible_out_point, authority_args).expect("fungible type script");
    let capacity_lock = if owner_authorized { owner_lock.clone() } else { normal_lock.clone() };

    let capacity_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(1_000_000_000_000u64.pack())
        .lock(capacity_lock)
        .type_(if policy_authorized { packed::ScriptOpt::from(policy_script) } else { packed::ScriptOpt::default() })
        .build();
    let capacity_input = context.create_cell(capacity_output, Bytes::default());
    let mut input_out_points = vec![capacity_input];
    for data in input_data {
        input_out_points.push(
            context.create_cell(
                packed::CellOutput::new_builder()
                    .capacity::<packed::Uint64>(100_000_000_000u64.pack())
                    .lock(normal_lock.clone())
                    .type_(packed::ScriptOpt::from(fungible_script.clone()))
                    .build(),
                data,
            ),
        );
    }

    let mut outputs =
        vec![packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000u64.pack()).lock(normal_lock.clone()).build()];
    let mut outputs_data = vec![Bytes::default()];
    for data in output_data {
        outputs.push(
            packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(100_000_000_000u64.pack())
                .lock(normal_lock.clone())
                .type_(packed::ScriptOpt::from(fungible_script.clone()))
                .build(),
        );
        outputs_data.push(data);
    }

    let mut witnesses = vec![Bytes::default(); input_out_points.len()];
    if fiber_witness && witnesses.len() > 1 {
        witnesses[1] = Bytes::from(XUDT_COMPATIBLE_WITNESS.to_vec());
    }
    let tx = TransactionBuilder::default()
        .inputs(input_out_points.into_iter().map(|out_point| packed::CellInput::new_builder().previous_output(out_point).build()))
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .witnesses(witnesses.pack())
        .build();
    let tx = context.complete_tx(tx);
    context.verify_tx(&tx, MAX_CYCLES)
}

fn amount_data(amount: u128) -> Bytes {
    Bytes::from(amount.to_le_bytes().to_vec())
}

fn compile_fungible_elf() -> Vec<u8> {
    let result = cellscript::compile_fungible_type_group_entry(
        FUNGIBLE_SOURCE,
        cellscript::CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..Default::default()
        },
    )
    .expect("compile fungible entry");
    cellscript::strip_vm_abi_trailer(&result.artifact_bytes).to_vec()
}

fn compile_always_success_elf() -> Vec<u8> {
    let result = cellscript::compile(
        ALWAYS_SUCCESS_SOURCE,
        cellscript::CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
    )
    .expect("compile always-success lock");
    cellscript::strip_vm_abi_trailer(&result.artifact_bytes).to_vec()
}
