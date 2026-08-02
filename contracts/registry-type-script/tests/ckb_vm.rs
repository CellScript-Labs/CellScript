use std::path::PathBuf;

use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_types::{
        bytes::Bytes,
        core::TransactionBuilder,
        packed::{CellInput, CellOutput, Script},
        prelude::*,
    },
    context::Context,
};

const MAX_CYCLES: u64 = 10_000_000;
const CELL_CAPACITY: u64 = 20_000_000_000;

struct Scripts {
    context: Context,
    lock: Script,
    other_lock: Script,
    registry_type: Script,
}

fn contract_binary() -> Bytes {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/riscv64imac-unknown-none-elf/release/cellscript-registry-type-script");
    std::fs::read(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}; run build_reproducible_release.sh first", path.display()))
        .into()
}

fn scripts(args: Option<Bytes>) -> Scripts {
    let mut context = Context::default();
    let lock_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());
    let lock = context.build_script(&lock_out_point, Bytes::new()).expect("always-success lock");
    let other_lock = context.build_script(&lock_out_point, Bytes::from_static(&[1])).expect("alternate lock");
    let type_out_point = context.deploy_cell(contract_binary());
    let args = args.unwrap_or_else(|| Bytes::copy_from_slice(lock.calc_script_hash().as_slice()));
    let registry_type = context.build_script(&type_out_point, args).expect("Registry Type Script");
    Scripts { context, lock, other_lock, registry_type }
}

fn commitment(seed: u8) -> Bytes {
    let mut data = b"CSREGv1".to_vec();
    data.extend([seed; 32]);
    data.into()
}

fn verify_creation(output_data: Bytes, args: Option<Bytes>, custody_input: bool, custody_output: bool) -> Result<u64, String> {
    let Scripts { mut context, lock, other_lock, registry_type } = scripts(args);
    let input_lock = if custody_input { lock.clone() } else { other_lock.clone() };
    let output_lock = if custody_output { lock } else { other_lock };
    let input_out_point =
        context.create_cell(CellOutput::new_builder().capacity(CELL_CAPACITY).lock(input_lock).build(), Bytes::new());
    let input = CellInput::new_builder().previous_output(input_out_point).build();
    let output = CellOutput::new_builder().capacity(CELL_CAPACITY).lock(output_lock).type_(Some(registry_type).pack()).build();
    let transaction = TransactionBuilder::default().input(input).output(output).output_data(output_data.pack()).build();
    let transaction = context.complete_tx(transaction);
    context.verify_tx(&transaction, MAX_CYCLES).map_err(|error| error.to_string())
}

#[test]
fn accepts_exact_commitment_data() {
    verify_creation(commitment(0x11), None, true, true).expect("valid commitment");
}

#[test]
fn rejects_wrong_magic_short_hash_and_trailing_bytes() {
    let mut wrong_magic = commitment(0x22).to_vec();
    wrong_magic[0] ^= 0xff;
    assert!(verify_creation(wrong_magic.into(), None, true, true).is_err());

    assert!(verify_creation(Bytes::from_static(b"CSREGv1"), None, true, true).is_err());

    let mut trailing = commitment(0x33).to_vec();
    trailing.push(0);
    assert!(verify_creation(trailing.into(), None, true, true).is_err());
}

#[test]
fn rejects_non_canonical_type_args() {
    assert!(verify_creation(commitment(0x44), Some(Bytes::new()), true, true).is_err());
    assert!(verify_creation(commitment(0x44), Some(Bytes::from(vec![1; 31])), true, true).is_err());
    assert!(verify_creation(commitment(0x44), Some(Bytes::from(vec![1; 33])), true, true).is_err());
}

#[test]
fn requires_custody_authorization_and_custody_locked_outputs() {
    assert!(verify_creation(commitment(0x45), None, false, true).is_err());
    assert!(verify_creation(commitment(0x46), None, true, false).is_err());
}

#[test]
fn accepts_replacement_and_destruction_but_rejects_malformed_input() {
    for replacement in [Some(commitment(0x66)), None] {
        let Scripts { mut context, lock, registry_type, .. } = scripts(None);
        let input_out_point = context.create_cell(
            CellOutput::new_builder().capacity(CELL_CAPACITY).lock(lock.clone()).type_(Some(registry_type.clone()).pack()).build(),
            commitment(0x55),
        );
        let input = CellInput::new_builder().previous_output(input_out_point).build();
        let mut builder = TransactionBuilder::default().input(input);
        if let Some(data) = replacement {
            builder = builder
                .output(
                    CellOutput::new_builder()
                        .capacity(CELL_CAPACITY)
                        .lock(lock.clone())
                        .type_(Some(registry_type.clone()).pack())
                        .build(),
                )
                .output_data(data.pack());
        }
        let transaction = context.complete_tx(builder.build());
        context.verify_tx(&transaction, MAX_CYCLES).expect("valid lifecycle transition");
    }

    let Scripts { mut context, lock, registry_type, .. } = scripts(None);
    let input_out_point = context.create_cell(
        CellOutput::new_builder().capacity(CELL_CAPACITY).lock(lock).type_(Some(registry_type).pack()).build(),
        Bytes::from_static(b"legacy-malformed"),
    );
    let transaction = TransactionBuilder::default().input(CellInput::new_builder().previous_output(input_out_point).build()).build();
    let transaction = context.complete_tx(transaction);
    assert!(context.verify_tx(&transaction, MAX_CYCLES).is_err());
}
