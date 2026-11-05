//! A protected Cell belongs to the running Lock group, not transaction input 0.
//!
//! The numeric predicate is only a source-binding fixture, not authentication.

use cellscript::{compile, strip_vm_abi_trailer, CellScriptEdition, CompileOptions, CompileResult};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

const ORDINARY_LOCK: &str = r#"
module protected_group::ordinary
resource Wallet { value: u64 }
lock permit(protected wallet: Wallet) -> bool {
    verification
        wallet.value == 7
}
"#;

const NATIVE_LOCK: &str = r#"
module protected_group::native
resource Wallet { value: u64 }
lock_script WalletPolicy on lock_group {
    lock permit(protected wallet: Wallet from group_input[0]) {
        require wallet.value == 7
    }
}
"#;

const FOREIGN_LOCK: &str = r#"
module protected_group::foreign
lock permit() -> bool { verification true }
"#;

fn compile_lock(source: &str, edition: CellScriptEdition) -> CompileResult {
    let result = compile(
        source,
        CompileOptions {
            edition,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
    )
    .expect("Lock binding fixture compiles");
    result.validate().expect("Lock binding bundle independently validates");
    assert_eq!(result.metadata.typed_semantics.foundation.entry_contract.script_role, "lock");
    result
}

fn execute_protected(
    protected: &CompileResult,
    foreign: &CompileResult,
    protected_value: u64,
    leading_foreign_value: Option<u64>,
) -> Result<u64, String> {
    let mut context = Context::new_with_deterministic_rng();
    let protected_code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&protected.artifact_bytes)));
    let protected_script = context.build_script(&protected_code, Bytes::new()).unwrap();
    let foreign_code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&foreign.artifact_bytes)));
    let foreign_script = context.build_script(&foreign_code, Bytes::new()).unwrap();
    assert_ne!(protected_script.calc_script_hash(), foreign_script.calc_script_hash());

    let mut cells = Vec::new();
    if let Some(value) = leading_foreign_value {
        cells.push((foreign_script, value));
    }
    cells.push((protected_script, protected_value));

    let mut transaction = TransactionBuilder::default();
    for (lock, value) in cells {
        let output = packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000_000u64.pack()).lock(lock).build();
        assert!(output.type_().to_opt().is_none(), "the program must execute as a Lock, never a Type Script");
        let data = Bytes::copy_from_slice(&value.to_le_bytes());
        let out_point = context.create_cell(output.clone(), data.clone());
        transaction = transaction
            .input(packed::CellInput::new_builder().previous_output(out_point).build())
            .output(output)
            .output_data(data.pack());
    }
    let transaction = context.complete_tx(transaction.build());
    context.verify_tx(&transaction, 10_000_000).map_err(|error| format!("{error:?}"))
}

fn assert_current_lock_group(protected: &CompileResult, foreign: &CompileResult) {
    assert!(execute_protected(protected, foreign, 7, None).expect("valid index-zero Lock group") > 0);
    assert!(execute_protected(protected, foreign, 8, None).is_err(), "invalid index-zero protected value must reject");
    assert!(
        execute_protected(protected, foreign, 7, Some(8)).expect("a different preceding Lock group's data must not be checked") > 0
    );
    let rejection = execute_protected(protected, foreign, 8, Some(7))
        .expect_err("valid foreign input[0] must not disguise invalid protected group-input[0] at transaction input[1]");
    assert!(rejection.contains("error code 5") || rejection.contains("error code: 5"), "unexpected rejection: {rejection}");
}

#[test]
fn ordinary_protected_lock_uses_current_group_in_edition_2027() {
    let foreign = compile_lock(FOREIGN_LOCK, CellScriptEdition::Edition2027);
    for source in [ORDINARY_LOCK.to_string(), ORDINARY_LOCK.replacen("    verification\n", "", 1)] {
        assert_current_lock_group(&compile_lock(&source, CellScriptEdition::Edition2027), &foreign);
    }
}

#[test]
fn native_protected_lock_uses_current_group_at_nonzero_transaction_index() {
    let foreign = compile_lock(FOREIGN_LOCK, CellScriptEdition::Edition2027);
    let protected = compile_lock(NATIVE_LOCK, CellScriptEdition::Edition2027);
    assert_current_lock_group(&protected, &foreign);
}

#[test]
fn protected_role_rejects_ambiguous_same_hash_lock_and_type() {
    for (source, edition) in [(ORDINARY_LOCK, CellScriptEdition::Edition2027), (NATIVE_LOCK, CellScriptEdition::Edition2027)] {
        let protected = compile_lock(source, edition);
        let mut context = Context::new_with_deterministic_rng();
        let code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&protected.artifact_bytes)));
        let script = context.build_script(&code, Bytes::new()).unwrap();
        let cell = packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(100_000_000_000u64.pack())
            .lock(script.clone())
            .type_(Some(script).pack())
            .build();
        let data = Bytes::copy_from_slice(&7u64.to_le_bytes());
        let out_point = context.create_cell(cell.clone(), data.clone());
        let tx = context.complete_tx(
            TransactionBuilder::default()
                .input(packed::CellInput::new_builder().previous_output(out_point).build())
                .output(cell)
                .output_data(data.pack())
                .build(),
        );
        let error = context.verify_tx(&tx, 10_000_000).expect_err("same-hash role ambiguity must reject despite a valid value");
        let error = format!("{error:?}");
        let code = cellscript::runtime_errors::CellScriptRuntimeError::ScriptRoleMismatch.code();
        assert!(error.contains(&format!("error code {code}")) || error.contains(&format!("error code: {code}")), "{error}");
    }
}
